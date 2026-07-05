use std::collections::{HashMap, HashSet};
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

fn format_mtime(time: SystemTime) -> Option<String> {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .and_then(|d| {
            chrono::DateTime::from_timestamp(d.as_secs() as i64, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        })
}

fn scan_walker(target: &PathBuf) -> ignore::Walk {
    WalkBuilder::new(target)
        .hidden(true)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .add_custom_ignore_filename(".openfileignore")
        .build()
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

    let scan_prefix = target
        .strip_prefix(&mount_point)
        .unwrap_or(&target)
        .to_string_lossy()
        .to_string();

    // === Phase 0: load caches ===
    let dir_cache = db::get_dir_cache(&pool, &device_id, &scan_prefix).await?;
    let existing_locations = db::get_locations_by_prefix(&pool, &device_id, &scan_prefix).await?;

    // Estimate total: use dir cache if available, otherwise quick count pass
    let mut total: u64 = if !dir_cache.is_empty() {
        dir_cache.values().map(|e| e.file_count as u64).sum()
    } else {
        scan_walker(&target)
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map_or(false, |ft| ft.is_file()))
            .filter(|e| e.file_name().to_string_lossy() != FILEMANAGER_ID_FILE)
            .count() as u64
    };
    progress.total.store(total, Ordering::Relaxed);
    let _ = channel.send(ScanEvent::Started { total_files: total });

    // === Phase 1: walk with per-directory mtime caching ===
    //
    // For each directory: compare mtime against cache. If unchanged, skip
    // per-file stat/classification for its direct children. Subdirectories
    // are still checked individually since a dir mtime only reflects
    // direct-children changes, not nested changes.
    let mut unchanged_dirs: HashSet<String> = HashSet::new();
    let mut dir_file_counts: HashMap<String, (String, i64)> = HashMap::new();
    let mut seen_dirs: Vec<String> = Vec::new();

    let mut to_hash: Vec<FileWorkItem> = Vec::new();
    let mut seen_paths: Vec<String> = Vec::with_capacity(total as usize);
    let mut scanned: u64 = 0;
    let mut added: u64 = 0;

    for entry_result in scan_walker(&target) {
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

        let entry = match entry_result {
            Ok(e) => e,
            Err(e) => {
                let _ = channel.send(ScanEvent::Error {
                    message: format!("walk error: {}", e),
                });
                continue;
            }
        };

        if entry.file_type().map_or(false, |ft| ft.is_dir()) {
            let dir_path = entry.path();
            let rel_dir = dir_path
                .strip_prefix(&mount_point)
                .unwrap_or(dir_path)
                .to_string_lossy()
                .to_string();
            seen_dirs.push(rel_dir.clone());

            let dir_mtime_str = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(format_mtime)
                .unwrap_or_default();

            if let Some(cached) = dir_cache.get(&rel_dir) {
                if cached.dir_mtime == dir_mtime_str {
                    unchanged_dirs.insert(rel_dir.clone());
                    dir_file_counts.insert(rel_dir, (dir_mtime_str, cached.file_count));
                    continue;
                }
            }
            // New or changed dir — will process its files
            dir_file_counts.insert(rel_dir, (dir_mtime_str, 0));
            continue;
        }

        if !entry.file_type().map_or(false, |ft| ft.is_file()) {
            continue;
        }
        if entry.file_name().to_string_lossy() == FILEMANAGER_ID_FILE {
            continue;
        }

        // --- File entry ---
        let file_path = entry.path();
        let relative_path = file_path
            .strip_prefix(&mount_point)
            .unwrap_or(file_path)
            .to_string_lossy()
            .to_string();

        scanned += 1;
        progress.scanned.store(scanned, Ordering::Relaxed);
        if scanned % 50 == 0 || scanned == total {
            let _ = channel.send(ScanEvent::Progress { scanned, total });
        }

        seen_paths.push(relative_path.clone());

        // Skip per-file work when the parent dir is unchanged since last scan
        let parent_rel = file_path
            .parent()
            .and_then(|p| p.strip_prefix(&mount_point).ok())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        if unchanged_dirs.contains(&parent_rel) {
            continue;
        }

        if let Some((_, count)) = dir_file_counts.get_mut(&parent_rel) {
            *count += 1;
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
        let modified_at = metadata.modified().ok().and_then(format_mtime);

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
            path: file_path.to_path_buf(),
            relative_path,
            file_name,
            extension,
            file_size,
            modified_at,
            is_new,
        });
    }

    // Correct total to actual count (estimate may have been slightly off)
    total = scanned;
    progress.total.store(total, Ordering::Relaxed);
    let _ = channel.send(ScanEvent::Progress { scanned, total });

    // === Phase 2: hash files ===
    let to_hash_total = to_hash.len() as u64;
    let skipped = total.saturating_sub(to_hash_total);
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

    // === Phase 3: cleanup stale locations + update dir cache ===
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

    // Persist dir cache updates
    let cache_entries: Vec<(String, String, i64)> = dir_file_counts
        .into_iter()
        .map(|(path, (mtime, count))| (path, mtime, count))
        .collect();
    if !cache_entries.is_empty() {
        db::upsert_dir_cache_batch(&pool, &device_id, &cache_entries).await?;
    }
    if target.is_dir() {
        db::remove_stale_dir_cache(&pool, &device_id, &scan_prefix, &seen_dirs).await?;
    }

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
