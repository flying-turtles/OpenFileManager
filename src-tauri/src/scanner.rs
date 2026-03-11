use std::path::PathBuf;
use std::time::SystemTime;

use futures::stream::{self, StreamExt};
use tauri::ipc::Channel;
use tokio_util::sync::CancellationToken;
use walkdir::WalkDir;

use crate::db::{self, DbPool};
use crate::devices::{detect_volumes, device_for_path};
use crate::error::AppError;
use crate::hasher;
use crate::models::ScanEvent;

const QUICK_HASH_THRESHOLD: u64 = 2 * 1024 * 1024 * 1024; // 2 GB

struct FileWorkItem {
    path: PathBuf,
    relative_path: String,
    file_name: String,
    extension: String,
    file_size: i64,
    modified_at: Option<String>,
    is_new: bool,
}

struct HashResult {
    item: FileWorkItem,
    hash: String,
}

pub async fn run_scan(
    pool: DbPool,
    target: PathBuf,
    mode: String, // "quick" or "full"
    channel: Channel<ScanEvent>,
    cancel_token: CancellationToken,
) -> Result<(), AppError> {
    // Determine which device this path belongs to
    let volumes = detect_volumes();
    let target_str = target.to_string_lossy().to_string();
    let (device_id, mount_point) = device_for_path(&volumes, &target_str)
        .ok_or_else(|| AppError::General(format!("No device found for path: {}", target_str)))?;

    // Enumerate files first
    let files: Vec<PathBuf> = WalkDir::new(&target)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .collect();

    let total = files.len() as u64;
    let _ = channel.send(ScanEvent::Started { total_files: total });

    let is_quick = mode == "quick";

    // Batch-fetch existing locations for this scan prefix
    let scan_prefix = target
        .strip_prefix(&mount_point)
        .unwrap_or(&target)
        .to_string_lossy()
        .to_string();
    let existing_locations = db::get_locations_by_prefix(&pool, &device_id, &scan_prefix).await?;

    // === Phase 1: classify files ===
    let mut to_hash: Vec<FileWorkItem> = Vec::new();
    let mut seen_paths: Vec<String> = Vec::with_capacity(files.len());
    let mut scanned: u64 = 0;
    let mut added: u64 = 0;

    for file_path in &files {
        if cancel_token.is_cancelled() {
            let _ = channel.send(ScanEvent::Cancelled);
            return Ok(());
        }

        scanned += 1;
        if scanned % 50 == 0 || scanned == total {
            let _ = channel.send(ScanEvent::Progress {
                scanned,
                total,
            });
        }

        let metadata = match std::fs::metadata(file_path) {
            Ok(m) => m,
            Err(e) => {
                let _ = channel.send(ScanEvent::Error {
                    message: format!("{}: {}", file_path.display(), e),
                });
                continue;
            }
        };

        let file_size = metadata.len() as i64;
        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .and_then(|d| {
                chrono::DateTime::from_timestamp(d.as_secs() as i64, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            });

        let relative_path = file_path
            .strip_prefix(&mount_point)
            .unwrap_or(file_path)
            .to_string_lossy()
            .to_string();
        let file_name = file_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let extension = file_path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();

        seen_paths.push(relative_path.clone());

        let existing = existing_locations.get(&relative_path);
        let is_new = existing.is_none();

        // Quick mode: skip if size+mtime match existing record
        if is_quick {
            if let Some(ex) = existing {
                let size_matches = ex.file_size == file_size;
                let mtime_matches = ex.modified_at.as_deref() == modified_at.as_deref();
                if size_matches && mtime_matches {
                    continue;
                }
            }
        }

        // Determine if we should hash or defer
        let should_hash = if is_quick {
            (file_size as u64) <= QUICK_HASH_THRESHOLD
        } else {
            true
        };

        if should_hash {
            to_hash.push(FileWorkItem {
                path: file_path.clone(),
                relative_path,
                file_name,
                extension,
                file_size,
                modified_at,
                is_new,
            });
        } else {
            // Deferred: store with a placeholder hash based on metadata
            let placeholder = format!(
                "deferred:{}:{}",
                file_size,
                modified_at.as_deref().unwrap_or("")
            );
            db::upsert_file(&pool, &placeholder, file_size, &file_name, &extension).await?;
            db::upsert_location(
                &pool,
                &placeholder,
                &device_id,
                &relative_path,
                &file_name,
                file_size,
                modified_at.as_deref(),
                "deferred",
            )
            .await?;
            if is_new {
                added += 1;
            }
        }
    }

    // === Phase 2: parallel hashing ===
    let parallelism = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    let mut hashed: u64 = 0;

    let cancel_token_stream = cancel_token.clone();
    let mut hash_stream = stream::iter(to_hash)
        .map(|item| {
            let token = cancel_token_stream.clone();
            tokio::task::spawn_blocking(move || {
                if token.is_cancelled() {
                    return Err(AppError::General("cancelled".into()));
                }
                let hash = hasher::hash_file_sync(&item.path)?;
                Ok(HashResult { item, hash })
            })
        })
        .buffer_unordered(parallelism);

    while let Some(result) = hash_stream.next().await {
        if cancel_token.is_cancelled() {
            let _ = channel.send(ScanEvent::Cancelled);
            return Ok(());
        }

        match result {
            Ok(Ok(HashResult { item, hash })) => {
                db::upsert_file(&pool, &hash, item.file_size, &item.file_name, &item.extension)
                    .await?;
                db::upsert_location(
                    &pool,
                    &hash,
                    &device_id,
                    &item.relative_path,
                    &item.file_name,
                    item.file_size,
                    item.modified_at.as_deref(),
                    &mode,
                )
                .await?;
                hashed += 1;
                if item.is_new {
                    added += 1;
                }
                let _ = channel.send(ScanEvent::FileHashed {
                    path: item.relative_path,
                    hash,
                });
            }
            Ok(Err(e)) => {
                let msg = e.to_string();
                if msg != "cancelled" {
                    let _ = channel.send(ScanEvent::Error { message: msg });
                }
            }
            Err(e) => {
                let _ = channel.send(ScanEvent::Error {
                    message: format!("task join error: {}", e),
                });
            }
        }
    }

    // === Phase 3: cleanup stale locations ===
    let removed =
        db::remove_stale_locations(&pool, &device_id, &scan_prefix, &seen_paths).await?;
    if removed > 0 {
        db::cleanup_orphaned_files(&pool).await?;
    }

    let _ = channel.send(ScanEvent::Finished {
        scanned,
        hashed,
        added,
        removed,
    });
    Ok(())
}
