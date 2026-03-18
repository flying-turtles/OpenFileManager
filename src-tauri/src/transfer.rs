use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tauri::ipc::Channel;
use tokio_util::sync::CancellationToken;

use crate::db::{self, DbPool};
use crate::error::AppError;
use crate::models::*;

/// Check which project files are available for transfer and resolve source paths.
/// Prefers fast drives over slow ones when picking a source location.
pub fn check_and_resolve(
    files: &[FileSafety],
    connected_devices: &HashMap<String, StorageDevice>,
    target_device_id: &str,
) -> (TransferCheck, Vec<ResolvedTransferFile>) {
    let mut available_count: u64 = 0;
    let mut unavailable_files = Vec::new();
    let mut total_bytes: i64 = 0;
    let mut already_on_target: u64 = 0;
    let mut resolved = Vec::new();

    for file in files {
        // Skip if already on target device
        if file.locations.iter().any(|l| l.device_id == target_device_id) {
            already_on_target += 1;
            continue;
        }

        // Find connected locations
        let connected_locs: Vec<&FileLocation> = file
            .locations
            .iter()
            .filter(|l| connected_devices.contains_key(&l.device_id))
            .collect();

        if connected_locs.is_empty() {
            unavailable_files.push(UnavailableFile {
                blake3_hash: file.blake3_hash.clone(),
                representative_name: file.representative_name.clone(),
                file_size: file.file_size,
            });
            continue;
        }

        // Pick best source: prefer fast drives
        let best = connected_locs
            .iter()
            .max_by_key(|l| {
                let dev = connected_devices.get(&l.device_id);
                if dev.map(|d| d.drive_speed == "fast").unwrap_or(false) {
                    1
                } else {
                    0
                }
            })
            .unwrap();

        let dev = connected_devices.get(&best.device_id).unwrap();
        let source_path = PathBuf::from(&dev.mount_point)
            .join(&best.file_path)
            .to_string_lossy()
            .to_string();

        resolved.push(ResolvedTransferFile {
            blake3_hash: file.blake3_hash.clone(),
            file_size: file.file_size,
            file_name: file.representative_name.clone(),
            source_path,
            modified_at: best.modified_at.clone(),
        });

        available_count += 1;
        total_bytes += file.file_size;
    }

    let check = TransferCheck {
        available_count,
        unavailable_files,
        total_bytes,
        already_on_target,
    };

    (check, resolved)
}

/// Copy resolved files to the destination drive.
/// Uses date-based destination dirs, same dedup naming as importer.
pub async fn run_transfer(
    pool: DbPool,
    files: Vec<ResolvedTransferFile>,
    dest_device_id: String,
    dest_mount: String,
    dest_label: String,
    channel: Channel<TransferEvent>,
    cancel_token: CancellationToken,
) -> Result<(), AppError> {
    let total_files = files.len() as u64;
    let total_bytes: i64 = files.iter().map(|f| f.file_size).sum();

    let _ = channel.send(TransferEvent::CopyStarted {
        total_files,
        total_bytes,
    });

    let mut bytes_copied: i64 = 0;
    let mut files_copied: u64 = 0;

    for file in &files {
        if cancel_token.is_cancelled() {
            let _ = channel.send(TransferEvent::Cancelled);
            return Ok(());
        }

        // Determine date-based dest dir from modified_at
        let date_dir = file
            .modified_at
            .as_deref()
            .and_then(|s| s.get(..10))
            .unwrap_or("unknown");

        let dest_dir = PathBuf::from(&dest_mount).join(date_dir);
        if let Err(e) = tokio::fs::create_dir_all(&dest_dir).await {
            let _ = channel.send(TransferEvent::Error {
                message: format!("mkdir {}: {}", dest_dir.display(), e),
            });
            continue;
        }

        // Dedup naming
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

        match tokio::fs::copy(&file.source_path, &dest_path).await {
            Ok(_) => {
                bytes_copied += file.file_size;
                files_copied += 1;

                let relative_path = dest_path
                    .strip_prefix(&dest_mount)
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
                    &dest_device_id,
                    &relative_path,
                    &file.file_name,
                    file.file_size,
                    file.modified_at.as_deref(),
                    "transfer",
                )
                .await;

                let _ = channel.send(TransferEvent::CopyProgress(DeviceCopyProgress {
                    device_id: dest_device_id.clone(),
                    device_label: dest_label.clone(),
                    bytes_copied,
                    total_bytes,
                    files_copied,
                    total_files,
                    current_file: file.file_name.clone(),
                }));
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&dest_path).await;
                let _ = channel.send(TransferEvent::Error {
                    message: format!("Copy {} failed: {}", file.file_name, e),
                });
            }
        }
    }

    if !cancel_token.is_cancelled() {
        let _ = channel.send(TransferEvent::CopyComplete);
    }

    Ok(())
}
