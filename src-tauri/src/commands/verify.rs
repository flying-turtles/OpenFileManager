use std::path::Path;
use std::time::SystemTime;

use futures::stream::{self, StreamExt};
use tauri::ipc::Channel;
use tauri::State;
use tokio_util::sync::CancellationToken;

use super::AppState;
use crate::db;
use crate::error::AppError;
use crate::hasher;
use crate::models::VerifyEvent;

fn format_mtime(time: SystemTime) -> Option<String> {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .and_then(|d| {
            chrono::DateTime::from_timestamp(d.as_secs() as i64, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        })
}

enum Outcome {
    Missing,
    Baselined(String),
    Verified,
    /// Size or mtime changed since indexing — legitimately edited, re-baseline
    Modified(String),
    Corrupted,
    Failed(String),
}

struct VerifyItem {
    location_id: i64,
    file_path: String,
    file_name: String,
    outcome: Outcome,
}

/// Verify every indexed file on a device by fully re-hashing it.
/// First verify stores a full-hash baseline; later verifies compare against
/// it. A content change without a size/mtime change is flagged as corruption
/// (bit rot) — the stored baseline is kept so re-runs keep flagging it.
#[tauri::command]
pub async fn verify_device(
    state: State<'_, AppState>,
    device_id: String,
    on_event: Channel<VerifyEvent>,
) -> Result<(), AppError> {
    let token = CancellationToken::new();
    {
        let mut guard = state.verify_cancel_token.lock().await;
        if guard.is_some() {
            return Err(AppError::General("A verification is already running".into()));
        }
        *guard = Some(token.clone());
    }

    let result = run_verify(&state, &device_id, &on_event, &token).await;

    *state.verify_cancel_token.lock().await = None;

    if let Err(e) = &result {
        let _ = on_event.send(VerifyEvent::Error {
            message: e.to_string(),
        });
    }
    result
}

async fn run_verify(
    state: &State<'_, AppState>,
    device_id: &str,
    channel: &Channel<VerifyEvent>,
    token: &CancellationToken,
) -> Result<(), AppError> {
    let device = db::get_device(&state.pool, device_id).await?;
    if !Path::new(&device.mount_point).exists() {
        return Err(AppError::General(format!(
            "\"{}\" is not connected",
            device.label
        )));
    }

    let locations = db::get_locations_for_verify(&state.pool, device_id).await?;
    let total = locations.len() as u64;
    let _ = channel.send(VerifyEvent::Started { total });

    let mount = device.mount_point.clone();
    let mut results = stream::iter(locations)
        .map(move |(id, file_path, file_name, file_size, modified_at, stored_hash)| {
            let full_path = Path::new(&mount).join(&file_path);
            tokio::task::spawn_blocking(move || {
                let outcome = (|| {
                    let metadata = match std::fs::metadata(&full_path) {
                        Ok(m) => m,
                        Err(_) => return Outcome::Missing,
                    };
                    let hash = match hasher::hash_file_full_sync(&full_path) {
                        Ok(h) => h,
                        Err(e) => return Outcome::Failed(e.to_string()),
                    };
                    match stored_hash {
                        None => Outcome::Baselined(hash),
                        Some(stored) if stored == hash => Outcome::Verified,
                        Some(_) => {
                            let size_changed = metadata.len() as i64 != file_size;
                            let mtime_changed = metadata
                                .modified()
                                .ok()
                                .and_then(format_mtime)
                                .as_deref()
                                != modified_at.as_deref();
                            if size_changed || mtime_changed {
                                Outcome::Modified(hash)
                            } else {
                                Outcome::Corrupted
                            }
                        }
                    }
                })();
                VerifyItem {
                    location_id: id,
                    file_path,
                    file_name,
                    outcome,
                }
            })
        })
        .buffer_unordered(3);

    let mut processed: u64 = 0;
    let mut verified: u64 = 0;
    let mut baselined: u64 = 0;
    let mut modified: u64 = 0;
    let mut corrupted: u64 = 0;
    let mut missing: u64 = 0;

    loop {
        let next = tokio::select! {
            _ = token.cancelled() => {
                let _ = channel.send(VerifyEvent::Cancelled);
                return Ok(());
            }
            next = results.next() => next,
        };
        let Some(joined) = next else { break };
        let item = joined.map_err(|e| AppError::General(format!("task join error: {}", e)))?;

        match item.outcome {
            Outcome::Missing => missing += 1,
            Outcome::Baselined(hash) => {
                db::set_full_hash(&state.pool, item.location_id, &hash).await?;
                baselined += 1;
            }
            Outcome::Verified => {
                db::touch_full_hash_verified(&state.pool, item.location_id).await?;
                verified += 1;
            }
            Outcome::Modified(hash) => {
                db::set_full_hash(&state.pool, item.location_id, &hash).await?;
                modified += 1;
            }
            Outcome::Corrupted => {
                corrupted += 1;
                let _ = channel.send(VerifyEvent::Corrupted {
                    location_id: item.location_id,
                    file_path: item.file_path.clone(),
                    file_name: item.file_name.clone(),
                });
            }
            Outcome::Failed(msg) => {
                let _ = channel.send(VerifyEvent::Error {
                    message: format!("{}: {}", item.file_path, msg),
                });
            }
        }

        processed += 1;
        if processed % 5 == 0 || processed == total {
            let _ = channel.send(VerifyEvent::Progress {
                processed,
                total,
                current_file: item.file_path,
            });
        }
    }

    let _ = channel.send(VerifyEvent::Finished {
        verified,
        baselined,
        modified,
        corrupted,
        missing,
    });
    Ok(())
}

#[tauri::command]
pub async fn cancel_verify(state: State<'_, AppState>) -> Result<(), AppError> {
    if let Some(token) = state.verify_cancel_token.lock().await.as_ref() {
        token.cancel();
    }
    Ok(())
}
