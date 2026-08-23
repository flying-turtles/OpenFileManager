# Project Diff & Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** From the Projects screen, nominate one device as a project's source of truth, see how every other device differs from it, then delete the files it no longer has from the backups and copy across the ones they are missing.

**Architecture:** The index supplies the candidate file set and the "before" state; a live `stat` of each indexed path supplies the "after" state. A pure classification function turns those two inputs into delete candidates, copy candidates, stale-row purges, and an unreadable bucket. Deletion reuses the existing `bulk_delete_file_copies` command; copying reuses `importer::copy_file_cancellable`.

**Tech Stack:** Rust / Tauri 2 / SQLx / SQLite on the backend; React 19 + TypeScript + Vite on the frontend. No frontend test runner exists — Rust `#[cfg(test)]` modules are the whole test suite.

**Deviation from the spec:** the spec called for virtualizing rows with `@tanstack/react-virtual`. A recursive folder tree does not virtualize cleanly, so the plan bounds the cost two other ways instead: previews mount only when scrolled into view (`LazyThumb`), and any folder holding more than 500 files starts collapsed. The expensive part of a large diff is thumbnail generation, and that is what the lazy mount removes; plain rows are cheap. Revisit if a real shoot makes the page feel heavy.

**Spec:** `docs/superpowers/specs/2026-08-23-project-diff-sync-design.md`

## Global Constraints

- File identity is always `(blake3_hash, file_size)`, never `blake3_hash` alone. `blake3_hash` covers only the first 4 MB, so video containers sharing a header collide.
- Only `std::io::ErrorKind::NotFound` may be read as "the user deleted this". Every other probe outcome — timeout, permission, IO error — is `Presence::Unknown` and removes the file from both the delete and copy lists.
- Every filesystem probe goes through `tokio::task::spawn_blocking` behind a `tokio::time::timeout`. A stalled network mount must never occupy a runtime worker.
- `compute_project_diff` is read-only against the database. Index mutation happens only at resolve time.
- Rust structs crossing IPC carry `#[derive(Debug, Clone, Serialize, Deserialize)]` and `#[serde(rename_all = "camelCase")]`, matching `src-tauri/src/models.rs`.
- Never overwrite an existing file at a copy target. Skip and report.
- Deletion defaults to the Trash. Permanent deletion is opt-in via the shared `PermanentToggle`.
- Commit after every task. Run `cd src-tauri && cargo test` before every Rust commit and `npm run build` before every frontend commit.
- Per `CLAUDE.md`: the dmg is rebuilt after the application changes (final task).

## File Structure

| File | Responsibility |
|---|---|
| `src-tauri/src/diff.rs` (create) | Engine: `Presence`, `probe_path`, `probe_locations`, pure `classify`, `run_diff_copy` |
| `src-tauri/src/commands/diff.rs` (create) | Six Tauri commands, device/mount resolution, reachability gating |
| `src-tauri/src/models.rs` (modify) | `DiffDeviceOption`, `DiffFileEntry`, `DiffDeviceResult`, `ProjectDiff`, `DiffUnreadable`, `DiffCopyItem`, `DiffCopyError`, `DiffCopyResult`, `DiffEvent`, `DiffCopyEvent` |
| `src-tauri/src/db.rs` (modify) | `get_project_diff_devices` |
| `src-tauri/src/lib.rs` (modify) | `mod diff;`, AppState init, `invoke_handler` registrations |
| `src-tauri/src/commands/mod.rs` (modify) | `mod diff;`, `pub use diff::*;`, two AppState cancel-token fields |
| `src/types/index.ts` (modify) | TypeScript mirrors of the new Rust types |
| `src/api/commands.ts` (modify) | Six `invoke` bindings |
| `src/hooks/useProjectDiff.ts` (create) | Diff lifecycle state, plus pure exported tree/selection helpers |
| `src/components/LazyThumb.tsx` (create) | Mounts `<FilePreview>` only once scrolled into view |
| `src/components/DiffResolveModal.tsx` (create) | Confirmation, thumbnail grid, progress, result |
| `src/pages/ProjectDiff.tsx` (create) | The diff view |
| `src/pages/ProjectDiff.css` (create) | Its styles |
| `src/pages/Projects.tsx` (modify) | Source-of-truth dropdown and Show diff button |
| `src/App.tsx` (modify) | `projectDiff` page state and handoff |

---

### Task 1: Diff types and the pure classifier

The classifier is the heart of the feature and has no IO, so it gets written and tested first.

**Files:**
- Modify: `src-tauri/src/models.rs` (append at end)
- Create: `src-tauri/src/diff.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod diff;` next to the other module declarations)

**Interfaces:**
- Consumes: `crate::models::FileLocation` (existing).
- Produces: `diff::Presence`, `diff::ProbedLocation`, `diff::ClassifyOutput`, `diff::classify(probed, sot_device_id, sot_mount, backup_device_ids) -> ClassifyOutput`, and the model structs `DiffFileEntry`, `DiffUnreadable`, `DiffDeviceOption`, `DiffDeviceResult`, `ProjectDiff`, `DiffCopyItem`, `DiffCopyError`, `DiffCopyResult`, `DiffEvent`, `DiffCopyEvent`.

- [ ] **Step 1: Add the model types**

Append to `src-tauri/src/models.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffDeviceOption {
    pub device_id: String,
    pub label: String,
    pub file_count: i64,
    pub is_connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffFileEntry {
    pub blake3_hash: String,
    pub file_size: i64,
    pub file_name: String,
    /// Path relative to the mount of the device this entry belongs to. For a
    /// copy entry that is the target device, and the value is the mirrored
    /// source-of-truth relative path.
    pub relative_path: String,
    /// The backup `file_locations` row to delete. `None` for copy entries.
    pub location_id: Option<i64>,
    /// Absolute path on the source of truth. Set for copy entries only.
    pub source_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffUnreadable {
    pub device_id: String,
    pub relative_path: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffDeviceResult {
    pub device_id: String,
    pub device_label: String,
    /// Set when the device was not diffed at all, e.g. it is offline.
    pub skip_reason: Option<String>,
    pub to_delete: Vec<DiffFileEntry>,
    pub to_copy: Vec<DiffFileEntry>,
    pub delete_bytes: i64,
    pub copy_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDiff {
    pub project_id: i64,
    pub sot_device_id: String,
    pub sot_label: String,
    pub devices: Vec<DiffDeviceResult>,
    /// Stale source-of-truth rows, purged on resolve.
    pub purge_location_ids: Vec<i64>,
    pub unreadable: Vec<DiffUnreadable>,
    pub total_delete_files: i64,
    pub total_delete_bytes: i64,
    pub total_copy_files: i64,
    pub total_copy_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffCopyItem {
    pub blake3_hash: String,
    pub file_size: i64,
    pub file_name: String,
    pub source_path: String,
    pub target_device_id: String,
    /// Relative to the target device's mount point.
    pub relative_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffCopyError {
    pub file_name: String,
    pub target_device_id: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffCopyResult {
    pub copied: i64,
    pub bytes_copied: i64,
    /// Targets that already held a file at the mirrored path.
    pub skipped: Vec<DiffCopyError>,
    pub failed: Vec<DiffCopyError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiffEvent {
    Started { total: u64 },
    #[serde(rename_all = "camelCase")]
    Progress {
        checked: u64,
        total: u64,
        current_device: String,
    },
    Finished,
    Error { message: String },
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiffCopyEvent {
    Progress(DeviceCopyProgress),
    Complete(DiffCopyResult),
    Error { message: String },
    Cancelled,
}
```

- [ ] **Step 2: Write the failing tests**

Create `src-tauri/src/diff.rs` containing only the test module for now:

```rust
use crate::models::*;

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
}
```

Add `mod diff;` to `src-tauri/src/lib.rs` alongside the existing module declarations (`mod backup;`, `mod db;`, and so on).

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test diff::`
Expected: compile errors — `cannot find type Presence`, `cannot find function classify`.

- [ ] **Step 4: Write the implementation**

Insert above the `#[cfg(test)]` block in `src-tauri/src/diff.rs`:

```rust
use std::collections::HashMap;
use std::path::Path;

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
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test diff::`
Expected: 8 passed.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/models.rs src-tauri/src/diff.rs src-tauri/src/lib.rs
git commit -m "diff: types and the pure SoT/backup classifier"
```

---

### Task 2: Filesystem probing

**Files:**
- Modify: `src-tauri/src/diff.rs`

**Interfaces:**
- Consumes: `diff::Presence`, `diff::ProbedLocation` from Task 1; `crate::models::FileLocation`, `crate::models::DiffEvent`.
- Produces: `diff::probe_path(path: PathBuf, secs: u64) -> Presence` and `diff::probe_locations(locations: Vec<FileLocation>, mounts: &HashMap<String, String>, cancel: &CancellationToken, channel: &Channel<DiffEvent>) -> Vec<ProbedLocation>`.

- [ ] **Step 1: Write the failing tests**

Add to the `mod tests` block in `src-tauri/src/diff.rs`:

```rust
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test diff::tests::probe`
Expected: compile error — `cannot find function probe_path`.

- [ ] **Step 3: Write the implementation**

Add to the top of `src-tauri/src/diff.rs`:

```rust
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tauri::ipc::Channel;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::models::FileLocation;

