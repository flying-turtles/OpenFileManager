use std::collections::HashMap;
use std::path::{Path, PathBuf};
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
        let group = &groups[key];
        let sot = match group.iter().find(|l| l.device_id == sot_device_id) {
            Some(l) => *l,
            // Never on the source of truth: not our business.
            None => continue,
        };

        match sot.presence {
            Presence::Unknown => {
                out.unreadable.push(DiffUnreadable {
                    device_id: sot.device_id.clone(),
                    relative_path: sot.file_path.clone(),
                    error: "Could not read on the source of truth".to_string(),
                });
            }
            Presence::Gone => {
                out.purge_location_ids.push(sot.location_id);
                for loc in group.iter().filter(|l| l.device_id != sot_device_id) {
                    match loc.presence {
                        Presence::Present => out.delete.push((
                            loc.device_id.clone(),
                            DiffFileEntry {
                                blake3_hash: loc.blake3_hash.clone(),
                                file_size: loc.file_size,
                                file_name: loc.file_name.clone(),
                                relative_path: loc.file_path.clone(),
                                location_id: Some(loc.location_id),
                                source_path: None,
                            },
                        )),
                        Presence::Gone => {}
                        Presence::Unknown => out.unreadable.push(DiffUnreadable {
                            device_id: loc.device_id.clone(),
                            relative_path: loc.file_path.clone(),
                            error: "Could not read on this device".to_string(),
                        }),
                    }
                }
            }
            Presence::Present => {
                for device_id in backup_device_ids {
                    let existing = group.iter().find(|l| l.device_id == *device_id);
                    match existing.map(|l| l.presence) {
                        Some(Presence::Present) => {}
                        Some(Presence::Unknown) => out.unreadable.push(DiffUnreadable {
                            device_id: device_id.clone(),
                            relative_path: existing.unwrap().file_path.clone(),
                            error: "Could not read on this device".to_string(),
                        }),
                        // No row at all, or a row whose file is gone.
                        None | Some(Presence::Gone) => out.copy.push((
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
                            },
                        )),
                    }
                }
            }
        }
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
) -> Vec<ProbedLocation> {
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
                presence,
            })
        }));
    }

    let mut probed = Vec::with_capacity(handles.len());
    let mut checked: u64 = 0;
    for handle in handles {
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
    }

    probed
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

/// Copies each item to its target device, indexing what lands.
///
/// An occupied target is skipped, never overwritten — this feature only ever
/// adds files a backup is missing.
pub async fn run_diff_copy(
    pool: &DbPool,
    items: Vec<DiffCopyItem>,
    mounts: &HashMap<String, String>,
    channel: &Channel<DiffCopyEvent>,
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
            let _ = channel.send(DiffCopyEvent::Cancelled);
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

        let dest = plan_copy_target(Path::new(&mount), &item.relative_path);

        let _ = channel.send(DiffCopyEvent::Progress(DeviceCopyProgress {
            device_id: item.target_device_id.clone(),
            device_label: item.target_device_id.clone(),
            bytes_copied: result.bytes_copied,
            total_bytes,
            files_copied: result.copied as u64,
            total_files,
            current_file: item.file_name.clone(),
        }));

        if !may_write_target(probe_path(dest.clone(), PROBE_TIMEOUT_SECS).await) {
            result.skipped.push(DiffCopyError {
                file_name: item.file_name.clone(),
                target_device_id: item.target_device_id.clone(),
                error: "A file already exists at the target path".to_string(),
            });
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
                result.copied += 1;
                result.bytes_copied += item.file_size;

                let extension = dest
                    .extension()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();
                let _ = db::upsert_file(
                    pool,
                    &item.blake3_hash,
                    item.file_size,
                    &item.file_name,
                    &extension,
                )
                .await;
                let _ = db::upsert_location(
                    pool,
                    &item.blake3_hash,
                    &item.target_device_id,
                    &item.relative_path,
                    &item.file_name,
                    item.file_size,
                    None,
                    "diff-sync",
                )
                .await;
            }
            Ok(false) => {
                let _ = channel.send(DiffCopyEvent::Cancelled);
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

    let _ = channel.send(DiffCopyEvent::Complete(result.clone()));
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
}
