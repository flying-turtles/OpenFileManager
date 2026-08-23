use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Duration;

use futures::future::join_all;
use tauri::ipc::Channel;
use tauri::State;
use tokio_util::sync::CancellationToken;

use super::{path_online_within, AppState};
use crate::db::{self, DbPool};
use crate::devices;
use crate::diff::{self, Presence, SotRecheck};
use crate::error::AppError;
use crate::models::*;

/// Longer deadline than the per-file probes: a sleeping NAS gets a chance to
/// spin up before the whole diff is refused.
const MOUNT_DEADLINE_SECS: u64 = 5;

/// How many indexed files to sample when confirming that the volume mounted
/// at a device's mount point really is that device.
const IDENTITY_SAMPLE: i64 = 20;

/// Deadline for stat-ing the whole identity sample. Bounded so a stalled
/// mount cannot hold the check open; a timeout counts as "found nothing",
/// which refuses the device — the safe direction.
const IDENTITY_SAMPLE_DEADLINE_SECS: u64 = 10;

/// Whether the volume currently at a device's mount point is that device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MountVerdict {
    Ok,
    /// Nothing answered at the mount point within the deadline.
    Unreachable,
    /// A `.filemanagerid` marker naming a different device.
    ForeignMarker(String),
    /// The path answers, but none of the device's indexed files are there.
    /// This is what an unmounted volume looks like: macOS commonly leaves an
    /// empty stub directory under `/Volumes`, and a dropped SMB share leaves
    /// an existing empty directory too, so `path.exists()` alone says nothing.
    NoIndexedFilesFound { sampled: i64 },
}

impl MountVerdict {
    fn is_ok(&self) -> bool {
        *self == MountVerdict::Ok
    }

    /// Human-readable refusal, or `None` when the device checked out.
    fn refusal(&self, device: &StorageDevice) -> Option<String> {
        match self {
            MountVerdict::Ok => None,
            MountVerdict::Unreachable => Some(format!(
                "{} is not reachable at {}",
                device.label, device.mount_point
            )),
            MountVerdict::ForeignMarker(other) => Some(format!(
                "{} is not the disk mounted at {} — that volume identifies itself as {}",
                device.label, device.mount_point, other
            )),
            MountVerdict::NoIndexedFilesFound { sampled } => Some(format!(
                "{} does not look mounted at {} — none of {} sampled indexed files are there",
                device.label, device.mount_point, sampled
            )),
        }
    }
}

/// The identity decision, split out from the IO so it can be tested.
///
/// A "missing" marker is never a refusal on its own: only devices added by
/// hand get a `.filemanagerid` written, so auto-detected volumes legitimately
/// have none. The file sample is what catches the empty stub.
fn judge_mount(
    marker_status: &str,
    foreign_id: Option<String>,
    sampled: i64,
    found: i64,
) -> MountVerdict {
    if marker_status == "mismatch" {
        return MountVerdict::ForeignMarker(foreign_id.unwrap_or_else(|| "another device".into()));
    }
    if sampled > 0 && found == 0 {
        return MountVerdict::NoIndexedFilesFound { sampled };
    }
    MountVerdict::Ok
}

async fn read_marker(mount: &str) -> Option<String> {
    let marker_path = Path::new(mount).join(devices::FILEMANAGER_ID_FILE);
    match tokio::time::timeout(
        Duration::from_secs(MOUNT_DEADLINE_SECS),
        tokio::task::spawn_blocking(move || std::fs::read_to_string(marker_path).ok()),
    )
    .await
    {
        Ok(Ok(contents)) => contents,
        _ => None,
    }
}