/// How many probes may be in flight at once. Enough to hide per-call latency
/// on a spinning disk, small enough that a stalled mount cannot swallow the
/// blocking pool.
const PROBE_CONCURRENCY: usize = 8;

/// Per-probe deadline. A `stat` that has not answered in this long is treated
/// as unreadable, never as a deletion.
const PROBE_TIMEOUT_SECS: u64 = 5;
```

And, above the `#[cfg(test)]` block:

```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test diff::`
Expected: 11 passed.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/diff.rs
git commit -m "diff: bounded, deadline-guarded filesystem probing"
```

---

### Task 3: Device list query and command

**Files:**
- Modify: `src-tauri/src/db.rs` (add after `get_project_stats`)
- Create: `src-tauri/src/commands/diff.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `crate::models::DiffDeviceOption` from Task 1; existing `db::get_project`, `db::get_all_devices`, `commands::path_online`.
- Produces: `db::get_project_diff_devices(pool, start_date, end_date) -> Vec<DiffDeviceOption>` (with `is_connected` left `false`), and the Tauri command `get_project_diff_devices(project_id) -> Vec<DiffDeviceOption>` which fills `is_connected` in.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `src-tauri/src/db.rs`:

```rust
    #[tokio::test]
    async fn project_diff_devices_counts_files_per_device() {
        let pool = test_pool().await;
        for (id, mount) in [("dev-1", "/Volumes/One"), ("dev-2", "/Volumes/Two")] {
            upsert_device(
                &pool,
                &DetectedDisk {
                    id: id.into(),
                    label: format!("{id} label"),
                    mount_point: mount.into(),
                    total_bytes: 0,
                    available_bytes: 0,
                    is_removable: false,
                },
            )
            .await
            .unwrap();
        }

        // Two files inside the project window, one outside it.
        for (hash, modified) in [
            ("h1", "2026-08-14 10:00:00"),
            ("h2", "2026-08-14 11:00:00"),
            ("h3", "2026-09-01 10:00:00"),
        ] {
            upsert_file(&pool, hash, 10, "f.cr3", "cr3").await.unwrap();
            upsert_location(&pool, hash, "dev-1", &format!("2026/{hash}.cr3"), "f.cr3", 10, Some(modified), "full")
                .await
                .unwrap();
        }
        // dev-2 holds only one of the in-window files.
        upsert_location(&pool, "h1", "dev-2", "2026/h1.cr3", "f.cr3", 10, Some("2026-08-14 10:00:00"), "full")
            .await
            .unwrap();

        let devices = get_project_diff_devices(&pool, "2026-08-14", "2026-08-14")
            .await
            .unwrap();

        assert_eq!(devices.len(), 2);
        let one = devices.iter().find(|d| d.device_id == "dev-1").unwrap();
        let two = devices.iter().find(|d| d.device_id == "dev-2").unwrap();
        assert_eq!(one.file_count, 2, "the September file is out of the window");
        assert_eq!(two.file_count, 1);
        assert_eq!(one.label, "dev-1 label");
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd src-tauri && cargo test db::tests::project_diff_devices`
Expected: compile error — `cannot find function get_project_diff_devices`.

- [ ] **Step 3: Write the query**

Add to `src-tauri/src/db.rs`, directly after `get_project_stats`:

```rust
/// Devices holding at least one file of the project's date window.
///
/// `is_connected` is left `false`; only the command layer can probe mounts.
pub async fn get_project_diff_devices(
    pool: &DbPool,
    start_date: &str,
    end_date: &str,
) -> Result<Vec<DiffDeviceOption>, AppError> {
    let rows = sqlx::query_as::<_, (String, String, i64)>(
        "SELECT fl.device_id, d.label, COUNT(DISTINCT fl.blake3_hash)
         FROM file_locations fl
         JOIN storage_devices d ON d.id = fl.device_id
         WHERE fl.blake3_hash IN (
             SELECT fl2.blake3_hash
             FROM file_locations fl2
             GROUP BY fl2.blake3_hash
             HAVING MIN(fl2.modified_at) >= ? AND MIN(fl2.modified_at) < date(?, '+1 day')
         )
         GROUP BY fl.device_id, d.label
         ORDER BY d.label",
    )
    .bind(start_date)
    .bind(end_date)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(device_id, label, file_count)| DiffDeviceOption {
            device_id,
            label,
            file_count,
            is_connected: false,
        })
        .collect())
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd src-tauri && cargo test db::tests::project_diff_devices`
Expected: PASS.

- [ ] **Step 5: Add the command file**

Create `src-tauri/src/commands/diff.rs`:

```rust
use std::collections::HashMap;

use tauri::State;

use super::{path_online, path_online_within, AppState};
use crate::db;
use crate::error::AppError;
use crate::models::*;

#[tauri::command]
pub async fn get_project_diff_devices(
    state: State<'_, AppState>,
    project_id: i64,
) -> Result<Vec<DiffDeviceOption>, AppError> {
    let project = db::get_project(&state.pool, project_id).await?;
    let mut options =
        db::get_project_diff_devices(&state.pool, &project.start_date, &project.end_date).await?;

    let devices = db::get_all_devices(&state.pool).await?;
    let mounts: HashMap<String, String> = devices
        .into_iter()
        .map(|d| (d.id, d.mount_point))
        .collect();

    for opt in &mut options {
        opt.is_connected = match mounts.get(&opt.device_id) {
            Some(mount) => path_online(mount).await,
            None => false,
        };
    }

    Ok(options)
}
```

`path_online_within` is imported here for Task 4; if the compiler warns about it being unused at this point, leave the import out and add it in Task 4.

- [ ] **Step 6: Register the module and the command**

In `src-tauri/src/commands/mod.rs` add `mod diff;` to the module list (alphabetically, after `mod devices;`) and `pub use diff::*;` to the re-export block (after `pub use devices::*;`).

In `src-tauri/src/lib.rs`, inside `tauri::generate_handler![...]`, add after `commands::delete_redundant_scanned_files,`:

```rust
            commands::get_project_diff_devices,
```

- [ ] **Step 7: Verify the build and the whole suite**

Run: `cd src-tauri && cargo test`
Expected: all tests pass.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/db.rs src-tauri/src/commands/diff.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "diff: list the devices holding a project's files"
```

---

### Task 4: compute_project_diff

**Files:**
- Modify: `src-tauri/src/commands/diff.rs`
- Modify: `src-tauri/src/commands/mod.rs` (AppState fields)
- Modify: `src-tauri/src/lib.rs` (AppState init, command registration)

**Interfaces:**
- Consumes: `diff::probe_locations`, `diff::classify` from Tasks 1 and 2; `db::get_project`, `db::get_project_files`, `db::get_all_devices`, `commands::path_online_within`.
- Produces: Tauri commands `compute_project_diff(project_id, sot_device_id, on_event) -> ProjectDiff` and `cancel_project_diff()`.

- [ ] **Step 1: Add the cancel tokens to AppState**

In `src-tauri/src/commands/mod.rs`, add to `pub struct AppState`:

```rust
    pub diff_cancel_token: Arc<Mutex<Option<CancellationToken>>>,
    pub diff_copy_cancel_token: Arc<Mutex<Option<CancellationToken>>>,
```

In `src-tauri/src/lib.rs`, inside `app.manage(AppState { ... })`, add:

```rust
                diff_cancel_token: Arc::new(Mutex::new(None)),
                diff_copy_cancel_token: Arc::new(Mutex::new(None)),
```

- [ ] **Step 2: Write the command**

Append to `src-tauri/src/commands/diff.rs`:

```rust
use tauri::ipc::Channel;
use tokio_util::sync::CancellationToken;

use crate::diff;

/// Longer deadline than the per-file probes: a sleeping NAS gets a chance to
/// spin up before the whole diff is refused.
const MOUNT_DEADLINE_SECS: u64 = 5;

#[tauri::command]
pub async fn compute_project_diff(
    state: State<'_, AppState>,
    project_id: i64,
    sot_device_id: String,
    on_event: Channel<DiffEvent>,
) -> Result<ProjectDiff, AppError> {
    let cancel = CancellationToken::new();
    *state.diff_cancel_token.lock().await = Some(cancel.clone());

    let result = compute_inner(&state, project_id, &sot_device_id, &on_event, &cancel).await;

    *state.diff_cancel_token.lock().await = None;

    match result {
        Ok(diff) => {
            let _ = on_event.send(DiffEvent::Finished);
            Ok(diff)
        }
        Err(e) => {
            if cancel.is_cancelled() {
                let _ = on_event.send(DiffEvent::Cancelled);
            } else {
                let _ = on_event.send(DiffEvent::Error {
                    message: e.to_string(),
                });
            }
            Err(e)
        }
    }
}

