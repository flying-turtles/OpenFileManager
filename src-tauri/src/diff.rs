use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use tauri::ipc::Channel;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::models::*;
use crate::models::FileLocation;

/// How many probes may be in flight at once. Enough to hide per-call latency
/// on a spinning disk, small enough that a stalled mount cannot swallow the
/// blocking pool.
const PROBE_CONCURRENCY: usize = 8;

/// Per-probe deadline. A `stat` that has not answered in this long is treated
/// as unreadable, never as a deletion.
const PROBE_TIMEOUT_SECS: u64 = 5;

/// What the filesystem says about one indexed path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    Present,
    /// `NotFound` — the only outcome read as "the user deleted this".
    Gone,
    /// Timeout, permission or IO error. Never treated as a deletion.
    Unknown,
}

/// One indexed copy of a project file plus its observed state on disk.
#[derive(Debug, Clone)]
pub struct ProbedLocation {
    pub location_id: i64,
    pub device_id: String,
    pub file_path: String,
    pub file_name: String,
    pub file_size: i64,
    pub blake3_hash: String,
    /// Carried through from the index so a copy can be written with the same
    /// mtime it had on the source of truth.
    pub modified_at: Option<String>,
    pub presence: Presence,
}

#[derive(Debug, Default)]
pub struct ClassifyOutput {
    /// (target device id, entry) pairs.
    pub delete: Vec<(String, DiffFileEntry)>,
    pub copy: Vec<(String, DiffFileEntry)>,
    pub purge_location_ids: Vec<i64>,
    pub unreadable: Vec<DiffUnreadable>,
}

/// Turns probed locations into the four diff buckets.
///
/// Grouping is by `(blake3_hash, file_size)`: `blake3_hash` only covers the
/// first 4 MB, so two files sharing a header are the same hash but not the
/// same file.
///
/// Within a group the two directions use deliberately different rules:
///
/// * **Deleting** is evidence-driven and per *path*. A backup row is only a
///   delete candidate when the source of truth has an indexed row at the very
///   same relative path and that path probed `Gone`, *and* no source-of-truth
///   row in the group is still `Present`. The same content living at two
///   paths on the source of truth (a Lightroom cull that rejects `A/x.cr3`
///   and keeps `B/x.cr3`) must never take the backup's surviving copy with
///   it. A backup copy at a relative path the source of truth never indexed
///   is left alone: there is no evidence about that path either way.
/// * **Copying** is per *content*. A backup that already holds this content
///   anywhere is in sync; proposing a second copy just because the source of
///   truth keeps two would multiply files on the backups without protecting
///   anything new.
pub fn classify(
    probed: &[ProbedLocation],
    sot_device_id: &str,
    sot_mount: &str,
    backup_device_ids: &[String],
) -> ClassifyOutput {
    let mut groups: HashMap<(&str, i64), Vec<&ProbedLocation>> = HashMap::new();
    for loc in probed {
        groups
            .entry((loc.blake3_hash.as_str(), loc.file_size))
            .or_default()
            .push(loc);
    }

    let mut out = ClassifyOutput::default();

    // Deterministic output: the UI groups by device and path, but stable
    // ordering keeps tests and diffs readable.
    let mut keys: Vec<&(&str, i64)> = groups.keys().collect();
    keys.sort();

    for key in keys {
        let mut group: Vec<&ProbedLocation> = groups[key].clone();
        // Row order out of SQLite is rowid order, which would make every
        // choice below depend on insertion history. Sort so it does not.
        group.sort_by(|a, b| (&a.device_id, &a.file_path).cmp(&(&b.device_id, &b.file_path)));

        let (sot_rows, backup_rows): (Vec<&ProbedLocation>, Vec<&ProbedLocation>) = group
            .iter()
            .partition(|l| l.device_id == sot_device_id);

        // Never on the source of truth: not our business.
        if sot_rows.is_empty() {
            continue;
        }

        // Purging is per row and independent of everything else in the group:
        // a row pointing at a path confirmed gone from disk is wrong whatever
        // the rest of the group looks like.
        for l in sot_rows.iter().filter(|l| l.presence == Presence::Gone) {
            out.purge_location_ids.push(l.location_id);
        }

        for l in sot_rows.iter().filter(|l| l.presence == Presence::Unknown) {
            out.unreadable.push(DiffUnreadable {
                device_id: l.device_id.clone(),
                relative_path: l.file_path.clone(),
                error: "Could not read on the source of truth".to_string(),
            });
        }

        let sot_present: Vec<&&ProbedLocation> = sot_rows
            .iter()
            .filter(|l| l.presence == Presence::Present)
            .collect();
        let any_sot_unknown = sot_rows.iter().any(|l| l.presence == Presence::Unknown);
        let gone_paths: std::collections::HashSet<&str> = sot_rows
            .iter()
            .filter(|l| l.presence == Presence::Gone)
            .map(|l| l.file_path.as_str())
            .collect();

        if !sot_present.is_empty() {
            // The content still exists on the source of truth, so nothing in
            // this group may be deleted — not even a backup row whose own
            // path vanished, because that path's content is still protected
            // here. Only the copy direction applies.
            let sot = sot_present[0];
            for device_id in backup_device_ids {
                let rows: Vec<&&ProbedLocation> = backup_rows
                    .iter()
                    .filter(|l| l.device_id == *device_id)
                    .collect();
                if rows.iter().any(|l| l.presence == Presence::Present) {
                    continue;
                }
                if let Some(unknown) = rows.iter().find(|l| l.presence == Presence::Unknown) {
                    out.unreadable.push(DiffUnreadable {
                        device_id: device_id.clone(),
                        relative_path: unknown.file_path.clone(),
                        error: "Could not read on this device".to_string(),
                    });
                    continue;
                }
                // No row at all, or only rows whose files are gone.
                out.copy.push((
                    device_id.clone(),
                    DiffFileEntry {
                        blake3_hash: sot.blake3_hash.clone(),
                        file_size: sot.file_size,
                        file_name: sot.file_name.clone(),
                        relative_path: sot.file_path.clone(),
                        location_id: None,
                        source_path: Some(
                            Path::new(sot_mount)
                                .join(&sot.file_path)
                                .to_string_lossy()
                                .to_string(),
                        ),
                        modified_at: sot.modified_at.clone(),
                    },
                ));
            }
        } else if !any_sot_unknown && !gone_paths.is_empty() {
            // Every indexed copy on the source of truth is confirmed gone, so
            // the content itself was rejected. Delete backup copies that sit
            // at one of those confirmed-gone relative paths.
            for loc in &backup_rows {
                match loc.presence {
                    Presence::Present if gone_paths.contains(loc.file_path.as_str()) => {
                        out.delete.push((
                            loc.device_id.clone(),
                            DiffFileEntry {
                                blake3_hash: loc.blake3_hash.clone(),
                                file_size: loc.file_size,
                                file_name: loc.file_name.clone(),
                                relative_path: loc.file_path.clone(),
                                location_id: Some(loc.location_id),
                                source_path: None,
                                modified_at: loc.modified_at.clone(),
                            },
                        ))
                    }
                    // Present at a path the source of truth never indexed:
                    // no evidence that this path was rejected, so leave it.
                    Presence::Present => {}
                    Presence::Gone => {}
                    Presence::Unknown => out.unreadable.push(DiffUnreadable {
                        device_id: loc.device_id.clone(),
                        relative_path: loc.file_path.clone(),
                        error: "Could not read on this device".to_string(),
                    }),
                }
            }
        }
        // Otherwise the source of truth only told us "unknown": already
        // reported above, and neither direction may act on it.
    }

    out.purge_location_ids.sort_unstable();
    out
}

