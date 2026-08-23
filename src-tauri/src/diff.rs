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