async fn compute_inner(
    state: &State<'_, AppState>,
    project_id: i64,
    sot_device_id: &str,
    on_event: &Channel<DiffEvent>,
    cancel: &CancellationToken,
) -> Result<ProjectDiff, AppError> {
    let project = db::get_project(&state.pool, project_id).await?;
    let all_devices = db::get_all_devices(&state.pool).await?;

    let sot = all_devices
        .iter()
        .find(|d| d.id == sot_device_id)
        .ok_or_else(|| AppError::General("Source of truth device is not known".into()))?
        .clone();

    // Without this the probes would report every file as gone and the diff
    // would propose deleting the entire project from the backups.
    if !path_online_within(&sot.mount_point, MOUNT_DEADLINE_SECS).await {
        return Err(AppError::General(format!(
            "{} is not reachable at {}",
            sot.label, sot.mount_point
        )));
    }

    let files = db::get_project_files(&state.pool, &project.start_date, &project.end_date).await?;
    let locations: Vec<FileLocation> = files.into_iter().flat_map(|f| f.locations).collect();

    if !locations.iter().any(|l| l.device_id == sot_device_id) {
        return Err(AppError::General(format!(
            "{} holds none of this project's files",
            sot.label
        )));
    }

    // Devices that appear in the project, other than the source of truth.
    let mut backup_ids: Vec<String> = locations
        .iter()
        .map(|l| l.device_id.clone())
        .filter(|id| id != sot_device_id)
        .collect();
    backup_ids.sort();
    backup_ids.dedup();

    let mut mounts: HashMap<String, String> = HashMap::new();
    mounts.insert(sot.id.clone(), sot.mount_point.clone());

    let mut reachable_backups: Vec<String> = Vec::new();
    let mut skipped: Vec<DiffDeviceResult> = Vec::new();

    for id in &backup_ids {
        let device = all_devices.iter().find(|d| d.id == *id);
        let reachable = match device {
            Some(d) => path_online_within(&d.mount_point, MOUNT_DEADLINE_SECS).await,
            None => false,
        };
        match (device, reachable) {
            (Some(d), true) => {
                mounts.insert(id.clone(), d.mount_point.clone());
                reachable_backups.push(id.clone());
            }
            (device, _) => skipped.push(DiffDeviceResult {
                device_id: id.clone(),
                device_label: device.map(|d| d.label.clone()).unwrap_or_else(|| id.clone()),
                skip_reason: Some("Not reachable".to_string()),
                to_delete: Vec::new(),
                to_copy: Vec::new(),
                delete_bytes: 0,
                copy_bytes: 0,
            }),
        }
    }

    let probed = diff::probe_locations(locations, &mounts, cancel, on_event).await;

    if cancel.is_cancelled() {
        return Err(AppError::General("Cancelled".into()));
    }

    // A mount that dropped mid-pass would look like a mass deletion.
    if !path_online_within(&sot.mount_point, MOUNT_DEADLINE_SECS).await {
        return Err(AppError::General(format!(
            "{} went offline during the diff",
            sot.label
        )));
    }

    let out = diff::classify(&probed, &sot.id, &sot.mount_point, &reachable_backups);

    let mut by_device: HashMap<String, DiffDeviceResult> = HashMap::new();
    for id in &reachable_backups {
        let label = all_devices
            .iter()
            .find(|d| d.id == *id)
            .map(|d| d.label.clone())
            .unwrap_or_else(|| id.clone());
        by_device.insert(
            id.clone(),
            DiffDeviceResult {
                device_id: id.clone(),
                device_label: label,
                skip_reason: None,
                to_delete: Vec::new(),
                to_copy: Vec::new(),
                delete_bytes: 0,
                copy_bytes: 0,
            },
        );
    }

    for (device_id, entry) in out.delete {
        if let Some(d) = by_device.get_mut(&device_id) {
            d.delete_bytes += entry.file_size;
            d.to_delete.push(entry);
        }
    }
    for (device_id, entry) in out.copy {
        if let Some(d) = by_device.get_mut(&device_id) {
            d.copy_bytes += entry.file_size;
            d.to_copy.push(entry);
        }
    }

    let mut devices: Vec<DiffDeviceResult> = by_device.into_values().collect();
    for d in &mut devices {
        d.to_delete.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        d.to_copy.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    }
    devices.extend(skipped);
    devices.sort_by(|a, b| a.device_label.cmp(&b.device_label));

    let total_delete_files = devices.iter().map(|d| d.to_delete.len() as i64).sum();
    let total_delete_bytes = devices.iter().map(|d| d.delete_bytes).sum();
    let total_copy_files = devices.iter().map(|d| d.to_copy.len() as i64).sum();
    let total_copy_bytes = devices.iter().map(|d| d.copy_bytes).sum();

    Ok(ProjectDiff {
        project_id,
        sot_device_id: sot.id,
        sot_label: sot.label,
        devices,
        purge_location_ids: out.purge_location_ids,
        unreadable: out.unreadable,
        total_delete_files,
        total_delete_bytes,
        total_copy_files,
        total_copy_bytes,
    })
}

#[tauri::command]
pub async fn cancel_project_diff(state: State<'_, AppState>) -> Result<(), AppError> {
    if let Some(token) = state.diff_cancel_token.lock().await.as_ref() {
        token.cancel();
    }
    Ok(())
}
```

- [ ] **Step 3: Register the commands**

In `src-tauri/src/lib.rs`, after `commands::get_project_diff_devices,`:

```rust
            commands::compute_project_diff,
            commands::cancel_project_diff,
```

- [ ] **Step 4: Verify the build**

Run: `cd src-tauri && cargo test`
Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/diff.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "diff: compute a project diff against a source-of-truth device"
```

---

### Task 5: Copy and purge

**Files:**
- Modify: `src-tauri/src/diff.rs`
- Modify: `src-tauri/src/commands/diff.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `importer::copy_file_cancellable`, `db::upsert_file`, `db::upsert_location`, `db::delete_file_location_no_cleanup`, `db::cleanup_orphaned_files`; `DiffCopyItem`, `DiffCopyResult`, `DiffCopyError`, `DiffCopyEvent` from Task 1.
- Produces: `diff::plan_copy_target(target_mount, relative_path) -> PathBuf`, `diff::may_write_target(presence) -> bool`, `diff::run_diff_copy(pool, items, mounts, channel, cancel) -> DiffCopyResult`; Tauri commands `copy_diff_files(items, on_event) -> DiffCopyResult`, `cancel_diff_copy()`, `purge_diff_locations(location_ids) -> u64`.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `src-tauri/src/diff.rs`:

```rust
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test diff::`
Expected: compile error — `cannot find function plan_copy_target`.

- [ ] **Step 3: Write the copy engine**

Add to `src-tauri/src/diff.rs`, above the `#[cfg(test)]` block:

```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test diff::`
Expected: 14 passed.

- [ ] **Step 5: Add the commands**

Append to `src-tauri/src/commands/diff.rs`:

```rust
#[tauri::command]
pub async fn copy_diff_files(
    state: State<'_, AppState>,
    items: Vec<DiffCopyItem>,
    on_event: Channel<DiffCopyEvent>,
) -> Result<DiffCopyResult, AppError> {
    let cancel = CancellationToken::new();
    *state.diff_copy_cancel_token.lock().await = Some(cancel.clone());

    let devices = db::get_all_devices(&state.pool).await?;
    let mounts: HashMap<String, String> =
        devices.into_iter().map(|d| (d.id, d.mount_point)).collect();

    let result = diff::run_diff_copy(&state.pool, items, &mounts, &on_event, &cancel).await;

    *state.diff_copy_cancel_token.lock().await = None;
    Ok(result)
}

#[tauri::command]
pub async fn cancel_diff_copy(state: State<'_, AppState>) -> Result<(), AppError> {
    if let Some(token) = state.diff_copy_cancel_token.lock().await.as_ref() {
        token.cancel();
    }
    Ok(())
}

/// Drops index rows for source-of-truth files confirmed gone from disk.
///
/// Unconditional on the delete selection: a row pointing at a file that is no
/// longer there is wrong whether or not the user propagated the deletion.
#[tauri::command]
pub async fn purge_diff_locations(
    state: State<'_, AppState>,
    location_ids: Vec<i64>,
) -> Result<u64, AppError> {
    let mut purged: u64 = 0;
    for id in location_ids {
        if db::delete_file_location_no_cleanup(&state.pool, id)
            .await
            .is_ok()
        {
            purged += 1;
        }
    }
    if purged > 0 {
        let _ = db::cleanup_orphaned_files(&state.pool).await;
    }
    Ok(purged)
}
```