/// `stat` one path with a deadline.
///
/// Uses `symlink_metadata` so a dangling symlink reports as present — the
/// entry does exist, and deleting a backup because a link target moved would
/// be wrong.
pub async fn probe_path(path: PathBuf, secs: u64) -> Presence {
    let probe = tokio::task::spawn_blocking(move || std::fs::symlink_metadata(&path));
    match tokio::time::timeout(Duration::from_secs(secs), probe).await {
        Ok(Ok(Ok(_))) => Presence::Present,
        Ok(Ok(Err(e))) if e.kind() == std::io::ErrorKind::NotFound => Presence::Gone,
        _ => Presence::Unknown,
    }
}

/// A periodic mid-pass check that the source of truth is still the disk we
/// started against.
///
/// The probe pass can run for minutes. A volume that unmounts and comes back
/// inside one pass would satisfy a before-and-after check while every file
/// probed in the gap read `Gone` — a whole-project deletion proposal. The
/// engine stays free of database and device concerns, so the caller supplies
/// the check.
pub trait SotRecheck: Sync {
    /// `false` aborts the pass.
    fn still_mounted(&self) -> Pin<Box<dyn Future<Output = bool> + Send + '_>>;
}

/// How many probes may complete between two mid-pass source-of-truth checks.
///
/// The check costs one marker read plus a sample of `stat`s, all under the
/// same deadline as the mount gate, so it is cheap next to 250 probes on a
/// healthy disk. It is also fast exactly when it matters: an unmounted volume
/// answers `NotFound` immediately, so 250 probes elapse in milliseconds and
/// the drop is caught almost at once. The slow case — a stalled mount — makes
/// the window long, but there every probe reads `Unknown` rather than `Gone`,
/// so nothing is ever proposed for deletion.
const SOT_RECHECK_EVERY: u64 = 250;

/// The outcome of a probe pass.
pub struct ProbePass {
    pub probed: Vec<ProbedLocation>,
    /// Set when a mid-pass source-of-truth re-check failed. The results are
    /// unusable: everything probed after the drop reads as gone.
    pub sot_lost: bool,
}

