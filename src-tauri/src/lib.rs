mod commands;
mod db;
mod devices;
mod error;
mod hasher;
mod importer;
mod models;
mod network;
mod scanner;
mod transfer;

use std::sync::Arc;
use tauri::{Emitter, Manager};
use tokio::sync::Mutex;

use commands::AppState;
use db::DbPool;
use models::DetectedDisk;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data = app
                .path()
                .app_data_dir()
                .expect("failed to get app data dir");
            let db_path = app_data.join("filemanager.db");

            let rt = tokio::runtime::Runtime::new().expect("failed to create runtime");
            let pool = rt.block_on(async {
                let pool = db::init_pool(&db_path).await.expect("failed to init db pool");
                db::run_migrations(&pool).await.expect("failed to run migrations");
                pool
            });

            let pool_clone = pool.clone();
            app.manage(AppState {
                pool,
                cancel_token: Arc::new(Mutex::new(None)),
                scan_progress: Arc::new(Mutex::new(None)),
                scan_target: Arc::new(Mutex::new(None)),
                import_cancel_token: Arc::new(Mutex::new(None)),
                import_analysis: Arc::new(Mutex::new(None)),
                transfer_cancel_token: Arc::new(Mutex::new(None)),
                transfer_resolved: Arc::new(Mutex::new(None)),
            });

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = auto_mount_network_drives(&pool_clone, &handle).await {
                    log::error!("Auto-mount error: {}", e);
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::detect_devices,
            commands::get_devices,
            commands::set_device_type,
            commands::set_drive_speed,
            commands::remove_device,
            commands::start_scan,
            commands::cancel_scan,
            commands::pause_scan,
            commands::get_pending_scans,
            commands::dismiss_pending_scan,
            commands::get_files_on_device,
            commands::get_files_on_device_page,
            commands::get_file_safety,
            commands::get_unsafe_files,
            commands::get_unsafe_files_page,
            commands::get_safe_files_page,
            commands::get_duplicate_files_page,
            commands::delete_file_copy,
            commands::bulk_delete_file_copies,
            commands::get_waste_candidates,
            commands::browse_directory,
            commands::get_file_locations,
            commands::get_dashboard_stats,
            commands::analyze_sd_card,
            commands::start_import,
            commands::cancel_import,
            commands::eject_device,
            commands::create_project,
            commands::get_projects,
            commands::get_project_detail,
            commands::update_project,
            commands::delete_project,
            commands::add_network_drive,
            commands::get_network_drives,
            commands::mount_network_drive,
            commands::unmount_network_drive,
            commands::remove_network_drive,
            commands::add_location,
            commands::resolve_file_path,
            commands::get_thumbnail,
            commands::open_file,
            commands::check_project_transfer,
            commands::start_project_transfer,
            commands::cancel_project_transfer,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

async fn auto_mount_network_drives(
    pool: &DbPool,
    handle: &tauri::AppHandle,
) -> Result<(), error::AppError> {
    let drives = db::get_all_network_drives(pool).await?;

    for drive in drives {
        let already_mounted = network::is_mountpoint(&drive.mount_point);

        if !already_mounted {
            let result = match drive.protocol.as_str() {
                "smb" => {
                    let password = if !drive.username.is_empty() {
                        network::keychain_load(&drive.id, &drive.username).unwrap_or_default()
                    } else {
                        String::new()
                    };
                    network::mount_smb(
                        &drive.host,
                        &drive.share_path,
                        &drive.username,
                        &password,
                        &drive.mount_point,
                    )
                }
                "nfs" => network::mount_nfs(&drive.host, &drive.share_path, &drive.mount_point),
                _ => continue,
            };

            if let Err(e) = result {
                log::error!("Failed to auto-mount {}: {}", drive.label, e);
                let _ = handle.emit("network-drive-status", serde_json::json!({
                    "id": drive.id,
                    "status": "error",
                    "message": e.to_string(),
                }));
                continue;
            }
        }

        // Upsert into storage_devices
        let disk = DetectedDisk {
            id: drive.id.clone(),
            label: drive.label.clone(),
            mount_point: drive.mount_point.clone(),
            total_bytes: 0,
            available_bytes: 0,
            is_removable: false,
        };
        let _ = db::upsert_device(pool, &disk).await;
        let _ = db::set_device_type(pool, &drive.id, &drive.device_type).await;

        let _ = handle.emit("network-drive-status", serde_json::json!({
            "id": drive.id,
            "status": "mounted",
        }));
    }
    Ok(())
}