- [ ] **Step 6: Register the commands**

In `src-tauri/src/lib.rs`, after `commands::cancel_project_diff,`:

```rust
            commands::copy_diff_files,
            commands::cancel_diff_copy,
            commands::purge_diff_locations,
```

- [ ] **Step 7: Verify the build**

Run: `cd src-tauri && cargo test`
Expected: all tests pass.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/diff.rs src-tauri/src/commands/diff.rs src-tauri/src/lib.rs
git commit -m "diff: copy missing files to backups and purge stale index rows"
```

---

### Task 6: TypeScript types and API bindings

**Files:**
- Modify: `src/types/index.ts` (append)
- Modify: `src/api/commands.ts` (extend the type import list, append the bindings)

**Interfaces:**
- Consumes: the Rust commands from Tasks 3 to 5.
- Produces: `getProjectDiffDevices`, `computeProjectDiff`, `cancelProjectDiff`, `copyDiffFiles`, `cancelDiffCopy`, `purgeDiffLocations`, plus the mirrored types.

- [ ] **Step 1: Add the types**

Append to `src/types/index.ts`:

```ts
export interface DiffDeviceOption {
  deviceId: string;
  label: string;
  fileCount: number;
  isConnected: boolean;
}

export interface DiffFileEntry {
  blake3Hash: string;
  fileSize: number;
  fileName: string;
  /** Relative to the mount of the device this entry belongs to. */
  relativePath: string;
  /** Backup location row to delete. Null for copy entries. */
  locationId: number | null;
  /** Absolute path on the source of truth. Set for copy entries only. */
  sourcePath: string | null;
}

export interface DiffUnreadable {
  deviceId: string;
  relativePath: string;
  error: string;
}

export interface DiffDeviceResult {
  deviceId: string;
  deviceLabel: string;
  /** Set when the device was not diffed at all, e.g. it is offline. */
  skipReason: string | null;
  toDelete: DiffFileEntry[];
  toCopy: DiffFileEntry[];
  deleteBytes: number;
  copyBytes: number;
}

export interface ProjectDiff {
  projectId: number;
  sotDeviceId: string;
  sotLabel: string;
  devices: DiffDeviceResult[];
  purgeLocationIds: number[];
  unreadable: DiffUnreadable[];
  totalDeleteFiles: number;
  totalDeleteBytes: number;
  totalCopyFiles: number;
  totalCopyBytes: number;
}

export interface DiffCopyItem {
  blake3Hash: string;
  fileSize: number;
  fileName: string;
  sourcePath: string;
  targetDeviceId: string;
  relativePath: string;
}

export interface DiffCopyError {
  fileName: string;
  targetDeviceId: string;
  error: string;
}

export interface DiffCopyResult {
  copied: number;
  bytesCopied: number;
  skipped: DiffCopyError[];
  failed: DiffCopyError[];
}

export type DiffEvent =
  | { Started: { total: number } }
  | { Progress: { checked: number; total: number; currentDevice: string } }
  | "Finished"
  | { Error: { message: string } }
  | "Cancelled";

export type DiffCopyEvent =
  | { Progress: DeviceCopyProgress }
  | { Complete: DiffCopyResult }
  | { Error: { message: string } }
  | "Cancelled";
```

- [ ] **Step 2: Add the bindings**

Add to the type import list at the top of `src/api/commands.ts`:

```ts
  DiffDeviceOption,
  ProjectDiff,
  DiffEvent,
  DiffCopyItem,
  DiffCopyResult,
  DiffCopyEvent,
```

Append to `src/api/commands.ts`:

```ts
export async function getProjectDiffDevices(
  projectId: number
): Promise<DiffDeviceOption[]> {
  return invoke("get_project_diff_devices", { projectId });
}

export async function computeProjectDiff(
  projectId: number,
  sotDeviceId: string,
  onEvent: (event: DiffEvent) => void
): Promise<ProjectDiff> {
  const channel = new Channel<DiffEvent>();
  channel.onmessage = onEvent;
  return invoke("compute_project_diff", { projectId, sotDeviceId, onEvent: channel });
}

export async function cancelProjectDiff(): Promise<void> {
  return invoke("cancel_project_diff");
}

export async function copyDiffFiles(
  items: DiffCopyItem[],
  onEvent: (event: DiffCopyEvent) => void
): Promise<DiffCopyResult> {
  const channel = new Channel<DiffCopyEvent>();
  channel.onmessage = onEvent;
  return invoke("copy_diff_files", { items, onEvent: channel });
}

export async function cancelDiffCopy(): Promise<void> {
  return invoke("cancel_diff_copy");
}

export async function purgeDiffLocations(locationIds: number[]): Promise<number> {
  return invoke("purge_diff_locations", { locationIds });
}
```

- [ ] **Step 3: Verify the build**

Run: `npm run build`
Expected: `tsc` clean, Vite build succeeds.

- [ ] **Step 4: Commit**

```bash
git add src/types/index.ts src/api/commands.ts
git commit -m "diff: TypeScript types and command bindings"
```

---

### Task 7: useProjectDiff hook and tree helpers

The tree grouping and checkbox propagation are pure functions so they can be read and reasoned about without a test runner.

**Files:**
- Create: `src/hooks/useProjectDiff.ts`

**Interfaces:**
- Consumes: `computeProjectDiff`, `cancelProjectDiff`, `copyDiffFiles`, `purgeDiffLocations` from Task 6; the existing `bulkDeleteFileCopies`; `notifyDone` from `src/utils/notify`.
- Produces: `entryKey(deviceId, relativePath) -> string`, `buildTree(entries) -> TreeNode`, `collectKeys(deviceId, node) -> string[]`, `folderState(keys, selected) -> "all" | "none" | "some"`, the `TreeNode` and `ResolveResult` types, and the `useProjectDiff(projectId, sotDeviceId)` hook returning `{ phase, diff, progress, error, selectedDelete, selectedCopy, deleteLocationIds, copyItems, toggleKeys, setAll, resolve, resolveProgress, resolveResult, cancel, reload }`.

- [ ] **Step 1: Write the helpers and the hook**

Create `src/hooks/useProjectDiff.ts`:

```ts
import { useState, useEffect, useCallback, useMemo } from "react";
import {
  computeProjectDiff,
  cancelProjectDiff,
  copyDiffFiles,
  purgeDiffLocations,
  bulkDeleteFileCopies,
} from "../api/commands";
import { notifyDone } from "../utils/notify";
import type {
  ProjectDiff,
  DiffFileEntry,
  DiffEvent,
  DiffCopyItem,
  DiffCopyResult,
  BulkDeleteResult,
} from "../types";

/** Identifies one entry across the whole diff. */
export function entryKey(deviceId: string, relativePath: string): string {
  return `${deviceId}>${relativePath}`;
}

export interface TreeNode {
  /** Folder path relative to the device mount. Empty at the root. */
  path: string;
  name: string;
  children: TreeNode[];
  files: DiffFileEntry[];
}

/** Groups entries into a folder tree by their relative path. */
export function buildTree(entries: DiffFileEntry[]): TreeNode {
  const root: TreeNode = { path: "", name: "", children: [], files: [] };

  for (const entry of entries) {
    const parts = entry.relativePath.split("/");
    const dirs = parts.slice(0, -1);
    let node = root;
    let sofar = "";
    for (const dir of dirs) {
      sofar = sofar ? `${sofar}/${dir}` : dir;
      let child = node.children.find((c) => c.name === dir);
      if (!child) {
        child = { path: sofar, name: dir, children: [], files: [] };
        node.children.push(child);
      }
      node = child;
    }
    node.files.push(entry);
  }

  const sortNode = (node: TreeNode) => {
    node.children.sort((a, b) => a.name.localeCompare(b.name));
    node.files.sort((a, b) => a.fileName.localeCompare(b.fileName));
    node.children.forEach(sortNode);
  };
  sortNode(root);

  return root;
}

/** Every entry key at or below this node. */
export function collectKeys(deviceId: string, node: TreeNode): string[] {
  const keys = node.files.map((f) => entryKey(deviceId, f.relativePath));
  for (const child of node.children) {
    keys.push(...collectKeys(deviceId, child));
  }
  return keys;
}

/** Checkbox state for a folder, given the keys underneath it. */
export function folderState(
  keys: string[],
  selected: Set<string>
): "all" | "none" | "some" {
  if (keys.length === 0) return "none";
  let hits = 0;
  for (const k of keys) if (selected.has(k)) hits++;
  if (hits === 0) return "none";
  if (hits === keys.length) return "all";
  return "some";
}

export type DiffPhase = "idle" | "computing" | "ready" | "resolving" | "done" | "error";

export interface DiffProgress {
  checked: number;
  total: number;
  currentDevice: string;
}

export interface ResolveResult {
  deleted: BulkDeleteResult | null;
  copied: DiffCopyResult | null;
  purged: number;
}