/// Confirms the volume at `device.mount_point` really is `device`.
///
/// `path_online_within` only answers "something exists at this path", which an
/// unmounted volume's leftover stub directory satisfies. This mirrors
/// `check_reconnect_target`: read the `.filemanagerid` marker, and stat a
/// sample of the device's indexed files.
pub async fn verify_device_mounted(pool: &DbPool, device: &StorageDevice) -> MountVerdict {
    if !path_online_within(&device.mount_point, MOUNT_DEADLINE_SECS).await {
        return MountVerdict::Unreachable;
    }

    let marker = read_marker(&device.mount_point).await;
    let (marker_status, foreign_id) = devices::evaluate_marker(marker.as_deref(), &device.id);

    let sample = db::get_device_file_sample(pool, &device.id, IDENTITY_SAMPLE)
        .await
        .unwrap_or_default();
    let sampled = sample.len() as i64;
    let base = PathBuf::from(&device.mount_point);
    let found = tokio::time::timeout(
        Duration::from_secs(IDENTITY_SAMPLE_DEADLINE_SECS),
        tokio::task::spawn_blocking(move || {
            sample
                .iter()
                .filter(|f| base.join(&f.file_path).exists())
                .count() as i64
        }),
    )
    .await
    .map(|r| r.unwrap_or(0))
    .unwrap_or(0);

    judge_mount(&marker_status, foreign_id, sampled, found)
}

#[tauri::command]
pub async fn get_project_diff_devices(
    state: State<'_, AppState>,
    project_id: i64,
) -> Result<Vec<DiffDeviceOption>, AppError> {
    let project = db::get_project(&state.pool, project_id).await?;
    let mut options =
        db::get_project_diff_devices(&state.pool, &project.start_date, &project.end_date).await?;

    let devices = db::get_all_devices(&state.pool).await?;
    let by_id: HashMap<String, StorageDevice> =
        devices.into_iter().map(|d| (d.id.clone(), d)).collect();

    // Verified concurrently: each check can take up to ~20s against a stalled
    // mount, and a sequential loop over several devices would block the SoT
    // picker for the sum of all of them. `join_all` preserves the order of
    // `options`, which the UI renders as-is.
    let verdicts = join_all(options.iter().map(|opt| async {
        // Reported as connected only if it would actually be usable as a
        // source of truth — an unmounted disk showing as connected here is an
        // invitation to run a diff that proposes deleting the whole project.
        match by_id.get(&opt.device_id) {
            Some(device) => verify_device_mounted(&state.pool, device).await.is_ok(),
            None => false,
        }
    }))
    .await;
    for (opt, is_connected) in options.iter_mut().zip(verdicts) {
        opt.is_connected = is_connected;
    }

    Ok(options)
}

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

/// Everything the probe pass needs, once the devices have been resolved and
/// gated.
pub struct DiffTargets {
    pub sot: StorageDevice,
    /// Every indexed location of the project's files, across all devices.
    pub locations: Vec<FileLocation>,
    /// Mount points of the source of truth and the verified backups only.
    pub mounts: HashMap<String, String>,
    pub reachable_backups: Vec<String>,
    /// Backups left out of the diff, already shaped as results.
    pub skipped: Vec<DiffDeviceResult>,
    pub all_devices: Vec<StorageDevice>,
}

/// Resolves the source of truth and the backup devices, and refuses the diff
/// outright when the source of truth is not the disk it claims to be.
///
/// Split out of `compute_inner` so the gate that stands between a stale mount
/// and a whole-project deletion is a named, directly testable unit rather
/// than something only reachable through a Tauri command.
pub async fn resolve_diff_targets(
    pool: &DbPool,
    project_id: i64,
    sot_device_id: &str,
) -> Result<DiffTargets, AppError> {
    let project = db::get_project(pool, project_id).await?;
    let all_devices = db::get_all_devices(pool).await?;

    let sot = all_devices
        .iter()
        .find(|d| d.id == sot_device_id)
        .ok_or_else(|| AppError::General("Source of truth device is not known".into()))?
        .clone();

    // Without this the probes would report every file as gone and the diff
    // would propose deleting the entire project from the backups.
    if let Some(reason) = verify_device_mounted(pool, &sot).await.refusal(&sot) {
        return Err(AppError::General(format!(
            "{} — reconnect it and try again",
            reason
        )));
    }

    let files = db::get_project_files(pool, &project.start_date, &project.end_date).await?;
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
        let verdict = match device {
            Some(d) => verify_device_mounted(pool, d).await,
            None => MountVerdict::Unreachable,
        };
        match (device, verdict.is_ok()) {
            (Some(d), true) => {
                mounts.insert(id.clone(), d.mount_point.clone());
                reachable_backups.push(id.clone());
            }
            (device, _) => skipped.push(DiffDeviceResult {
                device_id: id.clone(),
                device_label: device.map(|d| d.label.clone()).unwrap_or_else(|| id.clone()),
                skip_reason: Some(
                    device
                        .and_then(|d| verdict.refusal(d))
                        .unwrap_or_else(|| "Not reachable".to_string()),
                ),
                to_delete: Vec::new(),
                to_copy: Vec::new(),
                delete_bytes: 0,
                copy_bytes: 0,
            }),
        }
    }

    Ok(DiffTargets {
        sot,
        locations,
        mounts,
        reachable_backups,
        skipped,
        all_devices,
    })
}

