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

    let volumes = crate::devices::detect_volumes();
    let mut targets = Vec::new();
    for id in target_device_ids {
        let vol = volumes
            .iter()
            .find(|v| v.id == id)
            .ok_or_else(|| AppError::General(format!("Device {} not connected", id)))?;
        targets.push((vol.id.clone(), vol.mount_point.clone(), vol.label.clone()));
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
pub async fn cancel_import(state: State<'_, AppState>) -> Result<(), AppError> {
    let guard = state.import_cancel_token.lock().await;
    if let Some(token) = guard.as_ref() {
        token.cancel();
    }
    Ok(())
}