/// Probes every location whose device has a known mount point.
///
/// Locations on devices absent from `mounts` are dropped: the caller has
/// already decided those devices are out of the diff, and a missing mount is
/// not evidence of a deletion.
pub async fn probe_locations(
    locations: Vec<FileLocation>,
    mounts: &HashMap<String, String>,
    cancel: &CancellationToken,
    channel: &Channel<DiffEvent>,
    recheck: Option<&dyn SotRecheck>,
) -> ProbePass {
    let in_scope: Vec<FileLocation> = locations
        .into_iter()
        .filter(|l| mounts.contains_key(&l.device_id))
        .collect();

    let total = in_scope.len() as u64;
    let _ = channel.send(DiffEvent::Started { total });

    let semaphore = Arc::new(Semaphore::new(PROBE_CONCURRENCY));
    let mut handles = Vec::with_capacity(in_scope.len());

    for loc in in_scope {
        if cancel.is_cancelled() {
            break;
        }
        let mount = mounts[&loc.device_id].clone();
        let semaphore = semaphore.clone();
        handles.push(tokio::spawn(async move {
            let _permit = semaphore.acquire_owned().await.ok()?;
            let full = Path::new(&mount).join(&loc.file_path);
            let presence = probe_path(full, PROBE_TIMEOUT_SECS).await;
            Some(ProbedLocation {
                location_id: loc.id,
                device_id: loc.device_id,
                file_path: loc.file_path,
                file_name: loc.file_name,
                file_size: loc.file_size,
                blake3_hash: loc.blake3_hash,
                modified_at: loc.modified_at,
                presence,
            })
        }));
    }

    let mut probed = Vec::with_capacity(handles.len());
    let mut checked: u64 = 0;
    let mut sot_lost = false;
    let mut stop_at: Option<usize> = None;

    for (i, handle) in handles.iter_mut().enumerate() {
        // Cancelling has to stop the drain too. The spawned probes are
        // cancellation-unaware, so awaiting the rest of them would leave the
        // user waiting out the whole remaining pass — against a stalled mount
        // that is `remaining / concurrency * timeout`, i.e. tens of minutes.
        if cancel.is_cancelled() {
            stop_at = Some(i);
            break;
        }
        if let Ok(Some(p)) = handle.await {
            checked += 1;
            if checked % 25 == 0 || checked == total {
                let _ = channel.send(DiffEvent::Progress {
                    checked,
                    total,
                    current_device: p.device_id.clone(),
                });
            }
            probed.push(p);
        }
        if checked > 0 && checked % SOT_RECHECK_EVERY == 0 {
            if let Some(r) = recheck {
                if !r.still_mounted().await {
                    sot_lost = true;
                    stop_at = Some(i + 1);
                    break;
                }
            }
        }
    }

    if let Some(from) = stop_at {
        for handle in &handles[from..] {
            handle.abort();
        }
    }

    ProbePass { probed, sot_lost }
}

use crate::db::{self, DbPool};
use crate::importer::copy_file_cancellable;

/// The mirrored destination for a copy: the source-of-truth relative path,
/// rooted at the target device's mount.
pub fn plan_copy_target(target_mount: &Path, relative_path: &str) -> PathBuf {
    target_mount.join(relative_path)
}

/// Whether a copy may write to a target in this state.
///
/// Only an unambiguous absence permits a write. `Present` is an occupied
/// target and `Unknown` means we could not tell — neither is permission.
pub fn may_write_target(presence: Presence) -> bool {
    presence == Presence::Gone
}

/// Whether a `relative_path` from IPC is safe to root under a target mount.
///
/// `PathBuf::join` silently discards the base when the joined path is
/// absolute, and never resolves `..`. Both would let a copy land outside the
/// target mount. Checked via `Component`, not string matching on "..", so a
/// filename that merely contains dots is not caught.
fn is_safe_relative_path(relative_path: &str) -> bool {
    let path = Path::new(relative_path);
    if path.is_absolute() {
        return false;
    }
    !path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
}