export function useProjectDiff(projectId: number, sotDeviceId: string) {
  const [phase, setPhase] = useState<DiffPhase>("idle");
  const [diff, setDiff] = useState<ProjectDiff | null>(null);
  const [progress, setProgress] = useState<DiffProgress | null>(null);
  const [error, setError] = useState("");
  const [selectedDelete, setSelectedDelete] = useState<Set<string>>(new Set());
  const [selectedCopy, setSelectedCopy] = useState<Set<string>>(new Set());
  const [resolveProgress, setResolveProgress] = useState("");
  const [resolveResult, setResolveResult] = useState<ResolveResult | null>(null);
  const [reloadKey, setReloadKey] = useState(0);

  const handleEvent = useCallback((event: DiffEvent) => {
    if (typeof event === "string") return;
    if ("Progress" in event) setProgress(event.Progress);
    else if ("Error" in event) setError(event.Error.message);
  }, []);

  useEffect(() => {
    let cancelled = false;
    setPhase("computing");
    setError("");
    setDiff(null);
    setProgress(null);
    setResolveResult(null);

    computeProjectDiff(projectId, sotDeviceId, handleEvent)
      .then((d) => {
        if (cancelled) return;
        setDiff(d);
        // Both directions start fully selected: the point of the feature is
        // to bring the backups in line unless the user says otherwise.
        const del = new Set<string>();
        const cop = new Set<string>();
        for (const device of d.devices) {
          for (const e of device.toDelete) del.add(entryKey(device.deviceId, e.relativePath));
          for (const e of device.toCopy) cop.add(entryKey(device.deviceId, e.relativePath));
        }
        setSelectedDelete(del);
        setSelectedCopy(cop);
        setPhase("ready");
      })
      .catch((e) => {
        if (cancelled) return;
        setError(String(e));
        setPhase("error");
      });

    return () => {
      cancelled = true;
    };
  }, [projectId, sotDeviceId, reloadKey, handleEvent]);

  const toggleKeys = useCallback(
    (section: "delete" | "copy", keys: string[], next: boolean) => {
      const setter = section === "delete" ? setSelectedDelete : setSelectedCopy;
      setter((prev) => {
        const out = new Set(prev);
        for (const k of keys) {
          if (next) out.add(k);
          else out.delete(k);
        }
        return out;
      });
    },
    []
  );

  const setAll = useCallback(
    (section: "delete" | "copy", next: boolean) => {
      if (!diff) return;
      const keys: string[] = [];
      for (const device of diff.devices) {
        const entries = section === "delete" ? device.toDelete : device.toCopy;
        for (const e of entries) keys.push(entryKey(device.deviceId, e.relativePath));
      }
      toggleKeys(section, keys, next);
    },
    [diff, toggleKeys]
  );

  const deleteLocationIds = useMemo(() => {
    if (!diff) return [];
    const ids: number[] = [];
    for (const device of diff.devices) {
      for (const e of device.toDelete) {
        if (e.locationId !== null && selectedDelete.has(entryKey(device.deviceId, e.relativePath))) {
          ids.push(e.locationId);
        }
      }
    }
    return ids;
  }, [diff, selectedDelete]);

  const copyItems = useMemo<DiffCopyItem[]>(() => {
    if (!diff) return [];
    const items: DiffCopyItem[] = [];
    for (const device of diff.devices) {
      for (const e of device.toCopy) {
        if (e.sourcePath && selectedCopy.has(entryKey(device.deviceId, e.relativePath))) {
          items.push({
            blake3Hash: e.blake3Hash,
            fileSize: e.fileSize,
            fileName: e.fileName,
            sourcePath: e.sourcePath,
            targetDeviceId: device.deviceId,
            relativePath: e.relativePath,
          });
        }
      }
    }
    return items;
  }, [diff, selectedCopy]);

  const resolve = useCallback(
    async (permanent: boolean) => {
      if (!diff) return;
      setPhase("resolving");
      setError("");
      let deleted: BulkDeleteResult | null = null;
      let copied: DiffCopyResult | null = null;
      let purged = 0;

      try {
        if (deleteLocationIds.length > 0) {
          setResolveProgress("Deleting from backups...");
          deleted = await bulkDeleteFileCopies(
            deleteLocationIds,
            (event) => {
              if ("Progress" in event) {
                setResolveProgress(
                  `Deleting ${event.Progress.processed} / ${event.Progress.total}`
                );
              }
            },
            permanent
          );
        }

        if (copyItems.length > 0) {
          setResolveProgress("Copying to backups...");
          copied = await copyDiffFiles(copyItems, (event) => {
            if (typeof event !== "string" && "Progress" in event) {
              const p = event.Progress;
              setResolveProgress(
                `Copying ${p.filesCopied} / ${p.totalFiles} - ${p.currentFile}`
              );
            }
          });
        }

        if (diff.purgeLocationIds.length > 0) {
          setResolveProgress("Updating the index...");
          purged = await purgeDiffLocations(diff.purgeLocationIds);
        }

        setResolveResult({ deleted, copied, purged });
        setPhase("done");
        notifyDone(
          "Diff resolved",
          `${deleted?.succeeded.length ?? 0} deleted, ${copied?.copied ?? 0} copied`
        );
      } catch (e: any) {
        setError(String(e));
        setResolveResult({ deleted, copied, purged });
        setPhase("done");
      } finally {
        setResolveProgress("");
      }
    },
    [diff, deleteLocationIds, copyItems]
  );

  const cancel = useCallback(() => {
    cancelProjectDiff().catch(() => {});
  }, []);

  const reload = useCallback(() => setReloadKey((k) => k + 1), []);

  return {
    phase,
    diff,
    progress,
    error,
    selectedDelete,
    selectedCopy,
    deleteLocationIds,
    copyItems,
    toggleKeys,
    setAll,
    resolve,
    resolveProgress,
    resolveResult,
    cancel,
    reload,
  };
}
```

- [ ] **Step 2: Verify the build**

Run: `npm run build`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add src/hooks/useProjectDiff.ts
git commit -m "diff: useProjectDiff hook with pure tree and selection helpers"
```

---

### Task 8: LazyThumb

**Files:**
- Create: `src/components/LazyThumb.tsx`

**Interfaces:**
- Consumes: the existing `FilePreview` from `src/components/FilePreview.tsx` and the existing `FileLocation` type.
- Produces: `<LazyThumb locations={FileLocation[]} fileName={string} preferredDeviceId?={string} />`.

- [ ] **Step 1: Write the component**

Create `src/components/LazyThumb.tsx`:

```tsx
import { useEffect, useRef, useState } from "react";
import { FilePreview } from "./FilePreview";
import type { FileLocation } from "../types";

interface Props {
  locations: FileLocation[];
  fileName: string;
  preferredDeviceId?: string;
}

/**
 * Mounts a `<FilePreview>` only once the row is on screen.
 *
 * A diff can list thousands of files, and every mounted preview kicks off a
 * thumbnail generation for RAW and video. Mounting them all makes the page
 * unusable, so the work waits until the user actually scrolls there.
 */
export function LazyThumb({ locations, fileName, preferredDeviceId }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    if (visible) return;
    const node = ref.current;
    if (!node) return;

    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          setVisible(true);
          observer.disconnect();
        }
      },
      // Start a little early so a thumbnail is usually ready by the time the
      // row reaches the viewport.
      { rootMargin: "200px" }
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, [visible]);

  return (
    <div className="lazy-thumb" ref={ref}>
      {visible && (
        <FilePreview
          locations={locations}
          fileName={fileName}
          preferredDeviceId={preferredDeviceId}
        />
      )}
    </div>
  );
}
```

- [ ] **Step 2: Verify the build**

Run: `npm run build`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add src/components/LazyThumb.tsx
git commit -m "diff: lazy thumbnail wrapper"
```

---

### Task 9: ProjectDiff page

**Files:**
- Create: `src/pages/ProjectDiff.tsx`
- Create: `src/pages/ProjectDiff.css`

**Interfaces:**
- Consumes: `useProjectDiff`, `buildTree`, `collectKeys`, `folderState`, `entryKey`, `TreeNode` from Task 7; `LazyThumb` from Task 8; `formatBytes` from `src/utils/format`.
- Produces: `<ProjectDiff projectId={number} projectTitle={string} sotDeviceId={string} onBack={() => void} />` and the exported helper `previewLocation(deviceId, entry) -> FileLocation`, used again in Task 10.

- [ ] **Step 1: Write the page**

Create `src/pages/ProjectDiff.tsx`:

```tsx
import { useMemo, useState } from "react";
import {
  useProjectDiff,
  buildTree,
  collectKeys,
  folderState,
  entryKey,
  type TreeNode,
} from "../hooks/useProjectDiff";
import { LazyThumb } from "../components/LazyThumb";
import { formatBytes } from "../utils/format";
import type { DiffDeviceResult, DiffFileEntry, FileLocation } from "../types";
import "./ProjectDiff.css";

