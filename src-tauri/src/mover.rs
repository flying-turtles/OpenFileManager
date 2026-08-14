use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use ignore::WalkBuilder;
use tauri::ipc::Channel;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use crate::db::{self, DbPool};
use crate::devices;
use crate::error::AppError;
use crate::hasher;
use crate::models::*;

/// Destination sub-directory for a file, from its modified timestamp
/// ("2026-03-14 09:12:33" -> "2026-03-14").
pub fn date_dir(modified_at: Option<&str>) -> String {
    modified_at
        .and_then(|s| s.get(..10))
        .filter(|s| s.len() == 10)
        .unwrap_or("unknown")
        .to_string()
}

/// Pick a free destination path, suffixing `_1`, `_2`, ... on collision.
/// Collisions are checked against both the filesystem (`exists`) and the
/// paths already handed out for this plan (`taken`), so two sources with the
/// same name cannot be assigned the same destination.
pub fn assign_dest(
    dest_dir: &Path,
    file_name: &str,
    taken: &mut HashSet<PathBuf>,
    exists: &dyn Fn(&Path) -> bool,
) -> PathBuf {
    let stem = Path::new(file_name)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let ext = Path::new(file_name)
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();

    let mut candidate = dest_dir.join(file_name);
    let mut counter = 1u32;
    while taken.contains(&candidate) || exists(&candidate) {
        candidate = dest_dir.join(format!("{}_{}{}", stem, counter, ext));
        counter += 1;
    }
    taken.insert(candidate.clone());
    candidate
}

/// Reject move roots that would eat their own source.
pub fn validate_roots(sources: &[PathBuf], dest: &Path) -> Result<(), AppError> {
    if sources.is_empty() {
        return Err(AppError::General("No source selected".into()));
    }
    for src in sources {
        if dest == src || dest.starts_with(src) {
            return Err(AppError::General(format!(
                "Destination {} is inside the source {}",
                dest.display(),
                src.display()
            )));
        }
    }
    Ok(())
}

/// Walk the selected sources and assign every file a dated destination.
/// Blocking filesystem work — call from `spawn_blocking`.
pub fn build_plan(
    sources: Vec<PathBuf>,
    dest: PathBuf,
    volumes: &[DetectedDisk],
) -> Result<MovePlan, AppError> {
    validate_roots(&sources, &dest)?;

    let dest_str = dest.to_string_lossy().to_string();
    let (dest_device_id, dest_mount) = devices::device_for_path(volumes, &dest_str)
        .ok_or_else(|| AppError::General(format!("No device for destination: {}", dest_str)))?;
    let dest_label = volumes
        .iter()
        .find(|v| v.id == dest_device_id)
        .map(|v| v.label.clone())
        .unwrap_or_else(|| "Destination".to_string());

    let mut source_device: Option<(String, String)> = None;
    for src in &sources {
        let src_str = src.to_string_lossy().to_string();
        let resolved = devices::device_for_path(volumes, &src_str)
            .ok_or_else(|| AppError::General(format!("No device for source: {}", src_str)))?;
        match &source_device {
            None => source_device = Some(resolved),
            Some(existing) if existing.0 != resolved.0 => {
                return Err(AppError::General(
                    "All sources must be on the same device".into(),
                ))
            }
            _ => {}
        }
    }
    let (source_device_id, source_mount) = source_device
        .ok_or_else(|| AppError::General("No source selected".into()))?;

    let same_volume = source_device_id == dest_device_id;
    let mut taken: HashSet<PathBuf> = HashSet::new();
    let mut files = Vec::new();
    let mut total_bytes: i64 = 0;

    for src in &sources {
        let walked: Vec<PathBuf> = WalkBuilder::new(src)
            .hidden(true)
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .add_custom_ignore_filename(".openfileignore")
            .build()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map_or(false, |ft| ft.is_file()))
            .map(|e| e.into_path())
            .collect();

        for path in walked {
            let metadata = match std::fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let file_size = metadata.len() as i64;

            let modified_at = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .and_then(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string());

            let file_name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            let dest_dir = dest.join(date_dir(modified_at.as_deref()));
            let dest_path = assign_dest(&dest_dir, &file_name, &mut taken, &|p| p.exists());

            total_bytes += file_size;
            files.push(MoveFile {
                source_path: path.to_string_lossy().to_string(),
                dest_path: dest_path.to_string_lossy().to_string(),
                file_name,
                file_size,
                modified_at,
                same_volume,
            });
        }
    }

    let total_files = files.len() as u64;
    let same_volume_count = if same_volume { total_files } else { 0 };

    Ok(MovePlan {
        files,
        total_files,
        total_bytes,
        source_device_id,
        source_mount,
        source_roots: sources
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect(),
        dest_device_id,
        dest_mount,
        dest_label,
        same_volume_count,
    })
}

