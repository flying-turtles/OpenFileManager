use tauri::ipc::Channel;
use tauri::State;

use super::AppState;
use crate::backup;
use crate::db;
use crate::error::AppError;
use crate::models::{BackupEvent, BackupSettings};
use crate::network::{keychain_delete, keychain_load, keychain_store};

fn has_stored_password(username: &str) -> bool {
    keychain_load(backup::KEYCHAIN_ID, username).is_ok()
}

#[tauri::command]
pub async fn get_backup_settings(
    state: State<'_, AppState>,
) -> Result<Option<BackupSettings>, AppError> {
    let row = db::get_backup_settings(&state.pool).await?;
    Ok(row.map(|(host, port, database, username, last_backup_at)| BackupSettings {
        has_password: has_stored_password(&username),
        host,
        port,
        database,
        username,
        last_backup_at,
    }))
}

#[tauri::command]
pub async fn save_backup_settings(
    state: State<'_, AppState>,
    host: String,
    port: i64,
    database: String,
    username: String,
    password: String,
) -> Result<BackupSettings, AppError> {
    if host.trim().is_empty() || database.trim().is_empty() || username.trim().is_empty() {
        return Err(AppError::General(
            "Host, database and user are required".into(),
        ));
    }
    if !(1..=65535).contains(&port) {
        return Err(AppError::General("Port must be between 1 and 65535".into()));
    }

    // If the username changed, drop the old keychain entry
    if let Some((_, _, _, old_user, _)) = db::get_backup_settings(&state.pool).await? {
        if old_user != username {
            let _ = keychain_delete(backup::KEYCHAIN_ID, &old_user);
        }
    }

    db::save_backup_settings(&state.pool, host.trim(), port, database.trim(), username.trim())
        .await?;

    // Empty password = keep the existing keychain entry
    if !password.is_empty() {
        keychain_store(backup::KEYCHAIN_ID, username.trim(), &password)?;
    }

    let row = db::get_backup_settings(&state.pool).await?;
    let (host, port, database, username, last_backup_at) =
        row.ok_or_else(|| AppError::General("Failed to persist backup settings".into()))?;
    Ok(BackupSettings {
        has_password: has_stored_password(&username),
        host,
        port,
        database,
        username,
        last_backup_at,
    })
}

async fn connect_from_settings(state: &AppState) -> Result<sqlx::PgPool, AppError> {
    let (host, port, database, username, _) = db::get_backup_settings(&state.pool)
        .await?
        .ok_or_else(|| AppError::General("No backup connection configured".into()))?;
    let password = keychain_load(backup::KEYCHAIN_ID, &username)
        .map_err(|_| AppError::General("No password stored — save the connection details first".into()))?;
    backup::connect(&host, port as u16, &database, &username, &password).await
}

#[tauri::command]
pub async fn test_backup_connection(state: State<'_, AppState>) -> Result<(), AppError> {
    let pool = connect_from_settings(&state).await?;
    sqlx::query("SELECT 1")
        .execute(&pool)
        .await
        .map_err(|e| AppError::General(format!("PostgreSQL error: {}", e)))?;
    pool.close().await;
    Ok(())
}

#[tauri::command]
pub async fn run_database_backup(
    state: State<'_, AppState>,
    on_event: Channel<BackupEvent>,
) -> Result<(), AppError> {
    let pg = connect_from_settings(&state).await?;
    let result = backup::run_backup(&state.pool, &pg, &on_event).await;
    pg.close().await;

    match result {
        Ok(total_rows) => {
            let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
            db::set_last_backup_at(&state.pool, &now).await?;
            let _ = on_event.send(BackupEvent::Finished {
                total_rows,
                finished_at: now,
            });
            Ok(())
        }
        Err(e) => {
            let _ = on_event.send(BackupEvent::Error {
                message: e.to_string(),
            });
            Err(e)
        }
    }
}
