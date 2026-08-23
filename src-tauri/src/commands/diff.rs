use std::collections::HashMap;

use tauri::State;

use super::{path_online, AppState};
use crate::db;
use crate::error::AppError;
use crate::models::*;

#[tauri::command]
pub async fn get_project_diff_devices(
    state: State<'_, AppState>,
    project_id: i64,
) -> Result<Vec<DiffDeviceOption>, AppError> {
    let project = db::get_project(&state.pool, project_id).await?;
    let mut options =
        db::get_project_diff_devices(&state.pool, &project.start_date, &project.end_date).await?;

    let devices = db::get_all_devices(&state.pool).await?;
    let mounts: HashMap<String, String> = devices
        .into_iter()
        .map(|d| (d.id, d.mount_point))
        .collect();

    for opt in &mut options {
        opt.is_connected = match mounts.get(&opt.device_id) {
            Some(mount) => path_online(mount).await,
            None => false,
        };
    }

    Ok(options)
}