/// The mid-pass source-of-truth check the probe pass calls periodically.
struct MountedSot<'a> {
    pool: &'a DbPool,
    device: &'a StorageDevice,
}

impl SotRecheck for MountedSot<'_> {
    fn still_mounted(&self) -> Pin<Box<dyn Future<Output = bool> + Send + '_>> {
        Box::pin(async move { verify_device_mounted(self.pool, self.device).await.is_ok() })
    }
}

/// Below this many decided files, "all gone" says nothing — a four-frame
/// project fully rejected is 100% gone and perfectly legitimate.
const GONE_BACKSTOP_MIN_FILES: usize = 20;

/// `true` when the source of truth's probe results look like a missing disk
/// rather than a cull. Unreadable files are excluded: they are already kept
/// out of both lists and say nothing either way.
///
/// Gated on zero presence, not a high fraction. Every failure mode this
/// backstop exists to catch — unmounted volume, wrong disk — makes every
/// sampled file read as gone, because the disk is not there; there is no
/// slack to buy. A fractional threshold only produces false positives: a
/// photographer culling 95%+ of a shoot is ordinary in wildlife and sports
/// work, and even one present file proves the disk is mounted and readable.
fn sot_looks_wrong(present: usize, gone: usize) -> bool {
    present == 0 && gone >= GONE_BACKSTOP_MIN_FILES
}

