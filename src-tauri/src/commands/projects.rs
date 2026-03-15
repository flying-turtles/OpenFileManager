use tauri::State;

use super::AppState;
use crate::db;
use crate::error::AppError;
use crate::models::*;

#[tauri::command]
pub async fn create_project(
    state: State<'_, AppState>,
    title: String,
    description: String,
    start_date: String,
    end_date: String,
) -> Result<Project, AppError> {
    db::create_project(&state.pool, &title, &description, &start_date, &end_date).await
}

#[tauri::command]
pub async fn get_projects(state: State<'_, AppState>) -> Result<Vec<Project>, AppError> {
    db::get_all_projects(&state.pool).await
}

#[tauri::command]
pub async fn get_project_detail(
    state: State<'_, AppState>,
    id: i64,
) -> Result<ProjectDetail, AppError> {
    let project = db::get_project(&state.pool, id).await?;
    let stats = db::get_project_stats(&state.pool, &project.start_date, &project.end_date).await?;
    let files = db::get_project_files(&state.pool, &project.start_date, &project.end_date).await?;
    Ok(ProjectDetail { project, stats, files })
}

#[tauri::command]
pub async fn update_project(
    state: State<'_, AppState>,
    id: i64,
    title: String,
    description: String,
    start_date: String,
    end_date: String,
) -> Result<Project, AppError> {
    db::update_project(&state.pool, id, &title, &description, &start_date, &end_date).await
}

#[tauri::command]
pub async fn delete_project(state: State<'_, AppState>, id: i64) -> Result<(), AppError> {
    db::delete_project(&state.pool, id).await
}
