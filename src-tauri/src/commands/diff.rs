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