async fn compute_inner(
    state: &State<'_, AppState>,
    project_id: i64,
    sot_device_id: &str,
    on_event: &Channel<DiffEvent>,
    cancel: &CancellationToken,
) -> Result<ProjectDiff, AppError> {
    let targets = resolve_diff_targets(&state.pool, project_id, sot_device_id).await?;
    let DiffTargets {
        sot,
        locations,
        mounts,
        reachable_backups,
        skipped,
        all_devices,
    } = targets;

    let recheck = MountedSot {
        pool: &state.pool,
        device: &sot,
    };
    let pass = diff::probe_locations(locations, &mounts, cancel, on_event, Some(&recheck)).await;

    if cancel.is_cancelled() {
        return Err(AppError::General("Cancelled".into()));
    }

    if pass.sot_lost {
        return Err(AppError::General(format!(
            "{} went offline during the diff",
            sot.label
        )));
    }

    // A mount that dropped after the last mid-pass check would look like a
    // mass deletion.
    if let Some(reason) = verify_device_mounted(&state.pool, &sot).await.refusal(&sot) {
        return Err(AppError::General(format!("{} during the diff", reason)));
    }

    let (sot_present, sot_gone) = pass
        .probed
        .iter()
        .filter(|p| p.device_id == sot.id)
        .fold((0usize, 0usize), |(p, g), loc| match loc.presence {
            Presence::Present => (p + 1, g),
            Presence::Gone => (p, g + 1),
            Presence::Unknown => (p, g),
        });
    if sot_looks_wrong(sot_present, sot_gone) {
        return Err(AppError::General(format!(
            "Refusing the diff: {} of {} files are missing from {}. \
             That looks like the wrong disk rather than a cull — check it is \
             mounted and reconnected, then try again",
            sot_gone,
            sot_present + sot_gone,
            sot.label
        )));
    }

    let out = diff::classify(&pass.probed, &sot.id, &sot.mount_point, &reachable_backups);

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

#[tauri::command]
pub async fn copy_diff_files(
    state: State<'_, AppState>,
    items: Vec<DiffCopyItem>,
    on_event: Channel<DiffCopyEvent>,
) -> Result<DiffCopyResult, AppError> {
    let cancel = CancellationToken::new();
    *state.diff_copy_cancel_token.lock().await = Some(cancel.clone());

    let result = copy_inner(&state, items, &on_event, &cancel).await;

    *state.diff_copy_cancel_token.lock().await = None;
    result
}

async fn copy_inner(
    state: &State<'_, AppState>,
    items: Vec<DiffCopyItem>,
    on_event: &Channel<DiffCopyEvent>,
    cancel: &CancellationToken,
) -> Result<DiffCopyResult, AppError> {
    let devices = db::get_all_devices(&state.pool).await?;

    // Nothing else checks the targets. A backup that dropped since the diff
    // was computed leaves a writable directory behind when it sits under the
    // home directory, and the whole project would be written to the boot disk
    // and indexed as if it were on the backup.
    let mut target_ids: Vec<String> = items.iter().map(|i| i.target_device_id.clone()).collect();
    target_ids.sort();
    target_ids.dedup();

    let mut unusable: Vec<String> = Vec::new();
    for id in &target_ids {
        match devices.iter().find(|d| d.id == *id) {
            Some(device) => {
                if let Some(reason) = verify_device_mounted(&state.pool, device).await.refusal(device)
                {
                    unusable.push(reason);
                }
            }
            None => unusable.push(format!("{} is not a known device", id)),
        }
    }
    if !unusable.is_empty() {
        return Err(AppError::General(format!(
            "Nothing was copied — {}",
            unusable.join("; ")
        )));
    }

    let mounts: HashMap<String, String> =
        devices.into_iter().map(|d| (d.id, d.mount_point)).collect();

    Ok(diff::run_diff_copy(
        &state.pool,
        items,
        &mounts,
        |e| {
            let _ = on_event.send(e);
        },
        cancel,
    )
    .await)
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
        match db::delete_file_location_no_cleanup(&state.pool, id).await {
            Ok(rows_affected) => purged += rows_affected,
            Err(e) => log::warn!("Failed to purge diff location {}: {}", id, e),
        }
    }
    if purged > 0 {
        let _ = db::cleanup_orphaned_files(&state.pool).await;
    }
    Ok(purged)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_foreign_marker_refuses_the_device() {
        assert_eq!(
            judge_mount("mismatch", Some("other-disk".into()), 20, 20),
            MountVerdict::ForeignMarker("other-disk".into())
        );
    }

    #[test]
    fn an_unmounted_stub_directory_refuses_the_device() {
        // The stub exists, so `path.exists()` passed, but none of the
        // device's indexed files are under it.
        assert_eq!(
            judge_mount("missing", None, 20, 0),
            MountVerdict::NoIndexedFilesFound { sampled: 20 }
        );
    }

    #[test]
    fn a_missing_marker_alone_does_not_refuse_a_mounted_device() {
        // Auto-detected volumes never get a `.filemanagerid` written.
        assert_eq!(judge_mount("missing", None, 20, 18), MountVerdict::Ok);
    }

    #[test]
    fn a_device_with_no_indexed_files_cannot_be_judged_by_sample() {
        assert_eq!(judge_mount("match", None, 0, 0), MountVerdict::Ok);
    }

    async fn seeded_device(mount: &Path, files: &[&str]) -> (DbPool, StorageDevice) {
        let dir = tempfile::tempdir().unwrap();
        let pool = db::init_pool(&dir.path().join("test.db")).await.unwrap();
        db::run_migrations(&pool).await.unwrap();
        std::mem::forget(dir);

        db::upsert_device(
            &pool,
            &DetectedDisk {
                id: "dev-1".into(),
                label: "Media".into(),
                mount_point: mount.to_string_lossy().into_owned(),
                total_bytes: 0,
                available_bytes: 0,
                is_removable: false,
            },
        )
        .await
        .unwrap();
        for (i, f) in files.iter().enumerate() {
            let hash = format!("h{i}");
            db::upsert_file(&pool, &hash, 10, f, "cr3").await.unwrap();
            db::upsert_location(&pool, &hash, "dev-1", f, f, 10, None, "full")
                .await
                .unwrap();
        }
        let device = db::get_all_devices(&pool)
            .await
            .unwrap()
            .into_iter()
            .find(|d| d.id == "dev-1")
            .unwrap();
        (pool, device)
    }

    #[tokio::test]
    async fn an_empty_stub_directory_is_not_the_device() {
        // Exactly what an unmounted volume leaves behind: the mount point
        // exists — `path_online_within` is happy — and holds nothing.
        let tmp = tempfile::tempdir().unwrap();
        let stub = tmp.path().join("Media");
        std::fs::create_dir_all(&stub).unwrap();
        let (pool, device) = seeded_device(&stub, &["2026-08-14/a.cr3"]).await;

        assert!(super::path_online_within(&device.mount_point, 5).await);
        assert_eq!(
            verify_device_mounted(&pool, &device).await,
            MountVerdict::NoIndexedFilesFound { sampled: 1 }
        );
    }

    #[tokio::test]
    async fn a_really_mounted_device_verifies() {
        let tmp = tempfile::tempdir().unwrap();
        let mount = tmp.path().join("Media");
        std::fs::create_dir_all(mount.join("2026-08-14")).unwrap();
        std::fs::write(mount.join("2026-08-14/a.cr3"), b"x").unwrap();
        let (pool, device) = seeded_device(&mount, &["2026-08-14/a.cr3"]).await;

        assert_eq!(verify_device_mounted(&pool, &device).await, MountVerdict::Ok);
    }

    #[tokio::test]
    async fn a_volume_carrying_another_devices_marker_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let mount = tmp.path().join("Media");
        std::fs::create_dir_all(mount.join("2026-08-14")).unwrap();
        std::fs::write(mount.join("2026-08-14/a.cr3"), b"x").unwrap();
        std::fs::write(mount.join(devices::FILEMANAGER_ID_FILE), "someone-else").unwrap();
        let (pool, device) = seeded_device(&mount, &["2026-08-14/a.cr3"]).await;

        assert_eq!(
            verify_device_mounted(&pool, &device).await,
            MountVerdict::ForeignMarker("someone-else".into())
        );
    }

    #[tokio::test]
    async fn resolve_diff_targets_refuses_an_unmounted_source_of_truth() {
        let tmp = tempfile::tempdir().unwrap();
        let stub = tmp.path().join("Media");
        std::fs::create_dir_all(&stub).unwrap();
        let (pool, _device) = seeded_device(&stub, &["2026-08-14/a.cr3"]).await;
        let project = db::create_project(&pool, "Shoot", "", "2026-08-14", "2026-08-14")
            .await
            .unwrap();

        let err = match resolve_diff_targets(&pool, project.id, "dev-1").await {
            Err(e) => e,
            Ok(_) => panic!("an unmounted source of truth must not drive a diff"),
        };
        assert!(
            err.to_string().contains("does not look mounted"),
            "got: {err}"
        );
    }

    #[test]
    fn the_backstop_refuses_a_total_disappearance() {
        assert!(sot_looks_wrong(0, 3000));
        assert!(sot_looks_wrong(0, 20));
    }

    #[test]
    fn the_backstop_allows_an_ordinary_cull() {
        // 60% of a shoot rejected in Lightroom is a normal day.
        assert!(!sot_looks_wrong(400, 600));
    }

    #[test]
    fn the_backstop_allows_a_heavy_but_not_total_cull() {
        // 99.9% rejected in a wildlife/sports burst is ordinary, and even one
        // present file proves the disk is mounted and readable.
        assert!(!sot_looks_wrong(1, 999));
    }

    #[test]
    fn the_backstop_ignores_projects_too_small_to_judge() {
        // Four frames, all rejected: legitimate, and the ratio says nothing.
        assert!(!sot_looks_wrong(0, 4));
    }
}
