use std::collections::HashMap;

use tauri::ipc::Channel;
use tauri::State;
use tokio_util::sync::CancellationToken;

use super::AppState;
use crate::db;
use crate::devices;
use crate::error::AppError;
use crate::models::*;
use crate::transfer;

#[tauri::command]
pub async fn check_project_transfer(
    state: State<'_, AppState>,
    project_id: i64,
    target_device_id: String,
) -> Result<TransferCheck, AppError> {
    let project = db::get_project(&state.pool, project_id).await?;
    let files = db::get_project_files(&state.pool, &project.start_date, &project.end_date).await?;

    let volumes = devices::detect_volumes();
    let all_devices = db::get_all_devices(&state.pool).await?;
    let volume_ids: std::collections::HashSet<String> =
        volumes.iter().map(|v| v.id.clone()).collect();

    let mut connected_devices: HashMap<String, StorageDevice> = HashMap::new();
    for mut dev in all_devices {
        let is_connected =
            volume_ids.contains(&dev.id) || std::path::Path::new(&dev.mount_point).exists();
        if is_connected {
            dev.is_connected = true;
            if let Some(vol) = volumes.iter().find(|v| v.id == dev.id) {
                dev.mount_point = vol.mount_point.clone();
            }
            connected_devices.insert(dev.id.clone(), dev);
        }
    }

    let target = connected_devices
        .get(&target_device_id)
        .ok_or_else(|| AppError::General("Target device not connected".into()))?;
    let dest_mount = target.mount_point.clone();
    let dest_label = target.label.clone();

    let (check, resolved) =
        transfer::check_and_resolve(&files, &connected_devices, &target_device_id);

    // Store resolved files + target info for start_project_transfer
    {
        let mut guard = state.transfer_resolved.lock().await;
        *guard = Some((resolved, target_device_id, dest_mount, dest_label));
    }

    Ok(check)
}

#[tauri::command]
pub async fn start_project_transfer(
    state: State<'_, AppState>,
    on_event: Channel<TransferEvent>,
) -> Result<(), AppError> {
    let (resolved, target_device_id, dest_mount, dest_label) = {
        let mut guard = state.transfer_resolved.lock().await;
        guard
            .take()
            .ok_or_else(|| AppError::General("No check result. Run check first.".into()))?
    };

    let pool = state.pool.clone();
    let cancel_token = CancellationToken::new();

    {
        let mut guard = state.transfer_cancel_token.lock().await;
        *guard = Some(cancel_token.clone());
    }

    tokio::spawn(async move {
        if let Err(e) = transfer::run_transfer(
            pool,
            resolved,
            target_device_id,
            dest_mount,
            dest_label,
            on_event.clone(),
            cancel_token,
        )
        .await
        {
            let _ = on_event.send(TransferEvent::Error {
                message: e.to_string(),
            });
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn cancel_project_transfer(state: State<'_, AppState>) -> Result<(), AppError> {
    let guard = state.transfer_cancel_token.lock().await;
    if let Some(token) = guard.as_ref() {
        token.cancel();
    }
    Ok(())
}
