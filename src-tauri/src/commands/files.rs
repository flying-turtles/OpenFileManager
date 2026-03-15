use std::path::PathBuf;

use tauri::State;

use super::AppState;
use crate::db;
use crate::error::AppError;
use crate::models::*;

#[tauri::command]
pub async fn get_files_on_device(
    state: State<'_, AppState>,
    device_id: String,
) -> Result<Vec<FileLocation>, AppError> {
    db::get_files_on_device(&state.pool, &device_id).await
}

#[tauri::command]
pub async fn get_files_on_device_page(
    state: State<'_, AppState>,
    device_id: String,
    cursor: Option<String>,
    limit: Option<i64>,
) -> Result<FilePageResult, AppError> {
    let limit = limit.unwrap_or(500);
    let (files, next_cursor, total) =
        db::get_files_on_device_page(&state.pool, &device_id, cursor.as_deref(), limit).await?;
    Ok(FilePageResult { files, next_cursor, total })
}

#[tauri::command]
pub async fn get_unsafe_files_page(
    state: State<'_, AppState>,
    offset: Option<i64>,
    limit: Option<i64>,
) -> Result<UnsafeFilePageResult, AppError> {
    let (files, total, has_more) =
        db::get_unsafe_files_page(&state.pool, offset.unwrap_or(0), limit.unwrap_or(500)).await?;
    Ok(UnsafeFilePageResult { files, total, has_more })
}

#[tauri::command]
pub async fn get_safe_files_page(
    state: State<'_, AppState>,
    offset: Option<i64>,
    limit: Option<i64>,
) -> Result<UnsafeFilePageResult, AppError> {
    let (files, total, has_more) =
        db::get_safe_files_page(&state.pool, offset.unwrap_or(0), limit.unwrap_or(500)).await?;
    Ok(UnsafeFilePageResult { files, total, has_more })
}

#[tauri::command]
pub async fn get_duplicate_files_page(
    state: State<'_, AppState>,
    offset: Option<i64>,
    limit: Option<i64>,
) -> Result<UnsafeFilePageResult, AppError> {
    let (files, total, has_more) =
        db::get_duplicate_files_page(&state.pool, offset.unwrap_or(0), limit.unwrap_or(500)).await?;
    Ok(UnsafeFilePageResult { files, total, has_more })
}

#[tauri::command]
pub async fn delete_file_copy(
    state: State<'_, AppState>,
    location_id: i64,
) -> Result<(), AppError> {
    let file_path = db::delete_file_location(&state.pool, location_id).await?;

    let path = std::path::Path::new(&file_path);
    if path.exists() {
        tokio::fs::remove_file(path).await?;
    }

    Ok(())
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
