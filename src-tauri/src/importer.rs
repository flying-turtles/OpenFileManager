use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use tauri::ipc::Channel;
use tokio_util::sync::CancellationToken;
use ignore::WalkBuilder;

use crate::db::{self, DbPool};
use crate::devices;
use crate::error::AppError;
use crate::hasher;
use crate::models::*;

/// Copies a file in chunks, checking the cancellation token between chunks.
/// Returns Ok(true) if complete, Ok(false) if cancelled (partial file removed).
pub async fn copy_file_cancellable(
    src: &str,
    dest: &Path,
    cancel_token: &CancellationToken,
) -> Result<bool, std::io::Error> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut reader = tokio::fs::File::open(src).await?;
    let mut writer = tokio::fs::File::create(dest).await?;
    let mut buf = vec![0u8; 256 * 1024]; // 256KB chunks

    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                drop(writer);
                let _ = tokio::fs::remove_file(dest).await;
                return Ok(false);
            }
            result = reader.read(&mut buf) => {
                let n = result?;
                if n == 0 { break; }
                writer.write_all(&buf[..n]).await?;
            }
        }
    }
    writer.flush().await?;
    Ok(true)
}

pub async fn analyze_sd_card(
    pool: DbPool,
    sd_mount: PathBuf,
    channel: Channel<ImportEvent>,
    cancel_token: CancellationToken,
) -> Result<ImportAnalysis, AppError> {
    let volumes = devices::detect_volumes();
    let sd_mount_str = sd_mount.to_string_lossy().to_string();
    let (device_id, _) = devices::device_for_path(&volumes, &sd_mount_str)
        .ok_or_else(|| AppError::General(format!("No device for path: {}", sd_mount_str)))?;

    let sd_label = volumes
        .iter()
        .find(|v| v.id == device_id)
        .map(|v| v.label.clone())
        .unwrap_or_else(|| "SD Card".to_string());

    let files: Vec<PathBuf> = WalkBuilder::new(&sd_mount)
        .hidden(true)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .add_custom_ignore_filename(".openfileignore")
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map_or(false, |ft| ft.is_file()))
        .map(|e| e.into_path())
        .collect();

    let total = files.len() as u64;
    let _ = channel.send(ImportEvent::AnalysisStarted { total_files: total });

    let mut import_files = Vec::new();
    let mut total_bytes: i64 = 0;

    for (i, file_path) in files.iter().enumerate() {
        if cancel_token.is_cancelled() {
            let _ = channel.send(ImportEvent::Cancelled);
            return Err(AppError::General("Cancelled".into()));
        }

        let metadata = match std::fs::metadata(file_path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let file_size = metadata.len() as i64;
        total_bytes += file_size;

        let created_date = metadata
            .created()
            .or_else(|_| metadata.modified())
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .and_then(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
            .map(|dt| dt.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .and_then(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string());

        let relative_path = file_path
            .strip_prefix(&sd_mount)
            .unwrap_or(file_path)
            .to_string_lossy()
            .to_string();

        let file_name = file_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let blake3_hash = match hasher::hash_file_partial(file_path, 4 * 1024 * 1024).await {
            Ok(h) => h,
            Err(e) => {
                let _ = channel.send(ImportEvent::Error {
                    message: format!("{}: {}", file_path.display(), e),
                });
                continue;
            }
        };

        import_files.push(ImportFile {
            source_path: file_path.to_string_lossy().to_string(),
            relative_path,
            file_name,
            blake3_hash,
            file_size,
            created_date,
            modified_at,
            existing_locations: Vec::new(),
        });

        if (i + 1) % 10 == 0 || i + 1 == files.len() {
            let _ = channel.send(ImportEvent::AnalysisProgress {
                processed: (i + 1) as u64,
                total,
            });
        }
    }

    // Batch lookup existing locations
    let hashes: Vec<String> = import_files.iter().map(|f| f.blake3_hash.clone()).collect();
    let locations_map = db::get_locations_for_hashes(&pool, &hashes).await?;

    let mut new_file_count: u64 = 0;
    let mut existing_file_count: u64 = 0;
    for file in &mut import_files {
        if let Some(locs) = locations_map.get(&file.blake3_hash) {
            file.existing_locations = locs.clone();
            existing_file_count += 1;
        } else {
            new_file_count += 1;
        }
    }

    Ok(ImportAnalysis {
        sd_device_id: device_id,
        sd_label,
        files: import_files,
        total_bytes,
        new_file_count,
        existing_file_count,
    })
}

pub async fn run_import(
    pool: DbPool,
    analysis: Arc<ImportAnalysis>,
    target_devices: Vec<(String, String, String)>,
    channel: Channel<ImportEvent>,
    cancel_token: CancellationToken,
) -> Result<(), AppError> {
    let total_files = analysis.files.len() as u64;
    let _ = channel.send(ImportEvent::CopyStarted {
        total_files,
        device_count: target_devices.len() as u64,
    });

    let mut handles = Vec::new();

    for (device_id, mount_point, device_label) in target_devices {
        let pool = pool.clone();
        let analysis = analysis.clone();
        let channel = channel.clone();
        let cancel_token = cancel_token.clone();

        let device_total_bytes: i64 = analysis
            .files
            .iter()
            .filter(|f| !f.existing_locations.iter().any(|l| l.device_id == device_id))
            .map(|f| f.file_size)
            .sum();

        let device_total_files: u64 = analysis
            .files
            .iter()
            .filter(|f| !f.existing_locations.iter().any(|l| l.device_id == device_id))
            .count() as u64;

        let handle = tokio::spawn(async move {
            let mut bytes_copied: i64 = 0;
            let mut files_copied: u64 = 0;

            let mut consecutive_errors: u32 = 0;
            const MAX_CONSECUTIVE_ERRORS: u32 = 3;

            // Send initial progress so UI shows immediately
            let _ = channel.send(ImportEvent::CopyProgress(DeviceCopyProgress {
                device_id: device_id.clone(),
                device_label: device_label.clone(),
                bytes_copied: 0,
                total_bytes: device_total_bytes,
                files_copied: 0,
                total_files: device_total_files,
                current_file: "Preparing...".to_string(),
            }));

            for file in &analysis.files {
                if cancel_token.is_cancelled() {
                    let _ = channel.send(ImportEvent::Cancelled);
                    return;
                }

                if file.existing_locations.iter().any(|l| l.device_id == device_id) {
                    continue;
                }

                // Show which file is being copied before the copy starts
                let _ = channel.send(ImportEvent::CopyProgress(DeviceCopyProgress {
                    device_id: device_id.clone(),
                    device_label: device_label.clone(),
                    bytes_copied,
                    total_bytes: device_total_bytes,
                    files_copied,
                    total_files: device_total_files,
                    current_file: format!("Copying {}...", file.file_name),
                }));

                let dest_dir = PathBuf::from(&mount_point).join(&file.created_date);
                if let Err(e) = tokio::fs::create_dir_all(&dest_dir).await {
                    consecutive_errors += 1;
                    let _ = channel.send(ImportEvent::Error {
                        message: format!("mkdir {}: {}", dest_dir.display(), e),
                    });
                    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                        let _ = channel.send(ImportEvent::Error {
                            message: format!("Aborting {} — too many consecutive errors", device_label),
                        });
                        return;
                    }
                    continue;
                }

                let mut dest_path = dest_dir.join(&file.file_name);
                if dest_path.exists() {
                    let stem = Path::new(&file.file_name)
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let ext = Path::new(&file.file_name)
                        .extension()
                        .map(|e| format!(".{}", e.to_string_lossy()))
                        .unwrap_or_default();
                    let mut counter = 1u32;
                    loop {
                        dest_path = dest_dir.join(format!("{}_{}{}", stem, counter, ext));
                        if !dest_path.exists() {
                            break;
                        }
                        counter += 1;
                    }
                }

                match copy_file_cancellable(&file.source_path, &dest_path, &cancel_token).await {
                    Ok(true) => {
                        consecutive_errors = 0;
                        bytes_copied += file.file_size;
                        files_copied += 1;

                        let relative_path = dest_path
                            .strip_prefix(&mount_point)
                            .unwrap_or(&dest_path)
                            .to_string_lossy()
                            .to_string();

                        let extension = dest_path
                            .extension()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_lowercase();

                        let _ = db::upsert_file(
                            &pool,
                            &file.blake3_hash,
                            file.file_size,
                            &file.file_name,
                            &extension,
                        )
                        .await;

                        let _ = db::upsert_location(
                            &pool,
                            &file.blake3_hash,
                            &device_id,
                            &relative_path,
                            &file.file_name,
                            file.file_size,
                            file.modified_at.as_deref(),
                            "import",
                        )
                        .await;

                        let _ = channel.send(ImportEvent::CopyProgress(DeviceCopyProgress {
                            device_id: device_id.clone(),
                            device_label: device_label.clone(),
                            bytes_copied,
                            total_bytes: device_total_bytes,
                            files_copied,
                            total_files: device_total_files,
                            current_file: file.file_name.clone(),
                        }));
                    }
                    Ok(false) => {
                        // Cancelled mid-copy, partial file already cleaned up
                        let _ = channel.send(ImportEvent::Cancelled);
                        return;
                    }
                    Err(e) => {
                        consecutive_errors += 1;
                        let _ = tokio::fs::remove_file(&dest_path).await;
                        let _ = channel.send(ImportEvent::Error {
                            message: format!("Copy {} failed: {}", file.file_name, e),
                        });
                        if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                            let _ = channel.send(ImportEvent::Error {
                                message: format!("Aborting {} — too many consecutive errors", device_label),
                            });
                            return;
                        }
                    }
                }
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }

    if !cancel_token.is_cancelled() {
        let _ = channel.send(ImportEvent::CopyComplete);
    }

    Ok(())
}