type Section = "delete" | "copy";

interface Props {
  projectId: number;
  projectTitle: string;
  sotDeviceId: string;
  onBack: () => void;
}

/** A stand-in FileLocation so <FilePreview> can resolve a path. */
export function previewLocation(deviceId: string, entry: DiffFileEntry): FileLocation {
  return {
    id: entry.locationId ?? 0,
    blake3Hash: entry.blake3Hash,
    deviceId,
    filePath: entry.relativePath,
    fileName: entry.fileName,
    fileSize: entry.fileSize,
    modifiedAt: null,
    lastVerified: "",
    scanMode: "diff",
  };
}

function FolderCheckbox({
  state,
  onChange,
}: {
  state: "all" | "none" | "some";
  onChange: (next: boolean) => void;
}) {
  return (
    <input
      type="checkbox"
      checked={state === "all"}
      ref={(el) => {
        if (el) el.indeterminate = state === "some";
      }}
      onChange={() => onChange(state !== "all")}
    />
  );
}

function Folder({
  deviceId,
  node,
  section,
  selected,
  toggleKeys,
  previewDeviceId,
  depth,
}: {
  deviceId: string;
  node: TreeNode;
  section: Section;
  selected: Set<string>;
  toggleKeys: (section: Section, keys: string[], next: boolean) => void;
  previewDeviceId: string;
  depth: number;
}) {
  // Deep folders and very large ones start closed: a day folder with
  // thousands of files should not render thousands of rows unasked.
  const [open, setOpen] = useState(depth < 2 && node.files.length <= 500);
  const keys = useMemo(() => collectKeys(deviceId, node), [deviceId, node]);
  const state = folderState(keys, selected);

  return (
    <div className="diff-folder" style={{ marginLeft: depth === 0 ? 0 : 16 }}>
      {node.path !== "" && (
        <div className="diff-folder-header">
          <FolderCheckbox state={state} onChange={(next) => toggleKeys(section, keys, next)} />
          <button className="diff-folder-name" onClick={() => setOpen((o) => !o)}>
            {open ? "-" : "+"} {node.name}
          </button>
          <span className="diff-folder-count">{keys.length}</span>
        </div>
      )}

      {open && (
        <>
          {node.children.map((child) => (
            <Folder
              key={child.path}
              deviceId={deviceId}
              node={child}
              section={section}
              selected={selected}
              toggleKeys={toggleKeys}
              previewDeviceId={previewDeviceId}
              depth={depth + 1}
            />
          ))}
          {node.files.map((file) => {
            const key = entryKey(deviceId, file.relativePath);
            return (
              <label key={key} className="diff-file-row">
                <input
                  type="checkbox"
                  checked={selected.has(key)}
                  onChange={(e) => toggleKeys(section, [key], e.target.checked)}
                />
                <LazyThumb
                  locations={[previewLocation(previewDeviceId, file)]}
                  fileName={file.fileName}
                  preferredDeviceId={previewDeviceId}
                />
                <span className="diff-file-name">{file.fileName}</span>
                <span className="diff-file-size">{formatBytes(file.fileSize)}</span>
              </label>
            );
          })}
        </>
      )}
    </div>
  );
}

function DeviceSection({
  device,
  section,
  selected,
  toggleKeys,
  sotDeviceId,
}: {
  device: DiffDeviceResult;
  section: Section;
  selected: Set<string>;
  toggleKeys: (section: Section, keys: string[], next: boolean) => void;
  sotDeviceId: string;
}) {
  const entries = section === "delete" ? device.toDelete : device.toCopy;
  const tree = useMemo(() => buildTree(entries), [entries]);

  if (device.skipReason) {
    return (
      <div className="diff-device diff-device-skipped">
        <strong>{device.deviceLabel}</strong>
        <span className="text-muted-color"> - {device.skipReason}</span>
      </div>
    );
  }
  if (entries.length === 0) return null;

  // Deletions preview from the backup that still holds the file; copies
  // preview from the source of truth, the only place they exist.
  const previewDeviceId = section === "delete" ? device.deviceId : sotDeviceId;
  const keys = collectKeys(device.deviceId, tree);
  const bytes = section === "delete" ? device.deleteBytes : device.copyBytes;

  return (
    <div className="diff-device">
      <div className="diff-device-header">
        <FolderCheckbox
          state={folderState(keys, selected)}
          onChange={(next) => toggleKeys(section, keys, next)}
        />
        <strong>{device.deviceLabel}</strong>
        <span className="text-muted-color">
          {entries.length} file{entries.length !== 1 ? "s" : ""}, {formatBytes(bytes)}
        </span>
      </div>
      <Folder
        deviceId={device.deviceId}
        node={tree}
        section={section}
        selected={selected}
        toggleKeys={toggleKeys}
        previewDeviceId={previewDeviceId}
        depth={0}
      />
    </div>
  );
}

