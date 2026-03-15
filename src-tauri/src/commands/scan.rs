use std::path::PathBuf;
use std::sync::Arc;

use tauri::ipc::Channel;
use tauri::State;
use tokio_util::sync::CancellationToken;

use super::AppState;
use crate::db;
use crate::devices;
use crate::error::AppError;
use crate::models::*;
use crate::scanner::ScanProgress;

#[tauri::command]
pub async fn start_scan(
    state: State<'_, AppState>,
    target: String,
    on_event: Channel<ScanEvent>,
) -> Result<(), AppError> {
    let pool = state.pool.clone();
    let cancel_token = CancellationToken::new();
    let progress = Arc::new(ScanProgress::new());

    {
        let mut guard = state.cancel_token.lock().await;
        *guard = Some(cancel_token.clone());
    }
    {
        let mut guard = state.scan_progress.lock().await;
        *guard = Some(progress.clone());
    }
    {
        let mut guard = state.scan_target.lock().await;
        *guard = Some(target.clone());
    }
    // Remove any existing pending scan for this target
    let _ = db::delete_pending_scan_by_target(&pool, "scan", &target).await;

    let target = PathBuf::from(target);
    tokio::spawn(async move {
        if let Err(e) = crate::scanner::run_scan(pool, target, on_event.clone(), cancel_token, progress).await {
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
pub async fn pause_scan(state: State<'_, AppState>) -> Result<(), AppError> {
    // Set pausing flag then cancel
    let progress_guard = state.scan_progress.lock().await;
    if let Some(progress) = progress_guard.as_ref() {
        progress.pausing.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    let token_guard = state.cancel_token.lock().await;
    if let Some(token) = token_guard.as_ref() {
        token.cancel();
    }
    drop(token_guard);
    drop(progress_guard);

    // Persist the pending scan
    let target = state.scan_target.lock().await.clone().unwrap_or_default();
    if !target.is_empty() {
        let progress_guard = state.scan_progress.lock().await;
        if let Some(p) = progress_guard.as_ref() {
            let volumes = devices::detect_volumes();
            let device_id = devices::device_for_path(&volumes, &target)
                .map(|(id, _)| id)
                .unwrap_or_default();
            db::upsert_pending_scan(
                &state.pool,
                "scan",
                &target,
                &device_id,
                "quick",
                p.total.load(std::sync::atomic::Ordering::Relaxed) as i64,
                p.scanned.load(std::sync::atomic::Ordering::Relaxed) as i64,
                p.hashed.load(std::sync::atomic::Ordering::Relaxed) as i64,
                p.added.load(std::sync::atomic::Ordering::Relaxed) as i64,
            ).await?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn get_pending_scans(state: State<'_, AppState>) -> Result<Vec<PendingScan>, AppError> {
    db::get_pending_scans(&state.pool).await
}

#[tauri::command]
pub async fn dismiss_pending_scan(state: State<'_, AppState>, id: i64) -> Result<(), AppError> {
    db::delete_pending_scan(&state.pool, id).await
}
