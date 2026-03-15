use std::collections::HashSet;

use tauri::State;

use super::{mark_connected, AppState};
use crate::db;
use crate::devices;
use crate::error::AppError;
use crate::models::*;

#[tauri::command]
pub async fn detect_devices(state: State<'_, AppState>) -> Result<Vec<StorageDevice>, AppError> {
    let disks = devices::detect_volumes();
    let connected_ids: HashSet<String> = disks.iter().map(|d| d.id.clone()).collect();
    for disk in &disks {
        db::upsert_device(&state.pool, disk).await?;
    }
    let all = db::get_all_devices(&state.pool).await?;
    Ok(mark_connected(all, &connected_ids))
}

#[tauri::command]
pub async fn get_devices(state: State<'_, AppState>) -> Result<Vec<StorageDevice>, AppError> {
    let disks = devices::detect_volumes();
    let connected_ids: HashSet<String> = disks.iter().map(|d| d.id.clone()).collect();
    let all = db::get_all_devices(&state.pool).await?;
    Ok(mark_connected(all, &connected_ids))
}

#[tauri::command]
pub async fn set_device_type(
    state: State<'_, AppState>,
    device_id: String,
    device_type: String,
) -> Result<(), AppError> {
    db::set_device_type(&state.pool, &device_id, &device_type).await
}

#[tauri::command]
pub async fn remove_device(
    state: State<'_, AppState>,
    device_id: String,
) -> Result<(), AppError> {
    db::delete_device(&state.pool, &device_id).await
}

#[tauri::command]
pub async fn add_location(
    state: State<'_, AppState>,
    path: String,
    label: String,
    device_type: String,
) -> Result<StorageDevice, AppError> {
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err(AppError::General(format!("Path does not exist: {}", path)));
    }

    // Reuse existing .filemanagerid if present, otherwise generate a new UUID
    let id_file = p.join(devices::FILEMANAGER_ID_FILE);
    let id = if id_file.is_file() {
        let contents = std::fs::read_to_string(&id_file)
            .map_err(|e| AppError::General(format!("Failed to read {}: {}", id_file.display(), e)))?;
        let existing = contents.trim().to_string();
        if existing.is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            existing
        }
    } else {
        uuid::Uuid::new_v4().to_string()
    };

    // Write the .filemanagerid file
    std::fs::write(&id_file, &id)
        .map_err(|e| AppError::General(format!("Failed to write {}: {}", id_file.display(), e)))?;

    let disk = DetectedDisk {
        id: id.clone(),
        label,
        mount_point: path.clone(),
        total_bytes: 0,
        available_bytes: 0,
        is_removable: false,
    };
    db::upsert_device(&state.pool, &disk).await?;
    db::set_device_type(&state.pool, &id, &device_type).await?;

    let devices = db::get_all_devices(&state.pool).await?;
    devices
        .into_iter()
        .find(|d| d.id == id)
        .ok_or_else(|| AppError::General("Failed to retrieve added device".into()))
}

#[tauri::command]
pub async fn eject_device(mount_point: String) -> Result<(), AppError> {
    let output = std::process::Command::new("diskutil")
        .args(["eject", &mount_point])
        .output()?;
    if !output.status.success() {
        return Err(AppError::General(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }
    Ok(())
}