/// Copies each item to its target device, indexing what lands.
///
/// An occupied target is skipped, never overwritten — this feature only ever
/// adds files a backup is missing. `on_event` is called synchronously for
/// each progress/terminal event, decoupling this from the `Channel` IPC type
/// so it is directly testable.
pub async fn run_diff_copy(
    pool: &DbPool,
    items: Vec<DiffCopyItem>,
    mounts: &HashMap<String, String>,
    on_event: impl Fn(DiffCopyEvent),
    cancel: &CancellationToken,
) -> DiffCopyResult {
    let mut result = DiffCopyResult {
        copied: 0,
        bytes_copied: 0,
        skipped: Vec::new(),
        failed: Vec::new(),
    };

    let total_files = items.len() as u64;
    let total_bytes: i64 = items.iter().map(|i| i.file_size).sum();

    for item in items {
        if cancel.is_cancelled() {
            on_event(DiffCopyEvent::Cancelled);
            return result;
        }

        let mount = match mounts.get(&item.target_device_id) {
            Some(m) => m.clone(),
            None => {
                result.failed.push(DiffCopyError {
                    file_name: item.file_name.clone(),
                    target_device_id: item.target_device_id.clone(),
                    error: "Target device has no known mount point".to_string(),
                });
                continue;
            }
        };

        if !is_safe_relative_path(&item.relative_path) {
            result.failed.push(DiffCopyError {
                file_name: item.file_name.clone(),
                target_device_id: item.target_device_id.clone(),
                error: "Refused: relative path escapes the target mount".to_string(),
            });
            continue;
        }

        let dest = plan_copy_target(Path::new(&mount), &item.relative_path);

        on_event(DiffCopyEvent::Progress(DeviceCopyProgress {
            device_id: item.target_device_id.clone(),
            device_label: item.target_device_id.clone(),
            bytes_copied: result.bytes_copied,
            total_bytes,
            files_copied: result.copied as u64,
            total_files,
            current_file: item.file_name.clone(),
        }));

        let presence = probe_path(dest.clone(), PROBE_TIMEOUT_SECS).await;
        if !may_write_target(presence) {
            // `Present` and `Unknown` are different faults: an occupied
            // target is expected and benign, but an unreadable one hides a
            // real error and must not be reported as "already exists".
            match presence {
                Presence::Present => result.skipped.push(DiffCopyError {
                    file_name: item.file_name.clone(),
                    target_device_id: item.target_device_id.clone(),
                    error: "A file already exists at the target path".to_string(),
                }),
                _ => result.failed.push(DiffCopyError {
                    file_name: item.file_name.clone(),
                    target_device_id: item.target_device_id.clone(),
                    error: "Could not read the target path".to_string(),
                }),
            }
            continue;
        }

        if let Some(parent) = dest.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                result.failed.push(DiffCopyError {
                    file_name: item.file_name.clone(),
                    target_device_id: item.target_device_id.clone(),
                    error: format!("mkdir {}: {}", parent.display(), e),
                });
                continue;
            }
        }

        match copy_file_cancellable(&item.source_path, &dest, cancel).await {
            Ok(true) => {
                let extension = dest
                    .extension()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();

                // A file on the backup that the index does not know about is
                // invisible to the next diff, which proposes copying it again
                // and then reports "already exists". Report the write failure
                // per file, the way `mover` does, instead of counting the copy
                // as a clean success.
                if let Err(e) = db::upsert_file(
                    pool,
                    &item.blake3_hash,
                    item.file_size,
                    &item.file_name,
                    &extension,
                )
                .await
                {
                    result.failed.push(DiffCopyError {
                        file_name: item.file_name.clone(),
                        target_device_id: item.target_device_id.clone(),
                        error: format!(
                            "file copied to {} but could not be indexed: {}",
                            dest.display(),
                            e
                        ),
                    });
                    continue;
                }
                if let Err(e) = db::upsert_location(
                    pool,
                    &item.blake3_hash,
                    &item.target_device_id,
                    &item.relative_path,
                    &item.file_name,
                    item.file_size,
                    item.modified_at.as_deref(),
                    "diff-sync",
                )
                .await
                {
                    result.failed.push(DiffCopyError {
                        file_name: item.file_name.clone(),
                        target_device_id: item.target_device_id.clone(),
                        error: format!(
                            "file copied to {} but could not be indexed: {}",
                            dest.display(),
                            e
                        ),
                    });
                    continue;
                }

                result.copied += 1;
                result.bytes_copied += item.file_size;
            }
            Ok(false) => {
                on_event(DiffCopyEvent::Cancelled);
                return result;
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&dest).await;
                result.failed.push(DiffCopyError {
                    file_name: item.file_name.clone(),
                    target_device_id: item.target_device_id.clone(),
                    error: e.to_string(),
                });
            }
        }
    }

    on_event(DiffCopyEvent::Complete(result.clone()));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probed(
        id: i64,
        device: &str,
        path: &str,
        hash: &str,
        size: i64,
        presence: Presence,
    ) -> ProbedLocation {
        ProbedLocation {
            location_id: id,
            device_id: device.to_string(),
            file_path: path.to_string(),
            file_name: path.rsplit('/').next().unwrap().to_string(),
            file_size: size,
            blake3_hash: hash.to_string(),
            modified_at: Some("2026-08-14 10:00:00".to_string()),
            presence,
        }
    }

    #[test]
    fn copy_target_is_the_mirrored_relative_path_under_the_target_mount() {
        let target = plan_copy_target(Path::new("/Volumes/Backup"), "2026-08-14/RAW/a.cr3");
        assert_eq!(target, PathBuf::from("/Volumes/Backup/2026-08-14/RAW/a.cr3"));
    }

    #[test]
    fn a_copy_may_only_write_an_unambiguously_absent_target() {
        assert!(may_write_target(Presence::Gone));
        // An occupied target must never be overwritten...
        assert!(!may_write_target(Presence::Present));
        // ...and "we could not tell" is not permission to write either.
        assert!(!may_write_target(Presence::Unknown));
    }

    #[tokio::test]
    async fn an_occupied_target_is_refused_and_left_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let dst_mount = tmp.path().join("backup");
        std::fs::create_dir_all(dst_mount.join("2026-08-14")).unwrap();
        let occupied = plan_copy_target(&dst_mount, "2026-08-14/a.cr3");
        std::fs::write(&occupied, b"original").unwrap();

        let presence = probe_path(occupied.clone(), 5).await;
        assert_eq!(presence, Presence::Present);
        assert!(!may_write_target(presence), "the copy must be refused");
        assert_eq!(std::fs::read(&occupied).unwrap(), b"original");
    }

    #[test]
    fn an_absolute_relative_path_is_rejected() {
        assert!(!is_safe_relative_path("/etc/foo"));
    }

    #[test]
    fn a_relative_path_containing_parent_dir_is_rejected() {
        assert!(!is_safe_relative_path(
            "../../../Library/LaunchAgents/x.plist"
        ));
    }

    #[test]
    fn an_ordinary_relative_path_is_accepted() {
        assert!(is_safe_relative_path("2026-08-14/RAW/a.cr3"));
    }

    #[test]
    fn a_filename_that_merely_contains_dots_is_not_caught() {
        // Component-based checking, not string matching on "..".
        assert!(is_safe_relative_path("2026-08-14/RAW/a..b.cr3"));
    }

    async fn test_diff_pool() -> DbPool {
        let dir = tempfile::tempdir().unwrap();
        let pool = db::init_pool(&dir.path().join("test.db")).await.unwrap();
        db::run_migrations(&pool).await.unwrap();
        // Leak the tempdir so the backing file survives for the pool's life;
        // each test gets its own directory so this does not accumulate.
        std::mem::forget(dir);
        pool
    }

    /// Seeds a `storage_devices` row so inserts into `file_locations` (which
    /// carries a foreign key on `device_id`) succeed. Mirrors the real
    /// `copy_diff_files` command, whose `mounts` map is built from
    /// `db::get_all_devices` and so is always backed by a devices row.
    async fn seed_device(pool: &DbPool, id: &str, mount_point: &str) {
        db::upsert_device(
            pool,
            &DetectedDisk {
                id: id.to_string(),
                label: id.to_string(),
                mount_point: mount_point.to_string(),
                total_bytes: 0,
                available_bytes: 0,
                is_removable: false,
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn run_diff_copy_skips_an_occupied_target_and_leaves_it_untouched() {
        let pool = test_diff_pool().await;
        let tmp = tempfile::tempdir().unwrap();
        let src_dir = tmp.path().join("sot");
        let dst_dir = tmp.path().join("backup");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::create_dir_all(&dst_dir).unwrap();
        std::fs::write(src_dir.join("a.cr3"), b"new content").unwrap();
        std::fs::write(dst_dir.join("a.cr3"), b"original").unwrap();
        seed_device(&pool, "bk1", &dst_dir.to_string_lossy()).await;

        let items = vec![DiffCopyItem {
            blake3_hash: "h1".to_string(),
            file_size: 11,
            file_name: "a.cr3".to_string(),
            source_path: src_dir.join("a.cr3").to_string_lossy().into_owned(),
            target_device_id: "bk1".to_string(),
            relative_path: "a.cr3".to_string(),
            modified_at: Some("2026-08-14 10:00:00".to_string()),
        }];
        let mut mounts = HashMap::new();
        mounts.insert("bk1".to_string(), dst_dir.to_string_lossy().into_owned());
        let cancel = CancellationToken::new();

        let result = run_diff_copy(&pool, items, &mounts, |_| {}, &cancel).await;

        assert_eq!(result.copied, 0);
        assert_eq!(result.skipped.len(), 1);
        assert!(result.failed.is_empty());
        assert_eq!(std::fs::read(dst_dir.join("a.cr3")).unwrap(), b"original");
    }

    #[tokio::test]
    async fn run_diff_copy_copies_an_absent_target_and_indexes_it() {
        let pool = test_diff_pool().await;
        let tmp = tempfile::tempdir().unwrap();
        let src_dir = tmp.path().join("sot");
        let dst_dir = tmp.path().join("backup");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::create_dir_all(&dst_dir).unwrap();
        let content = b"raw photo bytes";
        std::fs::write(src_dir.join("a.cr3"), content).unwrap();
        seed_device(&pool, "bk1", &dst_dir.to_string_lossy()).await;

        let items = vec![DiffCopyItem {
            blake3_hash: "h1".to_string(),
            file_size: content.len() as i64,
            file_name: "a.cr3".to_string(),
            source_path: src_dir.join("a.cr3").to_string_lossy().into_owned(),
            target_device_id: "bk1".to_string(),
            relative_path: "2026-08-14/a.cr3".to_string(),
            modified_at: Some("2026-08-14 10:00:00".to_string()),
        }];
        let mut mounts = HashMap::new();
        mounts.insert("bk1".to_string(), dst_dir.to_string_lossy().into_owned());
        let cancel = CancellationToken::new();

        let result = run_diff_copy(&pool, items, &mounts, |_| {}, &cancel).await;

        assert_eq!(result.copied, 1);
        assert_eq!(result.bytes_copied, content.len() as i64);
        assert!(result.failed.is_empty());
        assert!(result.skipped.is_empty());
        assert_eq!(
            std::fs::read(dst_dir.join("2026-08-14/a.cr3")).unwrap(),
            content
        );

        let locations = db::get_files_on_device(&pool, "bk1").await.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].scan_mode, "diff-sync");
        assert_eq!(locations[0].file_path, "2026-08-14/a.cr3");
        // Project membership is MIN(modified_at) over a hash's rows. A NULL
        // here drops the file out of its project the moment the
        // source-of-truth row is purged.
        assert_eq!(
            locations[0].modified_at.as_deref(),
            Some("2026-08-14 10:00:00")
        );
    }

    #[tokio::test]
    async fn run_diff_copy_reports_a_file_it_could_not_index() {
        let pool = test_diff_pool().await;
        let tmp = tempfile::tempdir().unwrap();
        let src_dir = tmp.path().join("sot");
        let dst_dir = tmp.path().join("backup");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::create_dir_all(&dst_dir).unwrap();
        std::fs::write(src_dir.join("a.cr3"), b"raw photo bytes").unwrap();
        // No storage_devices row for "bk1", so upsert_location's FOREIGN KEY
        // on device_id fails.

        let items = vec![DiffCopyItem {
            blake3_hash: "h1".to_string(),
            file_size: 15,
            file_name: "a.cr3".to_string(),
            source_path: src_dir.join("a.cr3").to_string_lossy().into_owned(),
            target_device_id: "bk1".to_string(),
            relative_path: "a.cr3".to_string(),
            modified_at: Some("2026-08-14 10:00:00".to_string()),
        }];
        let mut mounts = HashMap::new();
        mounts.insert("bk1".to_string(), dst_dir.to_string_lossy().into_owned());

        let result = run_diff_copy(&pool, items, &mounts, |_| {}, &CancellationToken::new()).await;

        // The bytes landed, but the index does not know about them — the next
        // diff would propose the same copy again and then call it a skip.
        assert!(dst_dir.join("a.cr3").exists());
        assert_eq!(result.copied, 0);
        assert_eq!(result.failed.len(), 1);
        assert!(
            result.failed[0].error.contains("could not be indexed"),
            "got: {}",
            result.failed[0].error
        );
    }

    #[tokio::test]
    async fn run_diff_copy_refuses_a_traversing_relative_path() {
        let pool = test_diff_pool().await;
        let tmp = tempfile::tempdir().unwrap();
        let src_dir = tmp.path().join("sot");
        let dst_dir = tmp.path().join("backup");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::create_dir_all(&dst_dir).unwrap();
        std::fs::write(src_dir.join("a.cr3"), b"secret").unwrap();

        let items = vec![DiffCopyItem {
            blake3_hash: "h1".to_string(),
            file_size: 6,
            file_name: "a.cr3".to_string(),
            source_path: src_dir.join("a.cr3").to_string_lossy().into_owned(),
            target_device_id: "bk1".to_string(),
            relative_path: "../../../etc/a.cr3".to_string(),
            modified_at: None,
        }];
        let mut mounts = HashMap::new();
        mounts.insert("bk1".to_string(), dst_dir.to_string_lossy().into_owned());
        let cancel = CancellationToken::new();

        let result = run_diff_copy(&pool, items, &mounts, |_| {}, &cancel).await;

        assert_eq!(result.copied, 0);
        assert_eq!(result.failed.len(), 1);
        assert!(!tmp.path().join("etc/a.cr3").exists());
        assert!(db::get_files_on_device(&pool, "bk1")
            .await
            .unwrap()
            .is_empty());
    }

    #[test]
    fn vanished_on_sot_deletes_from_every_backup_that_still_has_it() {
        let probed_locs = vec![
            probed(1, "sot", "2026-08-14/a.cr3", "h1", 100, Presence::Gone),
            probed(2, "bk1", "2026-08-14/a.cr3", "h1", 100, Presence::Present),
            probed(3, "bk2", "2026-08-14/a.cr3", "h1", 100, Presence::Present),
        ];
        let out = classify(
            &probed_locs,
            "sot",
            "/Volumes/SoT",
            &["bk1".to_string(), "bk2".to_string()],
        );

        assert_eq!(out.delete.len(), 2);
        assert_eq!(out.copy.len(), 0);
        assert_eq!(out.purge_location_ids, vec![1]);
        let ids: Vec<i64> = out.delete.iter().map(|(_, e)| e.location_id.unwrap()).collect();
        assert!(ids.contains(&2) && ids.contains(&3));
    }

    #[test]
    fn a_file_never_on_the_sot_is_never_a_delete_candidate() {
        let probed_locs = vec![
            probed(2, "bk1", "2026-08-14/orphan.cr3", "h9", 100, Presence::Present),
        ];
        let out = classify(&probed_locs, "sot", "/Volumes/SoT", &["bk1".to_string()]);

        assert!(out.delete.is_empty());
        assert!(out.copy.is_empty());
        assert!(out.purge_location_ids.is_empty());
        assert!(out.unreadable.is_empty());
    }

    #[test]
    fn same_hash_same_size_two_paths_on_the_sot() {
        // The same content sits at two paths on the source of truth. The user
        // rejected one in Lightroom and kept the other. Deleting the backup's
        // copy of the kept path would leave the content on the working disk
        // only — the exact inversion of what this app is for.
        let probed_locs = vec![
            probed(1, "sot", "2026-08-14/A/x.cr3", "h1", 100, Presence::Gone),
            probed(2, "sot", "2026-08-14/B/x.cr3", "h1", 100, Presence::Present),
            probed(3, "bk1", "2026-08-14/A/x.cr3", "h1", 100, Presence::Present),
            probed(4, "bk1", "2026-08-14/B/x.cr3", "h1", 100, Presence::Present),
        ];
        let out = classify(&probed_locs, "sot", "/Volumes/SoT", &["bk1".to_string()]);

        assert!(
            out.delete.is_empty(),
            "content still on the source of truth must not be deleted anywhere"
        );
        assert!(out.copy.is_empty(), "the backup already holds this content");
        // The row for the path that really did vanish is still stale.
        assert_eq!(out.purge_location_ids, vec![1]);
    }

    #[test]
    fn the_choice_of_sot_row_does_not_depend_on_row_order() {
        // Same group as above with the present row first. Row order out of
        // SQLite is rowid order, and it must not change the outcome.
        let reversed = vec![
            probed(2, "sot", "2026-08-14/B/x.cr3", "h1", 100, Presence::Present),
            probed(1, "sot", "2026-08-14/A/x.cr3", "h1", 100, Presence::Gone),
            probed(4, "bk1", "2026-08-14/B/x.cr3", "h1", 100, Presence::Present),
            probed(3, "bk1", "2026-08-14/A/x.cr3", "h1", 100, Presence::Present),
        ];
        let out = classify(&reversed, "sot", "/Volumes/SoT", &["bk1".to_string()]);

        assert!(out.delete.is_empty());
        assert_eq!(out.purge_location_ids, vec![1]);
    }

    #[test]
    fn every_sot_copy_gone_deletes_the_backup_copies_at_those_paths() {
        // Both paths rejected: the content itself is gone from the source of
        // truth, so both backup copies go.
        let probed_locs = vec![
            probed(1, "sot", "2026-08-14/A/x.cr3", "h1", 100, Presence::Gone),
            probed(2, "sot", "2026-08-14/B/x.cr3", "h1", 100, Presence::Gone),
            probed(3, "bk1", "2026-08-14/A/x.cr3", "h1", 100, Presence::Present),
            probed(4, "bk1", "2026-08-14/B/x.cr3", "h1", 100, Presence::Present),
        ];
        let out = classify(&probed_locs, "sot", "/Volumes/SoT", &["bk1".to_string()]);

        let mut ids: Vec<i64> = out.delete.iter().map(|(_, e)| e.location_id.unwrap()).collect();
        ids.sort();
        assert_eq!(ids, vec![3, 4]);
        assert_eq!(out.purge_location_ids, vec![1, 2]);
    }

    #[test]
    fn a_backup_copy_at_a_path_the_sot_never_indexed_is_left_alone() {
        // The source of truth lost A/x.cr3, but the backup keeps this content
        // at C/x.cr3, a path the index never saw on the source of truth.
        // There is no evidence that path was rejected.
        let probed_locs = vec![
            probed(1, "sot", "2026-08-14/A/x.cr3", "h1", 100, Presence::Gone),
            probed(2, "bk1", "2026-08-14/C/x.cr3", "h1", 100, Presence::Present),
        ];
        let out = classify(&probed_locs, "sot", "/Volumes/SoT", &["bk1".to_string()]);

        assert!(out.delete.is_empty());
        assert_eq!(out.purge_location_ids, vec![1]);
    }

    #[test]
    fn an_unreadable_sot_copy_blocks_deletion_for_the_whole_group() {
        let probed_locs = vec![
            probed(1, "sot", "2026-08-14/A/x.cr3", "h1", 100, Presence::Gone),
            probed(2, "sot", "2026-08-14/B/x.cr3", "h1", 100, Presence::Unknown),
            probed(3, "bk1", "2026-08-14/A/x.cr3", "h1", 100, Presence::Present),
        ];
        let out = classify(&probed_locs, "sot", "/Volumes/SoT", &["bk1".to_string()]);

        assert!(out.delete.is_empty());
        assert_eq!(out.unreadable.len(), 1);
        // The confirmed-gone row is still stale and still purged.
        assert_eq!(out.purge_location_ids, vec![1]);
    }

    #[test]
    fn same_hash_different_size_is_a_different_file() {
        // Two video files sharing a 4 MB header. The big one is gone from the
        // SoT; the small one is not. Only the big one may be deleted.
        let probed_locs = vec![
            probed(1, "sot", "2026-08-14/big.mov", "hv", 900, Presence::Gone),
            probed(2, "sot", "2026-08-14/small.mov", "hv", 100, Presence::Present),
            probed(3, "bk1", "2026-08-14/big.mov", "hv", 900, Presence::Present),
            probed(4, "bk1", "2026-08-14/small.mov", "hv", 100, Presence::Present),
        ];
        let out = classify(&probed_locs, "sot", "/Volumes/SoT", &["bk1".to_string()]);

        assert_eq!(out.delete.len(), 1);
        assert_eq!(out.delete[0].1.location_id, Some(3));
        assert_eq!(out.purge_location_ids, vec![1]);
    }

    #[test]
    fn unreadable_sot_file_is_excluded_from_both_lists() {
        let probed_locs = vec![
            probed(1, "sot", "2026-08-14/a.cr3", "h1", 100, Presence::Unknown),
            probed(2, "bk1", "2026-08-14/a.cr3", "h1", 100, Presence::Present),
        ];
        let out = classify(&probed_locs, "sot", "/Volumes/SoT", &["bk1".to_string()]);

        assert!(out.delete.is_empty());
        assert!(out.copy.is_empty());
        assert!(out.purge_location_ids.is_empty());
        assert_eq!(out.unreadable.len(), 1);
        assert_eq!(out.unreadable[0].device_id, "sot");
    }

    #[test]
    fn unreadable_backup_file_is_excluded_from_both_lists() {
        let probed_locs = vec![
            probed(1, "sot", "2026-08-14/a.cr3", "h1", 100, Presence::Present),
            probed(2, "bk1", "2026-08-14/a.cr3", "h1", 100, Presence::Unknown),
        ];
        let out = classify(&probed_locs, "sot", "/Volumes/SoT", &["bk1".to_string()]);

        assert!(out.delete.is_empty());
        assert!(out.copy.is_empty());
        assert_eq!(out.unreadable.len(), 1);
        assert_eq!(out.unreadable[0].device_id, "bk1");
    }

    #[test]
    fn missing_on_backup_mirrors_the_sot_relative_path() {
        let probed_locs = vec![
            probed(1, "sot", "2026-08-14/RAW/a.cr3", "h1", 100, Presence::Present),
        ];
        let out = classify(&probed_locs, "sot", "/Volumes/SoT", &["bk1".to_string()]);

        assert_eq!(out.copy.len(), 1);
        let (device, entry) = &out.copy[0];
        assert_eq!(device, "bk1");
        assert_eq!(entry.relative_path, "2026-08-14/RAW/a.cr3");
        assert_eq!(
            entry.source_path.as_deref(),
            Some("/Volumes/SoT/2026-08-14/RAW/a.cr3")
        );
        assert_eq!(entry.location_id, None);
    }

    #[test]
    fn a_backup_row_whose_file_vanished_is_a_copy_candidate() {
        let probed_locs = vec![
            probed(1, "sot", "2026-08-14/a.cr3", "h1", 100, Presence::Present),
            probed(2, "bk1", "2026-08-14/a.cr3", "h1", 100, Presence::Gone),
        ];
        let out = classify(&probed_locs, "sot", "/Volumes/SoT", &["bk1".to_string()]);

        assert_eq!(out.copy.len(), 1);
        assert!(out.delete.is_empty());
    }

    #[test]
    fn a_file_present_everywhere_produces_nothing() {
        let probed_locs = vec![
            probed(1, "sot", "2026-08-14/a.cr3", "h1", 100, Presence::Present),
            probed(2, "bk1", "2026-08-14/a.cr3", "h1", 100, Presence::Present),
        ];
        let out = classify(&probed_locs, "sot", "/Volumes/SoT", &["bk1".to_string()]);

        assert!(out.delete.is_empty());
        assert!(out.copy.is_empty());
        assert!(out.unreadable.is_empty());
    }

    #[tokio::test]
    async fn probe_reports_an_existing_file_as_present() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("a.cr3");
        std::fs::write(&file, b"hello").unwrap();

        assert_eq!(probe_path(file, 5).await, Presence::Present);
    }

    #[tokio::test]
    async fn probe_reports_a_missing_file_as_gone() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("nope.cr3");

        assert_eq!(probe_path(file, 5).await, Presence::Gone);
    }

    #[tokio::test]
    async fn probe_reports_a_path_under_a_missing_parent_as_gone() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("no-such-dir/a.cr3");

        assert_eq!(probe_path(file, 5).await, Presence::Gone);
    }
    fn location(id: i64, device: &str, path: &str) -> FileLocation {
        FileLocation {
            id,
            blake3_hash: format!("h{id}"),
            device_id: device.to_string(),
            file_path: path.to_string(),
            file_name: path.rsplit('/').next().unwrap().to_string(),
            file_size: 100,
            modified_at: Some("2026-08-14 10:00:00".to_string()),
            last_verified: "2026-08-14 10:00:00".to_string(),
            scan_mode: "full".to_string(),
        }
    }

    fn null_channel() -> Channel<DiffEvent> {
        Channel::new(|_| Ok(()))
    }

    #[tokio::test]
    async fn probe_locations_ignores_devices_with_no_mount() {
        // A backup left out of `mounts` is out of the diff. Probing it
        // through a mount point we do not have would report every one of its
        // files as missing.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.cr3"), b"x").unwrap();

        let locations = vec![
            location(1, "dev-1", "a.cr3"),
            location(2, "dev-2", "a.cr3"),
            location(3, "dev-2", "b.cr3"),
        ];
        let mut mounts = HashMap::new();
        mounts.insert("dev-1".to_string(), tmp.path().to_string_lossy().into_owned());

        let pass = probe_locations(
            locations,
            &mounts,
            &CancellationToken::new(),
            &null_channel(),
            None,
        )
        .await;

        assert!(!pass.sot_lost);
        assert_eq!(pass.probed.len(), 1);
        assert_eq!(pass.probed[0].device_id, "dev-1");
        assert!(pass.probed.iter().all(|p| p.device_id != "dev-2"));
    }

    #[tokio::test]
    async fn probe_locations_carries_the_indexed_mtime_through() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.cr3"), b"x").unwrap();
        let mut mounts = HashMap::new();
        mounts.insert("dev-1".to_string(), tmp.path().to_string_lossy().into_owned());

        let pass = probe_locations(
            vec![location(1, "dev-1", "a.cr3")],
            &mounts,
            &CancellationToken::new(),
            &null_channel(),
            None,
        )
        .await;

        assert_eq!(pass.probed[0].presence, Presence::Present);
        assert_eq!(
            pass.probed[0].modified_at.as_deref(),
            Some("2026-08-14 10:00:00")
        );
    }

    struct AlwaysLost;
    impl SotRecheck for AlwaysLost {
        fn still_mounted(&self) -> Pin<Box<dyn Future<Output = bool> + Send + '_>> {
            Box::pin(async { false })
        }
    }

    #[tokio::test]
    async fn probe_locations_stops_when_the_mid_pass_recheck_fails() {
        // A volume that unmounts and comes back inside one pass satisfies a
        // before-and-after check while everything probed in the gap reads as
        // gone.
        let tmp = tempfile::tempdir().unwrap();
        let mut mounts = HashMap::new();
        mounts.insert("dev-1".to_string(), tmp.path().to_string_lossy().into_owned());
        let locations: Vec<FileLocation> = (1..=(SOT_RECHECK_EVERY as i64 + 50))
            .map(|i| location(i, "dev-1", &format!("f{i}.cr3")))
            .collect();

        let pass = probe_locations(
            locations,
            &mounts,
            &CancellationToken::new(),
            &null_channel(),
            Some(&AlwaysLost),
        )
        .await;

        assert!(pass.sot_lost);
        assert_eq!(pass.probed.len() as u64, SOT_RECHECK_EVERY);
    }

    #[tokio::test]
    async fn probe_locations_completes_when_the_recheck_holds() {
        struct StillThere;
        impl SotRecheck for StillThere {
            fn still_mounted(&self) -> Pin<Box<dyn Future<Output = bool> + Send + '_>> {
                Box::pin(async { true })
            }
        }

        let tmp = tempfile::tempdir().unwrap();
        let mut mounts = HashMap::new();
        mounts.insert("dev-1".to_string(), tmp.path().to_string_lossy().into_owned());
        let total = SOT_RECHECK_EVERY as i64 + 5;
        let locations: Vec<FileLocation> = (1..=total)
            .map(|i| location(i, "dev-1", &format!("f{i}.cr3")))
            .collect();

        let pass = probe_locations(
            locations,
            &mounts,
            &CancellationToken::new(),
            &null_channel(),
            Some(&StillThere),
        )
        .await;

        assert!(!pass.sot_lost);
        assert_eq!(pass.probed.len() as i64, total);
    }
}
