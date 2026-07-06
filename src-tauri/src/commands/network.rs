use tauri::State;

use super::AppState;
use crate::db;
use crate::error::AppError;
use crate::models::*;
use crate::network;

#[tauri::command]
pub async fn add_network_drive(
    state: State<'_, AppState>,
    protocol: String,
    host: String,
    share_path: String,
    username: String,
    password: String,
    label: String,
    device_type: String,
) -> Result<NetworkDrive, AppError> {
    let id = network::generate_drive_id(&protocol, &host, &share_path);
    let mount_point = network::default_mount_point(&label);

    let drive = NetworkDrive {
        id: id.clone(),
        label,
        protocol,
        host,
        share_path,
        username: username.clone(),
        mount_point,
        device_type,
        created_at: String::new(),
        is_mounted: false,
    };

    db::insert_network_drive(&state.pool, &drive).await?;

    if !password.is_empty() {
        network::keychain_store(&id, &username, &password)?;
    }

    let mut saved = db::get_network_drive(&state.pool, &id).await?;
    saved.is_mounted = network::is_mountpoint(&saved.mount_point);
    Ok(saved)
}

#[tauri::command]
pub async fn get_network_drives(state: State<'_, AppState>) -> Result<Vec<NetworkDrive>, AppError> {
    let mut drives = db::get_all_network_drives(&state.pool).await?;
    for drive in &mut drives {
        drive.is_mounted = network::is_mountpoint(&drive.mount_point);
    }
    Ok(drives)
}

#[tauri::command]
pub async fn mount_network_drive(state: State<'_, AppState>, drive_id: String) -> Result<(), AppError> {
    let mut drive = db::get_network_drive(&state.pool, &drive_id).await?;

    // Drives added before the fix point at /Volumes/<label>, which normal
    // users cannot create on modern macOS — migrate them to the home dir.
    if drive.mount_point.starts_with("/Volumes/") && !network::is_mountpoint(&drive.mount_point) {
        let new_mount = network::default_mount_point(&drive.label);
        db::update_network_drive_mount_point(&state.pool, &drive_id, &new_mount).await?;
        drive.mount_point = new_mount;
    }

    match drive.protocol.as_str() {
        "smb" => {
            // Adopt a mount that already exists (manual mount_smbfs, Finder)
            // instead of failing with "File exists"
            if let Some(existing) = network::find_existing_smb_mount(&drive.host, &drive.share_path) {
                if existing != drive.mount_point {
                    db::update_network_drive_mount_point(&state.pool, &drive_id, &existing).await?;
                    drive.mount_point = existing;
                }
            } else {
                let password = if !drive.username.is_empty() {
                    network::keychain_load(&drive_id, &drive.username).unwrap_or_default()
                } else {
                    String::new()
                };
                network::mount_smb(&drive.host, &drive.share_path, &drive.username, &password, &drive.mount_point)?;
            }
        }
        "nfs" => {
            network::mount_nfs(&drive.host, &drive.share_path, &drive.mount_point)?;
        }
        _ => return Err(AppError::General(format!("Unsupported protocol: {}", drive.protocol))),
    }

    // Id marker at the mount root lets scans map paths to this drive
    network::ensure_id_marker(&drive.mount_point, &drive.id);

    // Upsert into storage_devices so scanning/file tracking works
    let disk = DetectedDisk {
        id: drive.id.clone(),
        label: drive.label.clone(),
        mount_point: drive.mount_point.clone(),
        total_bytes: 0,
        available_bytes: 0,
        is_removable: false,
    };
    db::upsert_device(&state.pool, &disk).await?;
    db::set_device_type(&state.pool, &drive.id, &drive.device_type).await?;

    Ok(())
}

#[tauri::command]
pub async fn unmount_network_drive(state: State<'_, AppState>, drive_id: String) -> Result<(), AppError> {
    let drive = db::get_network_drive(&state.pool, &drive_id).await?;
    network::unmount_drive(&drive.mount_point)?;
    Ok(())
}

#[tauri::command]
pub async fn remove_network_drive(state: State<'_, AppState>, drive_id: String) -> Result<(), AppError> {
    let drive = db::get_network_drive(&state.pool, &drive_id).await?;

    // Unmount if mounted
    if network::is_mountpoint(&drive.mount_point) {
        let _ = network::unmount_drive(&drive.mount_point);
    }

    // Delete keychain entry
    if !drive.username.is_empty() {
        let _ = network::keychain_delete(&drive_id, &drive.username);
    }

    db::delete_network_drive(&state.pool, &drive_id).await?;
    Ok(())
}