/// Hashes produced while streaming a file to its destination.
pub struct CopyOutcome {
    /// blake3 over every byte — the verification hash.
    pub full_hash: String,
    /// blake3 over the first 4 MB — the index's `blake3_hash` identity.
    pub partial_hash: String,
    pub bytes: i64,
}

const PARTIAL_WINDOW: u64 = 4 * 1024 * 1024;

/// Copy `src` to `dest`, hashing the source stream as it goes so the source
/// is read exactly once. Returns `Ok(None)` if cancelled, after removing the
/// partial destination file.
pub async fn copy_hashing(
    src: &Path,
    dest: &Path,
    cancel: &CancellationToken,
    on_bytes: &mut dyn FnMut(i64),
) -> Result<Option<CopyOutcome>, AppError> {
    let reader = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::fs::File::open(src),
    )
    .await
    .map_err(|_| AppError::General(format!("Timeout opening {}", src.display())))??;

    let mut reader = tokio::io::BufReader::with_capacity(1024 * 1024, reader);
    let mut writer = tokio::fs::File::create(dest).await?;

    let mut full = blake3::Hasher::new();
    let mut partial = blake3::Hasher::new();
    let mut partial_remaining = PARTIAL_WINDOW;
    let mut buf = vec![0u8; 1024 * 1024];
    let mut bytes: i64 = 0;

    loop {
        let n = tokio::select! {
            _ = cancel.cancelled() => {
                drop(writer);
                let _ = tokio::fs::remove_file(dest).await;
                return Ok(None);
            }
            result = reader.read(&mut buf) => result?,
        };
        if n == 0 {
            break;
        }

        writer.write_all(&buf[..n]).await?;
        full.update(&buf[..n]);
        if partial_remaining > 0 {
            let take = (partial_remaining as usize).min(n);
            partial.update(&buf[..take]);
            partial_remaining -= take as u64;
        }
        bytes += n as i64;
        on_bytes(n as i64);
    }

    // sync_all forces the SMB server to acknowledge the write — a silent
    // finalize failure would leave an index row for bytes that never landed.
    writer.flush().await?;
    writer.sync_all().await?;

    Ok(Some(CopyOutcome {
        full_hash: full.finalize().to_hex().to_string(),
        partial_hash: partial.finalize().to_hex().to_string(),
        bytes,
    }))
}

/// Remove directories left empty by a move, depth first. Returns true when
/// `root` itself was removed. Files and non-empty directories are untouched.
/// Blocking filesystem work — call from `spawn_blocking`.
pub fn remove_empty_dirs(root: &Path) -> bool {
    // Use symlink_metadata to check if it's a directory WITHOUT following symlinks.
    let metadata = match std::fs::symlink_metadata(root) {
        Ok(m) => m,
        Err(_) => return false,
    };
    if !metadata.is_dir() {
        return false;
    }
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return false,
    };
    let mut empty = true;
    for entry_result in entries {
        // Treat entry read errors as making the parent non-empty.
        let entry = match entry_result {
            Ok(e) => e,
            Err(_) => {
                empty = false;
                continue;
            }
        };
        let path = entry.path();
        // Use symlink_metadata to not follow symlinks. Symlinks (even to dirs)
        // are treated as non-directory entries.
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_dir() => {
                if !remove_empty_dirs(&path) {
                    empty = false;
                }
            }
            Ok(_) => {
                // File or symlink: parent is not empty.
                empty = false;
            }
            Err(_) => {
                // Unreadable entry: conservatively treat as non-empty.
                empty = false;
            }
        }
    }
    if empty {
        return std::fs::remove_dir(root).is_ok();
    }
    false
}