export function ProjectDiff({ projectId, projectTitle, sotDeviceId, onBack }: Props) {
  const {
    phase,
    diff,
    progress,
    error,
    selectedDelete,
    selectedCopy,
    deleteLocationIds,
    copyItems,
    toggleKeys,
    setAll,
    cancel,
  } = useProjectDiff(projectId, sotDeviceId);

  const [section, setSection] = useState<Section>("delete");
  const selected = section === "delete" ? selectedDelete : selectedCopy;

  return (
    <div className="page">
      <div className="page-header">
        <div>
          <h1>Diff - {projectTitle}</h1>
          {diff && (
            <p className="text-muted-color text-xs mt-4">
              Source of truth: <strong>{diff.sotLabel}</strong>
            </p>
          )}
        </div>
        <div className="flex-row gap-8">
          <button onClick={onBack}>Back</button>
        </div>
      </div>

      {phase === "computing" && (
        <div className="diff-progress">
          <div className="progress-bar">
            <div
              className="progress-fill"
              style={{
                width: `${progress && progress.total ? (progress.checked / progress.total) * 100 : 0}%`,
              }}
            />
          </div>
          <div className="progress-stats">
            <span>Checking files on disk...</span>
            <span>
              {progress?.checked ?? 0} / {progress?.total ?? 0}
            </span>
          </div>
          <button className="btn-danger" onClick={cancel}>
            Cancel
          </button>
        </div>
      )}

      {error && <div className="error-msg">{error}</div>}

      {diff && (
        <>
          <div className="stats-grid mb-20">
            <div className="stat-card">
              <div className="stat-value">{diff.totalDeleteFiles}</div>
              <div className="stat-label">
                To delete, {formatBytes(diff.totalDeleteBytes)}
              </div>
            </div>
            <div className="stat-card">
              <div className="stat-value">{diff.totalCopyFiles}</div>
              <div className="stat-label">To copy, {formatBytes(diff.totalCopyBytes)}</div>
            </div>
            <div className="stat-card">
              <div className="stat-value">{diff.unreadable.length}</div>
              <div className="stat-label">Unreadable</div>
            </div>
          </div>

          {diff.unreadable.length > 0 && (
            <div className="diff-warning">
              {diff.unreadable.length} file{diff.unreadable.length !== 1 ? "s" : ""} could
              not be read and were left out of the diff entirely. Nothing will happen to
              them.
              <details>
                <summary>Show</summary>
                {diff.unreadable.slice(0, 100).map((u, i) => (
                  <div key={i} className="text-xs text-muted-color">
                    {u.deviceId}: {u.relativePath} - {u.error}
                  </div>
                ))}
              </details>
            </div>
          )}

          <div className="browser-controls">
            <div className="filter-toggle">
              <button
                className={section === "delete" ? "active" : ""}
                onClick={() => setSection("delete")}
              >
                Delete from backups ({deleteLocationIds.length})
              </button>
              <button
                className={section === "copy" ? "active" : ""}
                onClick={() => setSection("copy")}
              >
                Copy to backups ({copyItems.length})
              </button>
            </div>
            <div className="flex-row gap-8">
              <button onClick={() => setAll(section, true)}>Select all</button>
              <button onClick={() => setAll(section, false)}>Select none</button>
            </div>
          </div>

          <div className="diff-listing">
            {diff.devices.map((device) => (
              <DeviceSection
                key={`${section}-${device.deviceId}`}
                device={device}
                section={section}
                selected={selected}
                toggleKeys={toggleKeys}
                sotDeviceId={diff.sotDeviceId}
              />
            ))}
          </div>
        </>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Write the styles**

Create `src/pages/ProjectDiff.css`:

```css
.diff-progress {
  margin-bottom: 20px;
}

.diff-warning {
  border: 1px solid var(--border);
  border-left: 3px solid var(--warn);
  border-radius: 4px;
  padding: 10px 12px;
  margin-bottom: 16px;
  font-size: 13px;
}

.diff-listing {
  margin-top: 12px;
}

.diff-device {
  border: 1px solid var(--border);
  border-radius: 6px;
  margin-bottom: 12px;
  padding: 10px 12px;
}

.diff-device-skipped {
  opacity: 0.6;
}

.diff-device-header,
.diff-folder-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 0;
}

.diff-folder-name {
  background: none;
  border: none;
  padding: 0;
  cursor: pointer;
  color: inherit;
  font: inherit;
}

.diff-folder-count {
  font-size: 12px;
  color: var(--text-muted);
}

.diff-file-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 3px 0 3px 16px;
  cursor: pointer;
}

.diff-file-row:hover {
  background: var(--bg-hover);
}

.diff-file-name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.diff-file-size {
  font-size: 12px;
  color: var(--text-muted);
}

.lazy-thumb {
  width: 48px;
  height: 48px;
  flex: 0 0 48px;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  border-radius: 3px;
  background: var(--bg-hover);
}

.lazy-thumb .file-preview,
.lazy-thumb .preview-container {
  width: 100%;
  height: 100%;
}

.lazy-thumb .preview-img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.lazy-thumb .preview-loading,
.lazy-thumb .preview-error {
  font-size: 9px;
  text-align: center;
  color: var(--text-muted);
}
```

- [ ] **Step 3: Verify the build**

Run: `npm run build`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src/pages/ProjectDiff.tsx src/pages/ProjectDiff.css
git commit -m "diff: project diff view with per-folder selection"
```

---

### Task 10: Resolve modal

**Files:**
- Create: `src/components/DiffResolveModal.tsx`
- Modify: `src/pages/ProjectDiff.tsx`
- Modify: `src/pages/ProjectDiff.css`

**Interfaces:**
- Consumes: `useFocusTrap` from `src/hooks/useFocusTrap`, `PermanentToggle` from `src/components/FileTable`, `LazyThumb` from Task 8, `formatBytes`, `previewLocation` from Task 9, and `ResolveResult` from Task 7.
- Produces: `<DiffResolveModal ... />` and the `DeletePreviewItem` type, wired into `ProjectDiff` behind a "Resolve diff" button.

- [ ] **Step 1: Write the modal**

Create `src/components/DiffResolveModal.tsx`:

```tsx
import { useState } from "react";
import { useFocusTrap } from "../hooks/useFocusTrap";
import { PermanentToggle } from "./FileTable";
import { LazyThumb } from "./LazyThumb";
import { formatBytes } from "../utils/format";
import type { FileLocation } from "../types";
import type { ResolveResult } from "../hooks/useProjectDiff";

/** One file the user is about to delete, with a location that can be previewed. */
export interface DeletePreviewItem {
  key: string;
  fileName: string;
  fileSize: number;
  deviceLabel: string;
  location: FileLocation;
}

interface Props {
  sotLabel: string;
  deleteItems: DeletePreviewItem[];
  copyCount: number;
  copyBytes: number;
  purgeCount: number;
  busy: boolean;
  progressText: string;
  result: ResolveResult | null;
  onConfirm: (permanent: boolean) => void;
  onClose: () => void;
}

export function DiffResolveModal({
  sotLabel,
  deleteItems,
  copyCount,
  copyBytes,
  purgeCount,
  busy,
  progressText,
  result,
  onConfirm,
  onClose,
}: Props) {
  const trapRef = useFocusTrap<HTMLDivElement>();
  const [permanent, setPermanent] = useState(false);
  const [confirmText, setConfirmText] = useState("");

  const deleteBytes = deleteItems.reduce((sum, i) => sum + i.fileSize, 0);
  // A move to the Trash is recoverable and needs no extra friction; a
  // permanent delete follows SourceCleanupModal and asks for the label.
  const confirmed = !permanent || confirmText === sotLabel;
  const canResolve = confirmed && (deleteItems.length > 0 || copyCount > 0);

  return (
    <div
      className="modal-overlay"
      onClick={() => !busy && !result && onClose()}
      role="dialog"
      aria-label="Resolve diff"
      aria-modal="true"
      onKeyDown={(e) => {
        if (e.key === "Escape" && !busy) onClose();
      }}
    >
      <div
        className="modal-content diff-resolve-modal"
        ref={trapRef}
        onClick={(e) => e.stopPropagation()}
      >
        {result ? (
          <>
            <h2>Diff resolved</h2>
            <p>
              {result.deleted?.succeeded.length ?? 0} deleted, {result.copied?.copied ?? 0}{" "}
              copied, {result.purged} index row{result.purged !== 1 ? "s" : ""} cleaned up.
            </p>
            {(result.copied?.skipped.length ?? 0) > 0 && (
              <>
                <p className="bulk-delete-warning">
                  {result.copied!.skipped.length} copy target
                  {result.copied!.skipped.length !== 1 ? "s" : ""} already held a file and
                  were left alone:
                </p>
                <div className="bulk-delete-file-list">
                  {result.copied!.skipped.map((s, i) => (
                    <div key={i} className="bulk-delete-file-item">
                      <span className="bulk-delete-file-path">{s.fileName}</span>
                      <span className="text-muted-color text-xs">{s.error}</span>
                    </div>
                  ))}
                </div>
              </>
            )}
            {((result.deleted?.failed.length ?? 0) > 0 ||
              (result.copied?.failed.length ?? 0) > 0) && (
              <>
                <p className="bulk-delete-warning">Failures:</p>
                <div className="bulk-delete-file-list">
                  {result.deleted?.failed.map((f, i) => (
                    <div key={`d${i}`} className="bulk-delete-file-item">
                      <span className="bulk-delete-file-path">{f.filePath}</span>
                      <span className="text-muted-color text-xs">{f.error}</span>
                    </div>
                  ))}
                  {result.copied?.failed.map((f, i) => (
                    <div key={`c${i}`} className="bulk-delete-file-item">
                      <span className="bulk-delete-file-path">{f.fileName}</span>
                      <span className="text-muted-color text-xs">{f.error}</span>
                    </div>
                  ))}
                </div>
              </>
            )}
            <div className="form-actions">
              <button className="btn-primary" onClick={onClose}>
                Close
              </button>
            </div>
          </>
        ) : busy ? (
          <>
            <h2>Resolving...</h2>
            <p className="text-muted-color">{progressText || "Working..."}</p>
          </>
        ) : (
          <>
            <h2>Resolve diff</h2>
            <p>
              {deleteItems.length} file{deleteItems.length !== 1 ? "s" : ""} (
              {formatBytes(deleteBytes)}) will be removed from the backups, and {copyCount}{" "}
              file{copyCount !== 1 ? "s" : ""} ({formatBytes(copyBytes)}) copied to them.
            </p>
            {purgeCount > 0 && (
              <p className="text-muted-color text-xs">
                {purgeCount} index row{purgeCount !== 1 ? "s" : ""} for files already gone
                from {sotLabel} will be cleaned up, including for files you deselected.
              </p>
            )}

            {deleteItems.length > 0 && (
              <>
                <p className="bulk-delete-warning">About to be deleted:</p>
                <div className="diff-resolve-grid">
                  {deleteItems.map((item) => (
                    <div key={item.key} className="diff-resolve-tile">
                      <LazyThumb
                        locations={[item.location]}
                        fileName={item.fileName}
                        preferredDeviceId={item.location.deviceId}
                      />
                      <span className="diff-resolve-tile-name" title={item.fileName}>
                        {item.fileName}
                      </span>
                      <span className="diff-resolve-tile-device">{item.deviceLabel}</span>
                    </div>
                  ))}
                </div>
              </>
            )}

            <PermanentToggle permanent={permanent} onChange={setPermanent} />
            {permanent && (
              <div className="form-group" style={{ marginTop: 12 }}>
                <label>
                  Type "<strong>{sotLabel}</strong>" to confirm
                </label>
                <input
                  type="text"
                  value={confirmText}
                  onChange={(e) => setConfirmText(e.target.value)}
                  placeholder={sotLabel}
                  autoFocus
                />
              </div>
            )}

            <div className="form-actions">
              <button onClick={onClose}>Cancel</button>
              <button
                className="btn-danger"
                onClick={() => onConfirm(permanent)}
                disabled={!canResolve}
              >
                Resolve
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Add its styles**

Append to `src/pages/ProjectDiff.css`:

```css
.diff-resolve-modal {
  max-width: 720px;
}

.diff-resolve-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(96px, 1fr));
  gap: 8px;
  max-height: 320px;
  overflow-y: auto;
  padding: 8px;
  border: 1px solid var(--border);
  border-radius: 4px;
  margin-bottom: 12px;
}

.diff-resolve-tile {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
}

.diff-resolve-tile .lazy-thumb {
  width: 88px;
  height: 88px;
  flex: 0 0 88px;
}

.diff-resolve-tile-name,
.diff-resolve-tile-device {
  font-size: 10px;
  max-width: 96px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.diff-resolve-tile-device {
  color: var(--text-muted);
}
```

- [ ] **Step 3: Wire the modal into the page**

In `src/pages/ProjectDiff.tsx`, add the import:

```tsx
import { DiffResolveModal, type DeletePreviewItem } from "../components/DiffResolveModal";
```

Add `resolve`, `resolveProgress`, `resolveResult` and `reload` to the values destructured from `useProjectDiff`, and add local state:

```tsx
  const [resolving, setResolving] = useState(false);
```

Build the preview list and the selected copy size, next to the other `useMemo` calls in `ProjectDiff`:

```tsx
  const deletePreviewItems = useMemo<DeletePreviewItem[]>(() => {
    if (!diff) return [];
    const items: DeletePreviewItem[] = [];
    for (const device of diff.devices) {
      for (const e of device.toDelete) {
        const key = entryKey(device.deviceId, e.relativePath);
        if (!selectedDelete.has(key)) continue;
        items.push({
          key,
          fileName: e.fileName,
          fileSize: e.fileSize,
          deviceLabel: device.deviceLabel,
          location: previewLocation(device.deviceId, e),
        });
      }
    }
    return items;
  }, [diff, selectedDelete]);

  const selectedCopyBytes = useMemo(
    () => copyItems.reduce((sum, i) => sum + i.fileSize, 0),
    [copyItems]
  );
```

Add a Resolve button to the page header, next to Back:

```tsx
          <button
            className="btn-danger"
            disabled={
              phase !== "ready" ||
              (deleteLocationIds.length === 0 && copyItems.length === 0)
            }
            onClick={() => setResolving(true)}
          >
            Resolve diff
          </button>
```

And render the modal as the last child of the outer `<div className="page">`:

```tsx
      {resolving && diff && (
        <DiffResolveModal
          sotLabel={diff.sotLabel}
          deleteItems={deletePreviewItems}
          copyCount={copyItems.length}
          copyBytes={selectedCopyBytes}
          purgeCount={diff.purgeLocationIds.length}
          busy={phase === "resolving"}
          progressText={resolveProgress}
          result={resolveResult}
          onConfirm={resolve}
          onClose={() => {
            const didResolve = resolveResult !== null;
            setResolving(false);
            // The index and the disks both moved; recompute rather than show
            // a diff that no longer describes reality.
            if (didResolve) reload();
          }}
        />
      )}
```

- [ ] **Step 4: Verify the build**

Run: `npm run build`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add src/components/DiffResolveModal.tsx src/pages/ProjectDiff.tsx src/pages/ProjectDiff.css
git commit -m "diff: resolve dialog with a preview grid of what gets deleted"
```

---

### Task 11: Projects entry point and app wiring

**Files:**
- Modify: `src/pages/Projects.tsx`
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: `getProjectDiffDevices` from Task 6, `<ProjectDiff>` from Tasks 9 and 10.
- Produces: an `onShowDiff(projectId, projectTitle, sotDeviceId)` prop on `Projects`, and a `projectDiff` page in `App`.

- [ ] **Step 1: Add the dropdown to the project detail view**

In `src/pages/Projects.tsx`:

Add to the imports (merge into the existing `../api/commands` and `../types` imports rather than duplicating them):

```tsx
import { getProjectDiffDevices } from "../api/commands";
import type { DiffDeviceOption } from "../types";
```

Extend `ProjectsProps`:

```tsx
  onShowDiff: (projectId: number, projectTitle: string, sotDeviceId: string) => void;
```

and add `onShowDiff` to the destructured parameter list of `export function Projects(...)`.

Add state near the other `useState` calls:

```tsx
  const [diffDevices, setDiffDevices] = useState<DiffDeviceOption[]>([]);
  const [sotDeviceId, setSotDeviceId] = useState("");
```

Load the options whenever a project detail is opened — place this with the other effects:

```tsx
  useEffect(() => {
    const id = selected?.project.id;
    if (view !== "detail" || id === undefined) return;
    setSotDeviceId("");
    let cancelled = false;
    getProjectDiffDevices(id)
      .then((d) => {
        if (!cancelled) setDiffDevices(d);
      })
      .catch(() => {
        if (!cancelled) setDiffDevices([]);
      });
    return () => {
      cancelled = true;
    };
  }, [view, selected?.project.id]);
```

In the detail header's button row, before the Transfer button:

```tsx
            <select
              value={sotDeviceId}
              onChange={(e) => setSotDeviceId(e.target.value)}
              aria-label="Source of truth"
            >
              <option value="">Source of truth...</option>
              {diffDevices.map((d) => (
                <option key={d.deviceId} value={d.deviceId} disabled={!d.isConnected}>
                  {d.label} ({d.fileCount}){d.isConnected ? "" : " - offline"}
                </option>
              ))}
            </select>
            <button
              disabled={!sotDeviceId}
              onClick={() => onShowDiff(project.id, project.title, sotDeviceId)}
            >
              Show diff
            </button>
```

- [ ] **Step 2: Wire the page into App**

In `src/App.tsx`:

Add the import:

```tsx
import { ProjectDiff } from "./pages/ProjectDiff";
```

Extend the `Page` union with `| "projectDiff"`.

Add state:

```tsx
  const [diffTarget, setDiffTarget] = useState<{
    projectId: number;
    projectTitle: string;
    sotDeviceId: string;
  } | null>(null);
```

Add the handler beside `handleTransferProject`:

```tsx
  const handleShowDiff = useCallback(
    (projectId: number, projectTitle: string, sotDeviceId: string) => {
      setDiffTarget({ projectId, projectTitle, sotDeviceId });
      setPage("projectDiff");
    },
    []
  );
```

Pass it through:

```tsx
        {page === "projects" && (
          <Projects
            onTransferProject={handleTransferProject}
            onShowDiff={handleShowDiff}
            initialProjectId={openProject}
          />
        )}
```

And render the page. It is unmounted when not active — unlike Move or Import, a diff describes a moment in time, so returning to it should recompute rather than show a stale answer:

```tsx
        {page === "projectDiff" && diffTarget && (
          <ProjectDiff
            projectId={diffTarget.projectId}
            projectTitle={diffTarget.projectTitle}
            sotDeviceId={diffTarget.sotDeviceId}
            onBack={() => {
              setOpenProject(diffTarget.projectId);
              setDiffTarget(null);
              setPage("projects");
            }}
          />
        )}
```

No sidebar button: the diff is only reachable from a project.

- [ ] **Step 3: Verify the build**

Run: `npm run build`
Expected: clean.

- [ ] **Step 4: Manual smoke test**

Run: `npm run tauri dev`

Walk through:
1. Projects, then open a project with files on two or more devices.
2. The Source of truth dropdown lists those devices with file counts; offline ones are disabled and labelled.
3. Pick a connected device, Show diff becomes enabled, click it.
4. The diff view shows a progress bar, then the two sections.
5. Delete a file from the source-of-truth disk in Finder, go back, re-run the diff: that file now appears under "Delete from backups", checked.
6. Deselect a folder; the count in the tab drops accordingly.
7. Resolve diff shows thumbnails of exactly the selected files. Cancel out without confirming and check nothing changed on disk.
8. Resolve for real with one file and confirm it lands in the Trash on the backup and disappears from the diff after the automatic recompute.

- [ ] **Step 5: Commit**

```bash
git add src/pages/Projects.tsx src/App.tsx
git commit -m "diff: source-of-truth picker on the project screen"
```

---

### Task 12: Release build and dmg

`CLAUDE.md` requires the dmg to be rebuilt whenever the application changes.

**Files:** none modified.

- [ ] **Step 1: Run the full test suite**

Run: `cd src-tauri && cargo test`
Expected: all pass.

- [ ] **Step 2: Build the release bundle**

Run: `npm run tauri build`
Expected: succeeds; the dmg lands in `src-tauri/target/release/bundle/dmg/`.

- [ ] **Step 3: Confirm the artifact**

Run: `ls -lh src-tauri/target/release/bundle/dmg/`
Expected: a `.dmg` with a current timestamp.

- [ ] **Step 4: Commit anything the build changed**

```bash
git status --short
```

Commit only if the build touched tracked files:

```bash
git commit -am "build: refresh bundle after project diff feature"
```

---

## Notes for the implementer

- `db::get_project_files` returns `FileSafety` values whose `locations` cover *every* device, including ones not in the diff. `probe_locations` drops locations whose device has no entry in the `mounts` map, which is how offline devices stay out of the classification.
- `bulk_delete_file_copies` already sends `BulkDeleteEvent::Progress` before each file and calls `cleanup_orphaned_files` at the end. Do not duplicate that logic.
- `upsert_location`'s last argument is the scan mode string. Copies made by this feature use `"diff-sync"` so they stay distinguishable from `"import"` and `"full"` rows later.
- The error variant used throughout is `AppError::General(String)`.
- `tempfile` is already a dev-dependency (used by `mover.rs` tests); no Cargo changes are needed.
