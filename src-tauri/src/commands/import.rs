use std::path::PathBuf;
use std::sync::Arc;

use tauri::ipc::Channel;
use tauri::State;
use tokio_util::sync::CancellationToken;

use super::AppState;
use crate::error::AppError;
use crate::models::*;

#[tauri::command]
pub async fn analyze_sd_card(
    state: State<'_, AppState>,
    sd_mount: String,
    on_event: Channel<ImportEvent>,
) -> Result<(), AppError> {
    let pool = state.pool.clone();
    let import_analysis = state.import_analysis.clone();
    let cancel_token = CancellationToken::new();

    {
        let mut guard = state.import_cancel_token.lock().await;
        *guard = Some(cancel_token.clone());
    }

    tokio::spawn(async move {
        match crate::importer::analyze_sd_card(pool, PathBuf::from(sd_mount), on_event.clone(), cancel_token).await {
            Ok(analysis) => {
                let analysis_arc = Arc::new(analysis.clone());
                *import_analysis.lock().await = Some(analysis_arc);
                let _ = on_event.send(ImportEvent::AnalysisComplete(analysis));
            }
            Err(e) => {
                if e.to_string() != "Cancelled" {
                    let _ = on_event.send(ImportEvent::Error { message: e.to_string() });
                }
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn start_import(
    state: State<'_, AppState>,
    target_device_ids: Vec<String>,
    on_event: Channel<ImportEvent>,
) -> Result<(), AppError> {
    let analysis = {
        let guard = state.import_analysis.lock().await;
        guard
            .clone()
            .ok_or_else(|| AppError::General("No analysis available. Run analyze first.".into()))?
    };

    let pool = state.pool.clone();
    let cancel_token = CancellationToken::new();

    {
        let mut guard = state.import_cancel_token.lock().await;
        *guard = Some(cancel_token.clone());
    }

    let all_devices = crate::db::get_all_devices(&pool).await?;
    let mut targets = Vec::new();
    for id in target_device_ids {
        let dev = all_devices
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| AppError::General(format!("Device {} not found", id)))?;
        if !std::path::Path::new(&dev.mount_point).exists() {
            return Err(AppError::General(format!("Device {} ({}) is not connected", dev.label, dev.mount_point)));
        }
        targets.push((dev.id.clone(), dev.mount_point.clone(), dev.label.clone()));
    }

    tokio::spawn(async move {
        if let Err(e) = crate::importer::run_import(pool, analysis, targets, on_event.clone(), cancel_token).await {
            let _ = on_event.send(ImportEvent::Error {
                message: e.to_string(),
            });
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn get_import_cleanup_preview(
    state: State<'_, AppState>,
) -> Result<SourceCleanupPreview, AppError> {
    let analysis = {
        let guard = state.import_analysis.lock().await;
        guard
            .clone()
            .ok_or_else(|| AppError::General("No analysis available. Run analyze first.".into()))?
    };
    crate::importer::compute_source_cleanup(&state.pool, &analysis).await
}

#[tauri::command]
pub async fn delete_imported_source_files(
    state: State<'_, AppState>,
    permanent: Option<bool>,
    on_event: Channel<SourceCleanupEvent>,
) -> Result<SourceCleanupResult, AppError> {
    let analysis = {
        let guard = state.import_analysis.lock().await;
        guard
            .clone()
            .ok_or_else(|| AppError::General("No analysis available. Run analyze first.".into()))?
    };

    // Recompute against current DB state so the delete list can't drift from
    // what the preview promised (e.g. copies made or removed since).
    let preview = crate::importer::compute_source_cleanup(&state.pool, &analysis).await?;
    let permanent = permanent.unwrap_or(false);

    if let Some(first) = preview.files.first() {
        if !super::path_online_within(&first.source_path, 5).await {
            return Err(AppError::General(format!(
                "{} is not reachable — reconnect it and try again",
                preview.sd_label
            )));
        }
    }

    // DB file_paths are relative to the device mount point, while the
    // analysis paths are relative to the (possibly narrowed) analyze folder.
    let sd_mount = crate::db::get_device(&state.pool, &preview.sd_device_id)
        .await
        .ok()
        .map(|d| d.mount_point);

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
                // Drop any tracked location of this file on the source device
                if let Some(mp) = &sd_mount {
                    if let Ok(rel) = std::path::Path::new(&f.source_path).strip_prefix(mp) {
                        let _ = crate::db::delete_location_by_device_and_path(
                            &state.pool,
                            &preview.sd_device_id,
                            &rel.to_string_lossy(),
                        )
                        .await;
                    }
                }
            }
            Err(e) => failed.push(SourceCleanupError {
                source_path: f.source_path.clone(),
                error: e.to_string(),
            }),
        }
    }

    if deleted > 0 {
        let _ = crate::db::cleanup_orphaned_files(&state.pool).await;
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
pub async fn cancel_import(state: State<'_, AppState>) -> Result<(), AppError> {
    let guard = state.import_cancel_token.lock().await;
    if let Some(token) = guard.as_ref() {
        token.cancel();
    }
    Ok(())
}
