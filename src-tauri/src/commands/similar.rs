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

    // Timeout + kill_on_drop so a hung Quick Look can never stall the scan
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        tokio::process::Command::new("qlmanage")
            .args(["-t", "-s", "128", "-o"])
            .arg(&out_dir)
            .arg(path)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| {
        let _ = std::fs::remove_dir_all(&out_dir);
        AppError::General(format!("qlmanage timed out for {}", path.display()))
    })??;

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
    device_id: Option<String>,
    folder: Option<String>,
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

    let result = run_similarity_scan(&app, &state, device_id, folder, &on_event, &token).await;

    *state.similar_cancel_token.lock().await = None;

    if let Err(e) = &result {
        let _ = on_event.send(SimilarScanEvent::Error {
            message: e.to_string(),
        });
    }
    result
}

/// Map an absolute folder path to (device_id, relative prefix) using the
/// longest matching connected mount point. Empty prefix = whole device.
fn resolve_folder(
    folder: &str,
    mounts: &HashMap<String, String>,
) -> Result<(String, Option<String>), AppError> {
    let folder_path = Path::new(folder);
    let mut best: Option<(&String, &String)> = None;
    for (id, mp) in mounts {
        if folder_path.starts_with(mp) && best.map_or(true, |(_, bmp)| mp.len() > bmp.len()) {
            best = Some((id, mp));
        }
    }
    let (id, mp) = best.ok_or_else(|| {
        AppError::General("Folder is not on any connected indexed drive".into())
    })?;
    let rel = folder_path
        .strip_prefix(mp)
        .unwrap_or_else(|_| Path::new(""))
        .to_string_lossy()
        .trim_matches('/')
        .to_string();
    Ok((id.clone(), if rel.is_empty() { None } else { Some(rel) }))
}

async fn run_similarity_scan(
    app: &tauri::AppHandle,
    state: &State<'_, AppState>,
    device_id: Option<String>,
    folder: Option<String>,
    channel: &Channel<SimilarScanEvent>,
    token: &CancellationToken,
) -> Result<(), AppError> {
    // Connected devices = mount point currently exists; optionally one device only
    let devices = db::get_all_devices(&state.pool).await?;
    let mut mounts: HashMap<String, String> = devices
        .into_iter()
        .filter(|d| Path::new(&d.mount_point).exists())
        .filter(|d| device_id.as_ref().map_or(true, |id| &d.id == id))
        .map(|d| (d.id, d.mount_point))
        .collect();

    // A folder narrows the scan to one device + path prefix (subfolders included)
    let mut path_prefix: Option<String> = None;
    if let Some(folder) = &folder {
        let (dev, prefix) = resolve_folder(folder, &mounts)?;
        mounts.retain(|id, _| id == &dev);
        path_prefix = prefix;
    }
    let connected_ids: Vec<String> = mounts.keys().cloned().collect();

    let todo = db::get_unhashed_images(
        &state.pool,
        &connected_ids,
        &similarity::all_image_exts(),
        path_prefix.as_deref(),
    )
    .await?;
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

    // select! so cancel fires immediately, even while decodes are in flight —
    // dropping the stream kills pending Quick Look processes (kill_on_drop)
    loop {
        let item = tokio::select! {
            _ = token.cancelled() => {
                let _ = channel.send(SimilarScanEvent::Cancelled);
                return Ok(());
            }
            item = results.next() => item,
        };
        let Some((hash, dhash)) = item else { break };
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
    device_id: Option<String>,
    folder: Option<String>,
) -> Result<Vec<SimilarGroup>, AppError> {
    // A folder narrows the scope to one device + path prefix
    let mut scope_device = device_id;
    let mut scope_prefix: Option<String> = None;
    if let Some(folder) = &folder {
        let devices = db::get_all_devices(&state.pool).await?;
        let mounts: HashMap<String, String> = devices
            .into_iter()
            .filter(|d| Path::new(&d.mount_point).exists())
            .map(|d| (d.id, d.mount_point))
            .collect();
        let (dev, prefix) = resolve_folder(folder, &mounts)?;
        scope_device = Some(dev);
        scope_prefix = prefix;
    }

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
        // With a device/folder filter, only show groups that touch that
        // scope; the whole group stays visible for context.
        let in_scope = scope_device.as_ref().map_or(true, |id| {
            files.iter().any(|f| {
                f.locations.iter().any(|l| {
                    &l.device_id == id
                        && scope_prefix.as_ref().map_or(true, |p| {
                            l.file_path == *p || l.file_path.starts_with(&format!("{}/", p))
                        })
                })
            })
        });
        if files.len() > 1 && in_scope {
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
