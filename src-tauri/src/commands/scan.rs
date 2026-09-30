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

/// Rows shown in the cleanup preview before the list is truncated. The
/// counts stay exact; only the rendered list is capped.
const CLEANUP_PREVIEW_LIMIT: i64 = 500;

/// What the index knows about a scanned location. Derived entirely from the
/// DB rather than from scan counters: a scan skips unchanged directories, so
/// its counters describe the run, not the location.
#[tauri::command]
pub async fn get_scan_summary(
    state: State<'_, AppState>,
    target: String,
) -> Result<ScanSummary, AppError> {
    let scope = crate::scanner::resolve_scan_scope(std::path::Path::new(&target))?;
    let pool = &state.pool;

    let (total_files, total_bytes, oldest_modified, newest_modified) =
        db::get_scan_location_stats(pool, &scope.device_id, &scope.scan_prefix).await?;
    let (projects, unassigned_files) =
        db::get_scan_project_breakdown(pool, &scope.device_id, &scope.scan_prefix).await?;
    let (redundant_files, redundant_bytes) =
        db::get_scan_redundancy_totals(pool, &scope.device_id, &scope.scan_prefix).await?;
    let device_groups =
        db::get_scan_device_groups(pool, &scope.device_id, &scope.scan_prefix).await?;

    let device_label = db::get_device(pool, &scope.device_id)
        .await
        .map(|d| d.label)
        .unwrap_or_else(|_| scope.device_id.clone());

    Ok(ScanSummary {
        device_id: scope.device_id,
        device_label,
        scan_prefix: scope.scan_prefix,
        total_files,
        total_bytes,
        oldest_modified,
        newest_modified,
        projects,
        unassigned_files,
        device_groups,
        redundant_files,
        redundant_bytes,
    })
}

/// Builds the cleanup preview for a scanned location. `limit` caps the
/// returned file list; the counts always reflect every eligible file.
async fn build_scan_cleanup_preview(
    state: &AppState,
    target: &str,
    limit: Option<i64>,
) -> Result<(SourceCleanupPreview, String), AppError> {
    let scope = crate::scanner::resolve_scan_scope(std::path::Path::new(target))?;
    let pool = &state.pool;

    let rows =
        db::get_scan_redundant_files(pool, &scope.device_id, &scope.scan_prefix, limit).await?;
    let (redundant_files, redundant_bytes) =
        db::get_scan_redundancy_totals(pool, &scope.device_id, &scope.scan_prefix).await?;
    let (total_files, ..) =
        db::get_scan_location_stats(pool, &scope.device_id, &scope.scan_prefix).await?;

    let hashes: Vec<String> = rows.iter().map(|(_, _, _, h)| h.clone()).collect();
    let locations_map = db::get_locations_for_hashes(pool, &hashes).await?;

    let files = rows
        .into_iter()
        .map(|(file_path, file_name, file_size, hash)| {
            // Same rule the SQL filtered on, re-expressed for display: which
            // other devices hold a same-sized copy.
            let mut backup_device_ids: Vec<String> = locations_map
                .get(&hash)
                .map(|locs| {
                    locs.iter()
                        .filter(|l| l.device_id != scope.device_id && l.file_size == file_size)
                        .map(|l| l.device_id.clone())
                        .collect()
                })
                .unwrap_or_default();
            backup_device_ids.sort();
            backup_device_ids.dedup();

            SourceCleanupFile {
                source_path: std::path::Path::new(&scope.mount_point)
                    .join(&file_path)
                    .to_string_lossy()
                    .to_string(),
                relative_path: file_path,
                file_name,
                file_size,
                backup_device_ids,
            }
        })
        .collect();

    let device_label = db::get_device(pool, &scope.device_id)
        .await
        .map(|d| d.label)
        .unwrap_or_else(|_| scope.device_id.clone());

    let label = if scope.scan_prefix.is_empty() {
        device_label
    } else {
        format!("{}/{}", device_label, scope.scan_prefix)
    };

    Ok((
        SourceCleanupPreview {
            sd_device_id: scope.device_id,
            sd_label: label,
            files,
            file_count: redundant_files,
            total_bytes: redundant_bytes,
            skipped_count: total_files.saturating_sub(redundant_files) as u64,
        },
        scope.mount_point,
    ))
}

#[tauri::command]
pub async fn get_scan_cleanup_preview(
    state: State<'_, AppState>,
    target: String,
) -> Result<SourceCleanupPreview, AppError> {
    let (preview, _) =
        build_scan_cleanup_preview(&state, &target, Some(CLEANUP_PREVIEW_LIMIT)).await?;
    Ok(preview)
}

/// Deletes every file in the scanned location that also lives on at least two
/// other devices. Eligibility is recomputed here rather than taken from the
/// frontend, so the delete set can never drift from what the DB supports.
#[tauri::command]
pub async fn delete_redundant_scanned_files(
    state: State<'_, AppState>,
    target: String,
    permanent: Option<bool>,
    on_event: Channel<SourceCleanupEvent>,
) -> Result<SourceCleanupResult, AppError> {
    let (preview, mount_point) = build_scan_cleanup_preview(&state, &target, None).await?;
    let permanent = permanent.unwrap_or(false);

    // Network mounts stall rather than error, so check reachability before
    // starting a long delete loop.
    if !preview.files.is_empty() && !super::path_online_within(&mount_point, 5).await {
        return Err(AppError::General(format!(
            "{} is not reachable — reconnect it and try again",
            preview.sd_label
        )));
    }

    let total = preview.files.len() as u64;
    let mut deleted: u64 = 0;
    let mut bytes_freed: i64 = 0;
    let mut failed = Vec::new();

    for (i, f) in preview.files.iter().enumerate() {
        let _ = on_event.send(SourceCleanupEvent::Progress {
            processed: i as u64,
            total,
            current_file: f.file_name.clone(),
        });

        match super::files::remove_from_disk(PathBuf::from(&f.source_path), permanent).await {
            Ok(()) => {
                deleted += 1;
                bytes_freed += f.file_size;
                let _ = db::delete_location_by_device_and_path(
                    &state.pool,
                    &preview.sd_device_id,
                    &f.relative_path,
                )
                .await;
            }
            Err(e) => failed.push(SourceCleanupError {
                source_path: f.source_path.clone(),
                error: e.to_string(),
            }),
        }
    }

    if deleted > 0 {
        let _ = db::cleanup_orphaned_files(&state.pool).await;
    }

    let result = SourceCleanupResult {
        deleted,
        bytes_freed,
        failed,
    };
    let _ = on_event.send(SourceCleanupEvent::Complete(result.clone()));
    Ok(result)
}

#[tauri::command]
pub async fn get_pending_scans(state: State<'_, AppState>) -> Result<Vec<PendingScan>, AppError> {
    db::get_pending_scans(&state.pool).await
}

#[tauri::command]
pub async fn dismiss_pending_scan(state: State<'_, AppState>, id: i64) -> Result<(), AppError> {
    db::delete_pending_scan(&state.pool, id).await
}
