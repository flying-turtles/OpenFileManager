use std::collections::HashMap;
use std::path::{Path, PathBuf};

use futures::stream::{self, StreamExt};
use tauri::ipc::Channel;
use tauri::{Manager, State};
use tokio_util::sync::CancellationToken;

use super::AppState;
use crate::db;
use crate::error::AppError;
use crate::models::{SimilarFile, SimilarGroup, SimilarScanEvent};
use crate::similarity;

/// Generate a small Quick Look thumbnail and decode it, for formats the
/// image crate can't read (HEIC, RAW, ...).
async fn dhash_via_quicklook(app: &tauri::AppHandle, path: &Path) -> Result<u64, AppError> {
    let tmp_dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| AppError::General(e.to_string()))?
        .join("phash-tmp");
    std::fs::create_dir_all(&tmp_dir)?;

    let key = blake3::hash(path.to_string_lossy().as_bytes());
    let out_dir = tmp_dir.join(&key.to_hex()[..16]);
    std::fs::create_dir_all(&out_dir)?;

    let output = tokio::process::Command::new("qlmanage")
        .args(["-t", "-s", "128", "-o"])
        .arg(&out_dir)
        .arg(path)
        .output()
        .await?;

    let result = (|| -> Result<u64, AppError> {
        if !output.status.success() {
            return Err(AppError::General(format!(
                "qlmanage failed for {}",
                path.display()
            )));
        }
        let png = std::fs::read_dir(&out_dir)?
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().map_or(false, |e| e == "png"))
            .ok_or_else(|| {
                AppError::General(format!("qlmanage produced no output for {}", path.display()))
            })?;
        similarity::dhash_file(&png)
    })();

    let _ = std::fs::remove_dir_all(&out_dir);
    result
}

async fn compute_dhash(app: &tauri::AppHandle, path: PathBuf) -> Result<u64, AppError> {
    let ext = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    if similarity::DECODABLE_EXTS.contains(&ext.as_str()) {
        tokio::task::spawn_blocking(move || similarity::dhash_file(&path))
            .await
            .map_err(|e| AppError::General(format!("task join error: {}", e)))?
    } else {
        dhash_via_quicklook(app, &path).await
    }
}

#[tauri::command]
pub async fn scan_similar_pictures(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    on_event: Channel<SimilarScanEvent>,
) -> Result<(), AppError> {
    let token = CancellationToken::new();
    {
        let mut guard = state.similar_cancel_token.lock().await;
        if guard.is_some() {
            return Err(AppError::General("A similarity scan is already running".into()));
        }
        *guard = Some(token.clone());
    }

    let result = run_similarity_scan(&app, &state, &on_event, &token).await;

    *state.similar_cancel_token.lock().await = None;

    if let Err(e) = &result {
        let _ = on_event.send(SimilarScanEvent::Error {
            message: e.to_string(),
        });
    }
    result
}

async fn run_similarity_scan(
    app: &tauri::AppHandle,
    state: &State<'_, AppState>,
    channel: &Channel<SimilarScanEvent>,
    token: &CancellationToken,
) -> Result<(), AppError> {
    // Connected devices = mount point currently exists
    let devices = db::get_all_devices(&state.pool).await?;
    let mounts: HashMap<String, String> = devices
        .into_iter()
        .filter(|d| Path::new(&d.mount_point).exists())
        .map(|d| (d.id, d.mount_point))
        .collect();
    let connected_ids: Vec<String> = mounts.keys().cloned().collect();

    let todo =
        db::get_unhashed_images(&state.pool, &connected_ids, &similarity::all_image_exts()).await?;
    let total = todo.len() as u64;
    let _ = channel.send(SimilarScanEvent::Started { total });

    let mut work = Vec::with_capacity(todo.len());
    for (hash, device_id, file_path) in todo {
        if let Some(mount) = mounts.get(&device_id) {
            work.push((hash, Path::new(mount).join(&file_path)));
        }
    }

    let mut results = stream::iter(work)
        .map(|(hash, path)| async move {
            let dhash = compute_dhash(app, path).await.ok();
            (hash, dhash)
        })
        .buffer_unordered(4);

    let mut processed: u64 = 0;
    let mut hashed: u64 = 0;
    let mut failed: u64 = 0;

    while let Some((hash, dhash)) = results.next().await {
        if token.is_cancelled() {
            let _ = channel.send(SimilarScanEvent::Cancelled);
            return Ok(());
        }
        match dhash {
            Some(h) => {
                db::upsert_image_hash(&state.pool, &hash, Some(h as i64)).await?;
                hashed += 1;
            }
            None => {
                // Remember the failure so we don't retry every scan
                db::upsert_image_hash(&state.pool, &hash, None).await?;
                failed += 1;
            }
        }
        processed += 1;
        if processed % 10 == 0 || processed == total {
            let _ = channel.send(SimilarScanEvent::Progress { processed, total });
        }
    }

    let _ = channel.send(SimilarScanEvent::Finished { hashed, failed });
    Ok(())
}

#[tauri::command]
pub async fn cancel_similar_scan(state: State<'_, AppState>) -> Result<(), AppError> {
    if let Some(token) = state.similar_cancel_token.lock().await.as_ref() {
        token.cancel();
    }
    Ok(())
}

#[tauri::command]
pub async fn get_similar_groups(
    state: State<'_, AppState>,
    max_distance: Option<u32>,
) -> Result<Vec<SimilarGroup>, AppError> {
    let rows = db::get_image_hashes(&state.pool).await?;
    let hashes: Vec<u64> = rows.iter().map(|(_, d, _, _)| *d as u64).collect();

    let clusters = similarity::cluster(&hashes, max_distance.unwrap_or(5));

    let mut groups = Vec::with_capacity(clusters.len());
    for cluster in clusters {
        let mut files = Vec::with_capacity(cluster.len());
        for idx in cluster {
            let (blake3_hash, _, representative_name, file_size) = &rows[idx];
            let locations = db::get_file_locations(&state.pool, blake3_hash).await?;
            if locations.is_empty() {
                continue;
            }
            files.push(SimilarFile {
                blake3_hash: blake3_hash.clone(),
                representative_name: representative_name.clone(),
                file_size: *file_size,
                locations,
            });
        }
        if files.len() > 1 {
            // Biggest file first — usually the highest-quality candidate to keep
            files.sort_by(|a, b| b.file_size.cmp(&a.file_size));
            groups.push(SimilarGroup { files });
        }
    }

    // Largest potential savings first
    groups.sort_by_key(|g| {
        let total: i64 = g.files.iter().skip(1).map(|f| f.file_size).sum();
        std::cmp::Reverse(total)
    });
    Ok(groups)
}
