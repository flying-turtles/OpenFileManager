use std::path::PathBuf;
use std::sync::Arc;

use tauri::ipc::Channel;
use tauri::State;
use tokio_util::sync::CancellationToken;

use super::AppState;
use crate::error::AppError;
use crate::models::*;

#[tauri::command]
pub async fn plan_move(
    state: State<'_, AppState>,
    sources: Vec<String>,
    dest: String,
) -> Result<MovePlanSummary, AppError> {
    if sources.is_empty() {
        return Err(AppError::General("No source selected".into()));
    }

    // A stalled network mount would otherwise hang the walk in
    // uninterruptible I/O; fail fast instead.
    for root in sources.iter().chain(std::iter::once(&dest)) {
        if !super::path_online_within(root.clone(), 5).await {
            return Err(AppError::General(format!(
                "{} is not reachable — reconnect it and try again",
                root
            )));
        }
    }

    let source_paths: Vec<PathBuf> = sources.iter().map(PathBuf::from).collect();
    let dest_path = PathBuf::from(&dest);

    let (plan, volumes) = tokio::task::spawn_blocking(move || {
        let volumes = crate::devices::detect_volumes();
        let plan = crate::mover::build_plan(source_paths, dest_path, &volumes);
        (plan, volumes)
    })
    .await
    .map_err(|e| AppError::General(format!("task join error: {}", e)))?;
    let plan = plan?;

    // Best-effort: register the resolved source/dest devices so the move's
    // index writes (file_locations.device_id FK -> storage_devices) succeed
    // even if the app has never scanned this volume before. A failure here
    // must not fail the plan itself.
    for device_id in [&plan.source_device_id, &plan.dest_device_id] {
        if let Some(disk) = volumes.iter().find(|d| &d.id == device_id) {
            let _ = crate::db::upsert_device(&state.pool, disk).await;
        }
    }

    let summary = MovePlanSummary::from(&plan);
    *state.move_plan.lock().await = Some(Arc::new(plan));
    Ok(summary)
}

#[tauri::command]
pub async fn start_move(
    state: State<'_, AppState>,
    permanent: Option<bool>,
    on_event: Channel<MoveEvent>,
) -> Result<(), AppError> {
    let plan = {
        let guard = state.move_plan.lock().await;
        guard
            .clone()
            .ok_or_else(|| AppError::General("No move planned. Plan first.".into()))?
    };

    let pool = state.pool.clone();
    let cancel_token = CancellationToken::new();
    {
        let mut guard = state.move_cancel_token.lock().await;
        *guard = Some(cancel_token.clone());
    }

    let permanent = permanent.unwrap_or(false);
    tokio::spawn(async move {
        if let Err(e) =
            crate::mover::run_move(pool, plan, permanent, on_event.clone(), cancel_token).await
        {
            let _ = on_event.send(MoveEvent::FileFailed(MoveError {
                source_path: String::new(),
                file_name: String::new(),
                error: e.to_string(),
            }));
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn cancel_move(state: State<'_, AppState>) -> Result<(), AppError> {
    let guard = state.move_cancel_token.lock().await;
    if let Some(token) = guard.as_ref() {
        token.cancel();
    }
    Ok(())
}
