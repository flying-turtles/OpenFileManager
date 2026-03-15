use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

use futures::stream::{self, StreamExt};
use tauri::ipc::Channel;
use tokio_util::sync::CancellationToken;
use ignore::WalkBuilder;

use crate::db::{self, DbPool};
use crate::devices::{detect_volumes, device_for_path, FILEMANAGER_ID_FILE};
use crate::error::AppError;
use crate::hasher;
use crate::models::ScanEvent;

const PARTIAL_HASH_BYTES: u64 = 4 * 1024 * 1024; // 4 MB

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

/// Shared counters so pause can snapshot progress
pub struct ScanProgress {
    pub scanned: AtomicU64,
    pub hashed: AtomicU64,
    pub added: AtomicU64,
    pub total: AtomicU64,
    pub pausing: AtomicBool,
}

impl ScanProgress {
    pub fn new() -> Self {
        Self {
            scanned: AtomicU64::new(0),
            hashed: AtomicU64::new(0),
            added: AtomicU64::new(0),
            total: AtomicU64::new(0),
            pausing: AtomicBool::new(false),
        }
    }
}

pub async fn run_scan(
    pool: DbPool,
    target: PathBuf,
    channel: Channel<ScanEvent>,
    cancel_token: CancellationToken,
    progress: Arc<ScanProgress>,
) -> Result<(), AppError> {
    // Determine which device this path belongs to
    let volumes = detect_volumes();
    let target_str = target.to_string_lossy().to_string();
    let (device_id, mount_point) = device_for_path(&volumes, &target_str)
        .ok_or_else(|| AppError::General(format!("No device found for path: {}", target_str)))?;

    // Enumerate files first
    let files: Vec<PathBuf> = WalkBuilder::new(&target)
        .hidden(true)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .add_custom_ignore_filename(".openfileignore")
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map_or(false, |ft| ft.is_file()))
        .filter(|e| e.file_name().to_string_lossy() != FILEMANAGER_ID_FILE)
        .map(|e| e.into_path())
        .collect();

    let total = files.len() as u64;
    progress.total.store(total, Ordering::Relaxed);
    let _ = channel.send(ScanEvent::Started { total_files: total });

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
            if progress.pausing.load(Ordering::Relaxed) {
                let s = progress.scanned.load(Ordering::Relaxed);
                let h = progress.hashed.load(Ordering::Relaxed);
                let a = progress.added.load(Ordering::Relaxed);
                let _ = channel.send(ScanEvent::Paused { scanned: s, hashed: h, added: a, total });
            } else {
                let _ = channel.send(ScanEvent::Cancelled);
            }
            return Ok(());
        }

        scanned += 1;
        progress.scanned.store(scanned, Ordering::Relaxed);
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

        // Skip unchanged files already hashed
        if let Some(ex) = existing {
            let size_matches = ex.file_size == file_size;
            let mtime_matches = ex.modified_at.as_deref() == modified_at.as_deref();
            if size_matches && mtime_matches && !ex.blake3_hash.starts_with("deferred:") {
                continue;
            }
        }

        to_hash.push(FileWorkItem {
            path: file_path.clone(),
            relative_path,
            file_name,
            extension,
            file_size,
            modified_at,
            is_new,
        });
    }

    // === Phase 2: hash files ===
    let to_hash_total = to_hash.len() as u64;
    let skipped = total - to_hash_total;
    let _ = channel.send(ScanEvent::HashingStarted { to_hash: to_hash_total, skipped });

    let parallelism = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    let mut hashed: u64 = 0;

    let cancel_token_h = cancel_token.clone();
    let mut hash_stream = stream::iter(to_hash)
        .map(move |item| {
            let token = cancel_token_h.clone();
            tokio::task::spawn_blocking(move || {
                if token.is_cancelled() {
                    return Err(AppError::General("cancelled".into()));
                }
                let hash = hasher::hash_file_partial_sync(&item.path, PARTIAL_HASH_BYTES)?;
                Ok(HashResult { item, hash })
            })
        })
        .buffer_unordered(parallelism);

    while let Some(result) = hash_stream.next().await {
        if cancel_token.is_cancelled() {
            if progress.pausing.load(Ordering::Relaxed) {
                let s = progress.scanned.load(Ordering::Relaxed);
                let h = progress.hashed.load(Ordering::Relaxed);
                let a = progress.added.load(Ordering::Relaxed);
                let _ = channel.send(ScanEvent::Paused { scanned: s, hashed: h, added: a, total });
            } else {
                let _ = channel.send(ScanEvent::Cancelled);
            }
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
                    "quick",
                )
                .await?;
                hashed += 1;
                progress.hashed.store(hashed, Ordering::Relaxed);
                if item.is_new {
                    added += 1;
                    progress.added.store(added, Ordering::Relaxed);
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
    // Skip cleanup when scanning a single file (not a directory) — we can't
    // determine what's "stale" from a single-file scan, and the prefix-LIKE
    // could accidentally match unrelated entries.
    let removed = if target.is_dir() {
        let r = db::remove_stale_locations(&pool, &device_id, &scan_prefix, &seen_paths).await?;
        if r > 0 {
            db::cleanup_orphaned_files(&pool).await?;
        }
        r
    } else {
        0
    };

    // Scan finished successfully — remove any pending record
    let _ = db::delete_pending_scan_by_target(&pool, "scan", &target_str).await;

    let _ = channel.send(ScanEvent::Finished {
        scanned,
        hashed,
        added,
        removed,
    });
    Ok(())
}
