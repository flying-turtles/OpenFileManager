use std::path::{Path, PathBuf};

use tauri::{Manager, State};

use super::AppState;
use crate::db;
use crate::error::AppError;

#[tauri::command]
pub async fn resolve_file_path(
    state: State<'_, AppState>,
    device_id: String,
    file_path: String,
) -> Result<String, AppError> {
    let device = db::get_device(&state.pool, &device_id).await?;
    let full = Path::new(&device.mount_point).join(&file_path);
    if !super::path_online(&full).await {
        return Err(AppError::General(format!(
            "File not reachable (drive offline?): {}",
            full.display()
        )));
    }
    Ok(full.to_string_lossy().into_owned())
}

fn thumbnail_cache_dir(app: &tauri::AppHandle) -> Result<PathBuf, AppError> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::General(e.to_string()))?
        .join("thumbnails");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn cache_key(path: &str, size: u32) -> String {
    let input = format!("{}:{}", path, size);
    let hash = blake3::hash(input.as_bytes());
    format!("{}.png", &hash.to_hex()[..16])
}

#[tauri::command]
pub async fn get_thumbnail(
    app: tauri::AppHandle,
    path: String,
    max_size: Option<u32>,
) -> Result<String, AppError> {
    let size = max_size.unwrap_or(512);
    let cache_dir = thumbnail_cache_dir(&app)?;
    let key = cache_key(&path, size);
    let cached_path = cache_dir.join(&key);

    if cached_path.exists() {
        return Ok(cached_path.to_string_lossy().into_owned());
    }

    // Cap concurrent qlmanage processes and give each a deadline: a stalled
    // network mount otherwise accumulates hundreds of hung child processes.
    static QL_SEM: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);
    let _permit = QL_SEM
        .acquire()
        .await
        .map_err(|e| AppError::General(e.to_string()))?;

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        tokio::process::Command::new("qlmanage")
            .args([
                "-t",
                "-s",
                &size.to_string(),
                "-o",
                &cache_dir.to_string_lossy(),
                &path,
            ])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| AppError::General(format!("thumbnail timed out: {}", path)))??;

    if !output.status.success() {
        return Err(AppError::General(format!(
            "qlmanage failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    // qlmanage outputs to {cache_dir}/{filename}.png
    let source_name = Path::new(&path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let ql_output = cache_dir.join(format!("{}.png", source_name));

    if ql_output.exists() {
        std::fs::rename(&ql_output, &cached_path)?;
    } else {
        // qlmanage sometimes uses different naming; find any new png
        let mut found = false;
        if let Ok(entries) = std::fs::read_dir(&cache_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with(&*source_name) && name.ends_with(".png") && name != key {
                    std::fs::rename(entry.path(), &cached_path)?;
                    found = true;
                    break;
                }
            }
        }
        if !found {
            return Err(AppError::General(
                "qlmanage produced no output".into(),
            ));
        }
    }

    Ok(cached_path.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn open_file(
    state: State<'_, AppState>,
    device_id: String,
    file_path: String,
) -> Result<(), AppError> {
    let device = db::get_device(&state.pool, &device_id).await?;
    let full = Path::new(&device.mount_point).join(&file_path);
    if !super::path_online(&full).await {
        return Err(AppError::General(format!(
            "File not reachable (drive offline?): {}",
            full.display()
        )));
    }
    let output = tokio::process::Command::new("open")
        .arg(&full)
        .output()
        .await?;
    if !output.status.success() {
        return Err(AppError::General(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }
    Ok(())
}

/// Reveal a file in Finder via `open -R`. Runs as a subprocess instead of
/// NSWorkspace: AppKit is main-thread-only and throws an ObjC exception on
/// tokio workers, which aborts the process.
#[tauri::command]
pub async fn reveal_in_finder(path: String) -> Result<(), AppError> {
    let output = tokio::process::Command::new("open")
        .args(["-R", &path])
        .output()
        .await?;
    if !output.status.success() {
        return Err(AppError::General(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }
    Ok(())
}
