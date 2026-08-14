use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use ignore::WalkBuilder;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use crate::devices;
use crate::error::AppError;
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
}
