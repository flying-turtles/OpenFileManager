mod commands;
mod db;
mod devices;
mod error;
mod hasher;
mod models;
mod scanner;

use std::sync::Arc;
use tauri::Manager;
use tokio::sync::Mutex;

use commands::AppState;

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

            app.manage(AppState {
                pool,
                cancel_token: Arc::new(Mutex::new(None)),
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::detect_devices,
            commands::get_devices,
            commands::set_device_type,
            commands::start_scan,
            commands::cancel_scan,
            commands::get_files_on_device,
            commands::get_file_safety,
            commands::get_unsafe_files,
            commands::get_waste_candidates,
            commands::browse_directory,
            commands::get_file_locations,
            commands::get_dashboard_stats,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
