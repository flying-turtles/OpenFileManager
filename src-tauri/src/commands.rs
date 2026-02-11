use std::path::PathBuf;
use std::sync::Arc;

use tauri::ipc::Channel;
use tauri::State;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::db::{self, DbPool};
use crate::devices;
use crate::error::AppError;
use crate::models::*;

pub struct AppState {
    pub pool: DbPool,
    pub cancel_token: Arc<Mutex<Option<CancellationToken>>>,
}

#[tauri::command]
pub async fn detect_devices(state: State<'_, AppState>) -> Result<Vec<StorageDevice>, AppError> {
    let disks = devices::detect_volumes();
    for disk in &disks {
        db::upsert_device(&state.pool, disk).await?;
    }
    db::get_all_devices(&state.pool).await
}

#[tauri::command]
pub async fn get_devices(state: State<'_, AppState>) -> Result<Vec<StorageDevice>, AppError> {
    db::get_all_devices(&state.pool).await
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
pub async fn start_scan(
    state: State<'_, AppState>,
    target: String,
    mode: String,
    on_event: Channel<ScanEvent>,
) -> Result<(), AppError> {
    let pool = state.pool.clone();
    let cancel_token = CancellationToken::new();

    {
        let mut guard = state.cancel_token.lock().await;
        *guard = Some(cancel_token.clone());
    }

    let target = PathBuf::from(target);
    tokio::spawn(async move {
        if let Err(e) = crate::scanner::run_scan(pool, target, mode, on_event.clone(), cancel_token).await {
            let _ = on_event.send(ScanEvent::Error {
                message: e.to_string(),
            });
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn cancel_scan(state: State<'_, AppState>) -> Result<(), AppError> {
    let guard = state.cancel_token.lock().await;
    if let Some(token) = guard.as_ref() {
        token.cancel();
    }
    Ok(())
}

#[tauri::command]
pub async fn get_files_on_device(
    state: State<'_, AppState>,
    device_id: String,
) -> Result<Vec<FileLocation>, AppError> {
    db::get_files_on_device(&state.pool, &device_id).await
}

#[tauri::command]
pub async fn get_file_safety(
    state: State<'_, AppState>,
    hash: String,
) -> Result<Option<FileSafety>, AppError> {
    db::get_file_safety(&state.pool, &hash).await
}

#[tauri::command]
pub async fn get_unsafe_files(state: State<'_, AppState>) -> Result<Vec<FileSafety>, AppError> {
    db::get_unsafe_files(&state.pool).await
}

#[tauri::command]
pub async fn get_waste_candidates(
    state: State<'_, AppState>,
    threshold: Option<i64>,
) -> Result<Vec<WasteCandidate>, AppError> {
    db::get_waste_candidates(&state.pool, threshold.unwrap_or(2)).await
}

#[tauri::command]
pub async fn browse_directory(path: String) -> Result<Vec<DirEntry>, AppError> {
    let path = PathBuf::from(&path);
    let mut entries = Vec::new();

    let mut dir = tokio::fs::read_dir(&path).await?;
    while let Some(entry) = dir.next_entry().await? {
        let metadata = entry.metadata().await?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::SystemTime::UNIX_EPOCH).ok())
            .map(|d| {
                chrono::DateTime::from_timestamp(d.as_secs() as i64, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            })
            .flatten();

        entries.push(DirEntry {
            name,
            is_dir: metadata.is_dir(),
            size: metadata.len() as i64,
            modified,
        });
    }

    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(entries)
}

#[tauri::command]
pub async fn get_file_locations(
    state: State<'_, AppState>,
    hash: String,
) -> Result<Vec<FileLocation>, AppError> {
    db::get_file_locations(&state.pool, &hash).await
}

#[tauri::command]
pub async fn get_dashboard_stats(state: State<'_, AppState>) -> Result<DashboardStats, AppError> {
    db::get_dashboard_stats(&state.pool).await
}