/// Execute a plan: move each file, verify it landed, delete the source, and
/// rewrite the index. Per-file failures are collected, never fatal.
pub async fn run_move(
    pool: DbPool,
    plan: Arc<MovePlan>,
    permanent: bool,
    channel: Channel<MoveEvent>,
    cancel: CancellationToken,
) -> Result<MoveResult, AppError> {
    let total = plan.total_files;
    let total_bytes = plan.total_bytes;
    let mut moved: u64 = 0;
    let mut bytes_moved: i64 = 0;
    let mut failed: Vec<MoveError> = Vec::new();
    let mut processed: u64 = 0;

    for file in &plan.files {
        if cancel.is_cancelled() {
            let _ = channel.send(MoveEvent::Cancelled);
            return Ok(MoveResult { moved, bytes_moved, failed });
        }

        let source = PathBuf::from(&file.source_path);
        let dest = PathBuf::from(&file.dest_path);

        let fail = |failed: &mut Vec<MoveError>, msg: String| {
            let err = MoveError {
                source_path: file.source_path.clone(),
                file_name: file.file_name.clone(),
                error: msg,
            };
            let _ = channel.send(MoveEvent::FileFailed(err.clone()));
            failed.push(err);
        };

        let _ = channel.send(MoveEvent::Progress {
            processed,
            total,
            bytes_moved,
            total_bytes,
            current_file: file.file_name.clone(),
            phase: "copying".into(),
        });

        if let Some(parent) = dest.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                fail(&mut failed, format!("mkdir {}: {}", parent.display(), e));
                processed += 1;
                continue;
            }
        }

        // Same-volume moves rename instead of copying: no bytes cross a
        // device boundary, so there is nothing to hash-verify.
        let mut renamed = false;
        if file.same_volume {
            if tokio::fs::rename(&source, &dest).await.is_ok() {
                match tokio::fs::metadata(&dest).await {
                    Ok(m) if m.len() as i64 == file.file_size => renamed = true,
                    Ok(m) => {
                        fail(
                            &mut failed,
                            format!("size mismatch after rename: {} vs {}", m.len(), file.file_size),
                        );
                        processed += 1;
                        continue;
                    }
                    Err(e) => {
                        fail(&mut failed, format!("stat after rename: {}", e));
                        processed += 1;
                        continue;
                    }
                }
            }
            // A rename failure (EXDEV, permissions) falls through to the copy path.
        }

        let (full_hash, partial_hash) = if renamed {
            // Hash the file at its new home so the index carries both hashes.
            let full = match hasher::hash_file_full(&dest).await {
                Ok(h) => h,
                Err(e) => {
                    fail(&mut failed, format!("hash after rename: {}", e));
                    processed += 1;
                    continue;
                }
            };
            let partial = match hasher::hash_file_partial(&dest, PARTIAL_WINDOW).await {
                Ok(h) => h,
                Err(e) => {
                    fail(&mut failed, format!("partial hash after rename: {}", e));
                    processed += 1;
                    continue;
                }
            };
            (full, partial)
        } else {
            let mut streamed: i64 = 0;
            let mut last = std::time::Instant::now();
            let outcome = {
                let channel = channel.clone();
                let file_name = file.file_name.clone();
                let mut on_bytes = |n: i64| {
                    streamed += n;
                    if last.elapsed() >= std::time::Duration::from_millis(500) {
                        last = std::time::Instant::now();
                        let _ = channel.send(MoveEvent::Progress {
                            processed,
                            total,
                            bytes_moved: bytes_moved + streamed,
                            total_bytes,
                            current_file: file_name.clone(),
                            phase: "copying".into(),
                        });
                    }
                };
                copy_hashing(&source, &dest, &cancel, &mut on_bytes).await
            };

            let outcome = match outcome {
                Ok(Some(o)) => o,
                Ok(None) => {
                    let _ = channel.send(MoveEvent::Cancelled);
                    return Ok(MoveResult { moved, bytes_moved, failed });
                }
                Err(e) => {
                    let _ = tokio::fs::remove_file(&dest).await;
                    fail(&mut failed, format!("copy: {}", e));
                    processed += 1;
                    continue;
                }
            };

            let _ = channel.send(MoveEvent::Progress {
                processed,
                total,
                bytes_moved: bytes_moved + outcome.bytes,
                total_bytes,
                current_file: file.file_name.clone(),
                phase: "verifying".into(),
            });

            let dest_hash = match hasher::hash_file_full(&dest).await {
                Ok(h) => h,
                Err(e) => {
                    let _ = tokio::fs::remove_file(&dest).await;
                    fail(&mut failed, format!("verify read: {}", e));
                    processed += 1;
                    continue;
                }
            };

            if dest_hash != outcome.full_hash {
                let _ = tokio::fs::remove_file(&dest).await;
                fail(
                    &mut failed,
                    "verification failed: destination hash differs from source".into(),
                );
                processed += 1;
                continue;
            }

            (outcome.full_hash, outcome.partial_hash)
        };

        // Source deletion only happens past this point — the destination is
        // verified (cross-volume) or was renamed in place (same volume).
        if !renamed {
            let _ = channel.send(MoveEvent::Progress {
                processed,
                total,
                bytes_moved,
                total_bytes,
                current_file: file.file_name.clone(),
                phase: "deleting".into(),
            });

            if let Err(e) = crate::commands::files::remove_from_disk(source.clone(), permanent).await
            {
                fail(&mut failed, format!("delete source: {}", e));
                processed += 1;
                continue;
            }
        }

        // Index: drop the old row, record the new one.
        let source_rel = source
            .strip_prefix(&plan.source_mount)
            .unwrap_or(&source)
            .to_string_lossy()
            .to_string();
        let dest_rel = dest
            .strip_prefix(&plan.dest_mount)
            .unwrap_or(&dest)
            .to_string_lossy()
            .to_string();
        let extension = dest
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();

        let _ = db::delete_location_by_device_and_path(&pool, &plan.source_device_id, &source_rel)
            .await;
        let _ = db::upsert_file(&pool, &partial_hash, file.file_size, &file.file_name, &extension)
            .await;
        let _ = db::upsert_location(
            &pool,
            &partial_hash,
            &plan.dest_device_id,
            &dest_rel,
            &file.file_name,
            file.file_size,
            file.modified_at.as_deref(),
            "move",
        )
        .await;
        let _ = db::set_full_hash_by_device_and_path(
            &pool,
            &plan.dest_device_id,
            &dest_rel,
            &full_hash,
        )
        .await;

        moved += 1;
        bytes_moved += file.file_size;
        processed += 1;

        let _ = channel.send(MoveEvent::Progress {
            processed,
            total,
            bytes_moved,
            total_bytes,
            current_file: file.file_name.clone(),
            phase: "copying".into(),
        });
    }

    // Directories the move emptied are swept away; anything still holding a
    // file (hidden files, failures) is left alone.
    let roots: Vec<PathBuf> = plan.source_roots.iter().map(PathBuf::from).collect();
    let _ = tokio::task::spawn_blocking(move || {
        for root in roots {
            remove_empty_dirs(&root);
        }
    })
    .await;

    let _ = db::cleanup_orphaned_files(&pool).await;

    let result = MoveResult { moved, bytes_moved, failed };
    let _ = channel.send(MoveEvent::Complete(result.clone()));
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DetectedDisk;

    fn disk(id: &str, mount: &str) -> DetectedDisk {
        DetectedDisk {
            id: id.to_string(),
            label: format!("{} label", id),
            mount_point: mount.to_string(),
            total_bytes: 0,
            available_bytes: 0,
            is_removable: false,
        }
    }

    #[test]
    fn build_plan_walks_a_folder_and_dates_the_destination() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let src = root.join("src/Shoot");
        std::fs::create_dir_all(src.join("RAW")).unwrap();
        std::fs::write(src.join("RAW/a.cr3"), b"hello").unwrap();
        let dest = root.join("dst");
        std::fs::create_dir_all(&dest).unwrap();

        let volumes = vec![disk("vol-1", root.to_str().unwrap())];
        let plan = build_plan(vec![src.clone()], dest.clone(), &volumes).unwrap();

        assert_eq!(plan.total_files, 1);
        assert_eq!(plan.total_bytes, 5);
        assert_eq!(plan.source_device_id, "vol-1");
        assert_eq!(plan.dest_device_id, "vol-1");
        assert_eq!(plan.same_volume_count, 1);
        assert!(plan.files[0].same_volume);
        assert_eq!(plan.files[0].file_name, "a.cr3");
        // Nested under a date dir directly beneath dest — no "RAW" level.
        let rel = PathBuf::from(&plan.files[0].dest_path);
        let rel = rel.strip_prefix(&dest).unwrap();
        let mut parts = rel.components();
        let date = parts.next().unwrap().as_os_str().to_string_lossy().to_string();
        assert_eq!(date.len(), 10, "expected a YYYY-MM-DD dir, got {}", date);
        assert_eq!(
            parts.next().unwrap().as_os_str().to_string_lossy(),
            "a.cr3"
        );
        assert!(parts.next().is_none(), "structure must be flattened");
    }

    #[test]
    fn build_plan_accepts_a_single_file_source() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        let file = root.join("src/one.mov");
        std::fs::write(&file, b"xy").unwrap();
        let dest = root.join("dst");
        std::fs::create_dir_all(&dest).unwrap();

        let volumes = vec![disk("vol-1", root.to_str().unwrap())];
        let plan = build_plan(vec![file], dest, &volumes).unwrap();

        assert_eq!(plan.total_files, 1);
        assert_eq!(plan.files[0].file_name, "one.mov");
    }

    #[test]
    fn build_plan_marks_a_cross_volume_move() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("a")).unwrap();
        std::fs::create_dir_all(root.join("b")).unwrap();
        std::fs::write(root.join("a/x.jpg"), b"z").unwrap();

        let volumes = vec![
            disk("vol-a", root.join("a").to_str().unwrap()),
            disk("vol-b", root.join("b").to_str().unwrap()),
        ];
        let plan = build_plan(vec![root.join("a/x.jpg")], root.join("b"), &volumes).unwrap();

        assert_eq!(plan.source_device_id, "vol-a");
        assert_eq!(plan.dest_device_id, "vol-b");
        assert_eq!(plan.same_volume_count, 0);
        assert!(!plan.files[0].same_volume);
    }

    #[test]
    fn build_plan_rejects_sources_on_different_devices() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("a")).unwrap();
        std::fs::create_dir_all(root.join("b")).unwrap();
        std::fs::create_dir_all(root.join("c")).unwrap();
        std::fs::write(root.join("a/x.jpg"), b"z").unwrap();
        std::fs::write(root.join("b/y.jpg"), b"z").unwrap();

        let volumes = vec![
            disk("vol-a", root.join("a").to_str().unwrap()),
            disk("vol-b", root.join("b").to_str().unwrap()),
            disk("vol-c", root.join("c").to_str().unwrap()),
        ];
        let err = build_plan(
            vec![root.join("a/x.jpg"), root.join("b/y.jpg")],
            root.join("c"),
            &volumes,
        )
        .unwrap_err();
        assert!(err.to_string().contains("same device"));
    }

    #[test]
    fn build_plan_rejects_an_unknown_device() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/x.jpg"), b"z").unwrap();
        let err = build_plan(vec![root.join("src/x.jpg")], root.join("dst"), &[]).unwrap_err();
        assert!(err.to_string().contains("No device"));
    }

    #[test]
    fn date_dir_takes_the_date_part() {
        assert_eq!(date_dir(Some("2026-03-14 09:12:33")), "2026-03-14");
    }

    #[test]
    fn date_dir_falls_back_to_unknown() {
        assert_eq!(date_dir(None), "unknown");
        assert_eq!(date_dir(Some("short")), "unknown");
    }

    #[test]
    fn assign_dest_uses_the_plain_name_when_free() {
        let mut taken = HashSet::new();
        let got = assign_dest(Path::new("/d/2026-03-14"), "a.cr3", &mut taken, &|_| false);
        assert_eq!(got, PathBuf::from("/d/2026-03-14/a.cr3"));
    }

    #[test]
    fn assign_dest_suffixes_around_an_existing_file() {
        let mut taken = HashSet::new();
        let exists = |p: &Path| p == Path::new("/d/2026-03-14/a.cr3");
        let got = assign_dest(Path::new("/d/2026-03-14"), "a.cr3", &mut taken, &exists);
        assert_eq!(got, PathBuf::from("/d/2026-03-14/a_1.cr3"));
    }

    #[test]
    fn assign_dest_suffixes_around_names_taken_earlier_in_the_plan() {
        let mut taken = HashSet::new();
        let first = assign_dest(Path::new("/d/2026-03-14"), "a.cr3", &mut taken, &|_| false);
        let second = assign_dest(Path::new("/d/2026-03-14"), "a.cr3", &mut taken, &|_| false);
        assert_eq!(first, PathBuf::from("/d/2026-03-14/a.cr3"));
        assert_eq!(second, PathBuf::from("/d/2026-03-14/a_1.cr3"));
    }

    #[test]
    fn assign_dest_keeps_extensionless_names_intact() {
        let mut taken = HashSet::new();
        let got = assign_dest(Path::new("/d/unknown"), "README", &mut taken, &|_| false);
        assert_eq!(got, PathBuf::from("/d/unknown/README"));
    }

    #[test]
    fn validate_roots_rejects_dest_inside_a_source() {
        let err = validate_roots(
            &[PathBuf::from("/Volumes/SD/Shoot")],
            Path::new("/Volumes/SD/Shoot/Sub"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("inside"));
    }

    #[test]
    fn validate_roots_rejects_dest_equal_to_a_source() {
        let err = validate_roots(
            &[PathBuf::from("/Volumes/SD/Shoot")],
            Path::new("/Volumes/SD/Shoot"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("inside"));
    }

    #[test]
    fn validate_roots_rejects_an_empty_source_list() {
        let err = validate_roots(&[], Path::new("/Volumes/Target")).unwrap_err();
        assert!(err.to_string().contains("No source"));
    }

    #[test]
    fn validate_roots_accepts_a_sibling_destination() {
        assert!(validate_roots(
            &[PathBuf::from("/Volumes/SD/Shoot")],
            Path::new("/Volumes/SD/Archive"),
        )
        .is_ok());
    }

    #[tokio::test]
    async fn copy_hashing_writes_the_file_and_reports_both_hashes() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.bin");
        let dst = tmp.path().join("dst.bin");
        let payload = vec![7u8; 5 * 1024 * 1024]; // larger than the 4MB partial window
        std::fs::write(&src, &payload).unwrap();

        let token = CancellationToken::new();
        let mut seen: i64 = 0;
        let outcome = copy_hashing(&src, &dst, &token, &mut |n| seen += n)
            .await
            .unwrap()
            .expect("not cancelled");

        assert_eq!(outcome.bytes, payload.len() as i64);
        assert_eq!(seen, payload.len() as i64);
        assert_eq!(std::fs::read(&dst).unwrap(), payload);

        let expected_full = crate::hasher::hash_file_full_sync(&src).unwrap();
        let expected_partial =
            crate::hasher::hash_file_partial_sync(&src, 4 * 1024 * 1024).unwrap();
        assert_eq!(outcome.full_hash, expected_full);
        assert_eq!(outcome.partial_hash, expected_partial);
        assert_ne!(
            outcome.full_hash, outcome.partial_hash,
            "a >4MB file must hash differently in full and partial form"
        );
    }

    #[tokio::test]
    async fn copy_hashing_removes_the_destination_when_cancelled() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.bin");
        let dst = tmp.path().join("dst.bin");
        std::fs::write(&src, vec![1u8; 4 * 1024 * 1024]).unwrap();

        let token = CancellationToken::new();
        token.cancel();
        let outcome = copy_hashing(&src, &dst, &token, &mut |_| {}).await.unwrap();

        assert!(outcome.is_none());
        assert!(!dst.exists(), "partial destination must not survive a cancel");
    }

    #[tokio::test]
    async fn verified_destination_matches_the_source_hash() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.bin");
        let dst = tmp.path().join("dst.bin");
        std::fs::write(&src, b"payload bytes").unwrap();

        let token = CancellationToken::new();
        let outcome = copy_hashing(&src, &dst, &token, &mut |_| {})
            .await
            .unwrap()
            .unwrap();
        let dest_hash = crate::hasher::hash_file_full(&dst).await.unwrap();
        assert_eq!(dest_hash, outcome.full_hash);
    }

    #[tokio::test]
    async fn a_corrupted_destination_fails_verification() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.bin");
        let dst = tmp.path().join("dst.bin");
        std::fs::write(&src, b"payload bytes").unwrap();

        let token = CancellationToken::new();
        let outcome = copy_hashing(&src, &dst, &token, &mut |_| {})
            .await
            .unwrap()
            .unwrap();
        std::fs::write(&dst, b"payload bytez").unwrap();

        let dest_hash = crate::hasher::hash_file_full(&dst).await.unwrap();
        assert_ne!(dest_hash, outcome.full_hash);
    }

    #[test]
    fn remove_empty_dirs_clears_a_fully_emptied_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Shoot");
        std::fs::create_dir_all(root.join("RAW/Sub")).unwrap();

        assert!(remove_empty_dirs(&root));
        assert!(!root.exists());
    }

    #[test]
    fn remove_empty_dirs_keeps_directories_that_still_hold_files() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Shoot");
        std::fs::create_dir_all(root.join("RAW")).unwrap();
        std::fs::create_dir_all(root.join("Empty")).unwrap();
        std::fs::write(root.join("RAW/left.cr3"), b"x").unwrap();

        assert!(!remove_empty_dirs(&root));
        assert!(root.join("RAW/left.cr3").exists());
        assert!(!root.join("Empty").exists(), "empty branches still go");
    }

    #[test]
    fn remove_empty_dirs_ignores_a_file_root() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("one.mov");
        std::fs::write(&file, b"x").unwrap();

        assert!(!remove_empty_dirs(&file));
        assert!(file.exists());
    }

    #[test]
    fn remove_empty_dirs_does_not_follow_symlinks_to_delete_outside_tree() {
        use std::os::unix::fs::symlink;

        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Create an external tree with an empty child.
        let outside = root.join("outside");
        let empty_child = outside.join("empty-child");
        std::fs::create_dir_all(&empty_child).unwrap();

        // Create a source tree with a symlink pointing to the outside tree.
        let source = root.join("source");
        std::fs::create_dir(&source).unwrap();
        symlink(&outside, source.join("link-to-outside")).unwrap();

        // Remove empty dirs from source tree.
        assert!(!remove_empty_dirs(&source));

        // The external empty-child must still exist — it should not be deleted
        // even though it was reachable through the symlink.
        assert!(
            empty_child.exists(),
            "external directory must not be deleted through symlink"
        );
        // The symlink itself must still exist.
        assert!(source.join("link-to-outside").exists());
    }

    // Mirrors the existing helper in db.rs — init_pool sets WAL journal mode
    // and creates the parent dir, so it needs a real file, not ":memory:".
    async fn test_pool() -> crate::db::DbPool {
        let dir = std::env::temp_dir().join(format!("ofm-move-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let pool = crate::db::init_pool(&dir.join("test.db")).await.unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        pool
    }

    fn plan_for(files: Vec<MoveFile>, source_mount: &str, dest_mount: &str) -> Arc<MovePlan> {
        let total_bytes = files.iter().map(|f| f.file_size).sum();
        Arc::new(MovePlan {
            total_files: files.len() as u64,
            total_bytes,
            files,
            source_device_id: "vol-src".into(),
            source_mount: source_mount.to_string(),
            source_roots: vec![],
            dest_device_id: "vol-dst".into(),
            dest_mount: dest_mount.to_string(),
            dest_label: "Target".into(),
            same_volume_count: 0,
        })
    }

    fn test_channel() -> Channel<MoveEvent> {
        Channel::new(|_event: tauri::ipc::InvokeResponseBody| Ok(()))
    }

    #[tokio::test]
    async fn run_move_copies_verifies_deletes_and_reindexes() {
        let tmp = tempfile::tempdir().unwrap();
        let src_mount = tmp.path().join("src");
        let dst_mount = tmp.path().join("dst");
        std::fs::create_dir_all(&src_mount).unwrap();
        std::fs::create_dir_all(&dst_mount).unwrap();
        let src_file = src_mount.join("a.cr3");
        std::fs::write(&src_file, b"the payload").unwrap();

        let pool = test_pool().await;
        // file_locations.device_id has a FOREIGN KEY into storage_devices, and
        // sqlx enables foreign-key enforcement by default: the index rewrite
        // silently no-ops without a device row to point at.
        db::upsert_device(&pool, &disk("vol-src", src_mount.to_str().unwrap()))
            .await
            .unwrap();
        db::upsert_device(&pool, &disk("vol-dst", dst_mount.to_str().unwrap()))
            .await
            .unwrap();
        let plan = plan_for(
            vec![MoveFile {
                source_path: src_file.to_string_lossy().to_string(),
                dest_path: dst_mount
                    .join("2026-03-14/a.cr3")
                    .to_string_lossy()
                    .to_string(),
                file_name: "a.cr3".into(),
                file_size: 11,
                modified_at: Some("2026-03-14 09:00:00".into()),
                same_volume: false,
            }],
            src_mount.to_str().unwrap(),
            dst_mount.to_str().unwrap(),
        );

        let channel = test_channel();
        let result = run_move(pool.clone(), plan, true, channel, CancellationToken::new())
            .await
            .unwrap();

        assert_eq!(result.moved, 1);
        assert_eq!(result.bytes_moved, 11);
        assert!(result.failed.is_empty());
        assert!(!src_file.exists(), "source must be gone after a verified move");
        assert_eq!(
            std::fs::read(dst_mount.join("2026-03-14/a.cr3")).unwrap(),
            b"the payload"
        );

        let locs = crate::db::get_files_on_device(&pool, "vol-dst").await.unwrap();
        assert_eq!(locs.len(), 1);
        assert_eq!(locs[0].file_path, "2026-03-14/a.cr3");
    }

    #[tokio::test]
    async fn run_move_keeps_the_source_when_the_destination_cannot_be_written() {
        let tmp = tempfile::tempdir().unwrap();
        let src_mount = tmp.path().join("src");
        let dst_mount = tmp.path().join("dst");
        std::fs::create_dir_all(&src_mount).unwrap();
        std::fs::create_dir_all(&dst_mount).unwrap();
        let src_file = src_mount.join("a.cr3");
        std::fs::write(&src_file, b"payload").unwrap();

        // A regular file where the date dir needs to be: create_dir_all fails.
        std::fs::write(dst_mount.join("2026-03-14"), b"blocker").unwrap();

        let pool = test_pool().await;
        let plan = plan_for(
            vec![MoveFile {
                source_path: src_file.to_string_lossy().to_string(),
                dest_path: dst_mount
                    .join("2026-03-14/a.cr3")
                    .to_string_lossy()
                    .to_string(),
                file_name: "a.cr3".into(),
                file_size: 7,
                modified_at: Some("2026-03-14 09:00:00".into()),
                same_volume: false,
            }],
            src_mount.to_str().unwrap(),
            dst_mount.to_str().unwrap(),
        );

        let channel = test_channel();
        let result = run_move(pool, plan, true, channel, CancellationToken::new())
            .await
            .unwrap();

        assert_eq!(result.moved, 0);
        assert_eq!(result.failed.len(), 1);
        assert!(src_file.exists(), "a failed move must never delete the source");
    }

    #[tokio::test]
    async fn run_move_renames_within_one_volume() {
        let tmp = tempfile::tempdir().unwrap();
        let mount = tmp.path().join("vol");
        std::fs::create_dir_all(mount.join("in")).unwrap();
        let src_file = mount.join("in/a.cr3");
        std::fs::write(&src_file, b"payload").unwrap();

        let pool = test_pool().await;
        db::upsert_device(&pool, &disk("vol-src", mount.to_str().unwrap()))
            .await
            .unwrap();
        let mut plan = plan_for(
            vec![MoveFile {
                source_path: src_file.to_string_lossy().to_string(),
                dest_path: mount.join("out/2026-03-14/a.cr3").to_string_lossy().to_string(),
                file_name: "a.cr3".into(),
                file_size: 7,
                modified_at: Some("2026-03-14 09:00:00".into()),
                same_volume: true,
            }],
            mount.to_str().unwrap(),
            mount.to_str().unwrap(),
        );
        Arc::get_mut(&mut plan).unwrap().dest_device_id = "vol-src".into();

        let channel = test_channel();
        let result = run_move(pool, plan, true, channel, CancellationToken::new())
            .await
            .unwrap();

        assert_eq!(result.moved, 1);
        assert!(!src_file.exists());
        assert!(mount.join("out/2026-03-14/a.cr3").exists());
    }
}
