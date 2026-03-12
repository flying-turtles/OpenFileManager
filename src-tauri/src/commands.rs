use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use tauri::ipc::Channel;
use tauri::State;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::db::{self, DbPool};
use crate::devices;
use crate::error::AppError;
use crate::models::*;
use crate::network;
use crate::scanner::ScanProgress;

pub struct AppState {
    pub pool: DbPool,
    pub cancel_token: Arc<Mutex<Option<CancellationToken>>>,
    pub scan_progress: Arc<Mutex<Option<Arc<ScanProgress>>>>,
    pub scan_target: Arc<Mutex<Option<String>>>,
    pub import_cancel_token: Arc<Mutex<Option<CancellationToken>>>,
    pub import_analysis: Arc<Mutex<Option<Arc<ImportAnalysis>>>>,
}

fn mark_connected(mut devices: Vec<StorageDevice>, connected_ids: &HashSet<String>) -> Vec<StorageDevice> {
    for dev in &mut devices {
        dev.is_connected = connected_ids.contains(&dev.id)
            || std::path::Path::new(&dev.mount_point).exists();
    }
    devices
}

#[tauri::command]
pub async fn detect_devices(state: State<'_, AppState>) -> Result<Vec<StorageDevice>, AppError> {
    let disks = devices::detect_volumes();
    let connected_ids: HashSet<String> = disks.iter().map(|d| d.id.clone()).collect();
    for disk in &disks {
        db::upsert_device(&state.pool, disk).await?;
    }
    let all = db::get_all_devices(&state.pool).await?;
    Ok(mark_connected(all, &connected_ids))
}

#[tauri::command]
pub async fn get_devices(state: State<'_, AppState>) -> Result<Vec<StorageDevice>, AppError> {
    let disks = devices::detect_volumes();
    let connected_ids: HashSet<String> = disks.iter().map(|d| d.id.clone()).collect();
    let all = db::get_all_devices(&state.pool).await?;
    Ok(mark_connected(all, &connected_ids))
}

#[tauri::command]
pub async fn set_device_type(
    state: State<'_, AppState>,
    device_id: String,
    device_type: String,
) -> Result<(), AppError> {
    db::set_device_type(&state.pool, &device_id, &device_type).await
}

#[tauri::command]
pub async fn remove_device(
    state: State<'_, AppState>,
    device_id: String,
) -> Result<(), AppError> {
    db::delete_device(&state.pool, &device_id).await
}

#[tauri::command]
pub async fn start_scan(
    state: State<'_, AppState>,
    target: String,
    on_event: Channel<ScanEvent>,
) -> Result<(), AppError> {
    let pool = state.pool.clone();
    let cancel_token = CancellationToken::new();
    let progress = Arc::new(ScanProgress::new());

    {
        let mut guard = state.cancel_token.lock().await;
        *guard = Some(cancel_token.clone());
    }
    {
        let mut guard = state.scan_progress.lock().await;
        *guard = Some(progress.clone());
    }
    {
        let mut guard = state.scan_target.lock().await;
        *guard = Some(target.clone());
    }
    // Remove any existing pending scan for this target
    let _ = db::delete_pending_scan_by_target(&pool, "scan", &target).await;

    let target = PathBuf::from(target);
    tokio::spawn(async move {
        if let Err(e) = crate::scanner::run_scan(pool, target, on_event.clone(), cancel_token, progress).await {
            let _ = on_event.send(ScanEvent::Error {
                message: e.to_string(),
            });
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn cancel_scan(state: State<'_, AppState>) -> Result<(), AppError> {
    let guard = state.cancel_token.lock().await;
    if let Some(token) = guard.as_ref() {
        token.cancel();
    }
    Ok(())
}

#[tauri::command]
pub async fn pause_scan(state: State<'_, AppState>) -> Result<(), AppError> {
    // Set pausing flag then cancel
    let progress_guard = state.scan_progress.lock().await;
    if let Some(progress) = progress_guard.as_ref() {
        progress.pausing.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    let token_guard = state.cancel_token.lock().await;
    if let Some(token) = token_guard.as_ref() {
        token.cancel();
    }
    drop(token_guard);
    drop(progress_guard);

    // Persist the pending scan
    let target = state.scan_target.lock().await.clone().unwrap_or_default();
    if !target.is_empty() {
        let progress_guard = state.scan_progress.lock().await;
        if let Some(p) = progress_guard.as_ref() {
            let volumes = devices::detect_volumes();
            let device_id = devices::device_for_path(&volumes, &target)
                .map(|(id, _)| id)
                .unwrap_or_default();
            db::upsert_pending_scan(
                &state.pool,
                "scan",
                &target,
                &device_id,
                "quick",
                p.total.load(std::sync::atomic::Ordering::Relaxed) as i64,
                p.scanned.load(std::sync::atomic::Ordering::Relaxed) as i64,
                p.hashed.load(std::sync::atomic::Ordering::Relaxed) as i64,
                p.added.load(std::sync::atomic::Ordering::Relaxed) as i64,
            ).await?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn get_pending_scans(state: State<'_, AppState>) -> Result<Vec<PendingScan>, AppError> {
    db::get_pending_scans(&state.pool).await
}

#[tauri::command]
pub async fn dismiss_pending_scan(state: State<'_, AppState>, id: i64) -> Result<(), AppError> {
    db::delete_pending_scan(&state.pool, id).await
}

#[tauri::command]
pub async fn get_files_on_device(
    state: State<'_, AppState>,
    device_id: String,
) -> Result<Vec<FileLocation>, AppError> {
    db::get_files_on_device(&state.pool, &device_id).await
}

#[tauri::command]
pub async fn get_files_on_device_page(
    state: State<'_, AppState>,
    device_id: String,
    cursor: Option<String>,
    limit: Option<i64>,
) -> Result<FilePageResult, AppError> {
    let limit = limit.unwrap_or(500);
    let (files, next_cursor, total) =
        db::get_files_on_device_page(&state.pool, &device_id, cursor.as_deref(), limit).await?;
    Ok(FilePageResult { files, next_cursor, total })
}

#[tauri::command]
pub async fn get_unsafe_files_page(
    state: State<'_, AppState>,
    offset: Option<i64>,
    limit: Option<i64>,
) -> Result<UnsafeFilePageResult, AppError> {
    let (files, total, has_more) =
        db::get_unsafe_files_page(&state.pool, offset.unwrap_or(0), limit.unwrap_or(500)).await?;
    Ok(UnsafeFilePageResult { files, total, has_more })
}

#[tauri::command]
pub async fn get_safe_files_page(
    state: State<'_, AppState>,
    offset: Option<i64>,
    limit: Option<i64>,
) -> Result<UnsafeFilePageResult, AppError> {
    let (files, total, has_more) =
        db::get_safe_files_page(&state.pool, offset.unwrap_or(0), limit.unwrap_or(500)).await?;
    Ok(UnsafeFilePageResult { files, total, has_more })
}

#[tauri::command]
pub async fn get_file_safety(
    state: State<'_, AppState>,
    hash: String,
) -> Result<Option<FileSafety>, AppError> {
    db::get_file_safety(&state.pool, &hash).await
}

#[tauri::command]
pub async fn get_unsafe_files(state: State<'_, AppState>) -> Result<Vec<FileSafety>, AppError> {
    db::get_unsafe_files(&state.pool).await
}

#[tauri::command]
pub async fn get_waste_candidates(
    state: State<'_, AppState>,
    threshold: Option<i64>,
) -> Result<Vec<WasteCandidate>, AppError> {
    db::get_waste_candidates(&state.pool, threshold.unwrap_or(2)).await
}

#[tauri::command]
pub async fn browse_directory(path: String) -> Result<Vec<DirEntry>, AppError> {
    let path = PathBuf::from(&path);
    let mut entries = Vec::new();

    let mut dir = tokio::fs::read_dir(&path).await?;
    while let Some(entry) = dir.next_entry().await? {
        let metadata = entry.metadata().await?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::SystemTime::UNIX_EPOCH).ok())
            .map(|d| {
                chrono::DateTime::from_timestamp(d.as_secs() as i64, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            })
            .flatten();

        entries.push(DirEntry {
            name,
            is_dir: metadata.is_dir(),
            size: metadata.len() as i64,
            modified,
        });
    }

    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(entries)
}

#[tauri::command]
pub async fn get_file_locations(
    state: State<'_, AppState>,
    hash: String,
) -> Result<Vec<FileLocation>, AppError> {
    db::get_file_locations(&state.pool, &hash).await
}

#[tauri::command]
pub async fn get_dashboard_stats(state: State<'_, AppState>) -> Result<DashboardStats, AppError> {
    db::get_dashboard_stats(&state.pool).await
}

#[tauri::command]
pub async fn analyze_sd_card(
    state: State<'_, AppState>,
    sd_mount: String,
    on_event: Channel<ImportEvent>,
) -> Result<(), AppError> {
    let pool = state.pool.clone();
    let import_analysis = state.import_analysis.clone();
    let cancel_token = CancellationToken::new();

    {
        let mut guard = state.import_cancel_token.lock().await;
        *guard = Some(cancel_token.clone());
    }

    tokio::spawn(async move {
        match crate::importer::analyze_sd_card(pool, PathBuf::from(sd_mount), on_event.clone(), cancel_token).await {
            Ok(analysis) => {
                let analysis_arc = Arc::new(analysis.clone());
                *import_analysis.lock().await = Some(analysis_arc);
                let _ = on_event.send(ImportEvent::AnalysisComplete(analysis));
            }
            Err(e) => {
                if e.to_string() != "Cancelled" {
                    let _ = on_event.send(ImportEvent::Error { message: e.to_string() });
                }
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn start_import(
    state: State<'_, AppState>,
    target_device_ids: Vec<String>,
    on_event: Channel<ImportEvent>,
) -> Result<(), AppError> {
    let analysis = {
        let guard = state.import_analysis.lock().await;
        guard
            .clone()
            .ok_or_else(|| AppError::General("No analysis available. Run analyze first.".into()))?
    };

    let pool = state.pool.clone();
    let cancel_token = CancellationToken::new();

    {
        let mut guard = state.import_cancel_token.lock().await;
        *guard = Some(cancel_token.clone());
    }

    let volumes = crate::devices::detect_volumes();
    let mut targets = Vec::new();
    for id in target_device_ids {
        let vol = volumes
            .iter()
            .find(|v| v.id == id)
            .ok_or_else(|| AppError::General(format!("Device {} not connected", id)))?;
        targets.push((vol.id.clone(), vol.mount_point.clone(), vol.label.clone()));
    }

    tokio::spawn(async move {
        if let Err(e) = crate::importer::run_import(pool, analysis, targets, on_event.clone(), cancel_token).await {
            let _ = on_event.send(ImportEvent::Error {
                message: e.to_string(),
            });
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn cancel_import(state: State<'_, AppState>) -> Result<(), AppError> {
    let guard = state.import_cancel_token.lock().await;
    if let Some(token) = guard.as_ref() {
        token.cancel();
    }
    Ok(())
}

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

// --- Network drive commands ---

#[tauri::command]
pub async fn add_network_drive(
    state: State<'_, AppState>,
    protocol: String,
    host: String,
    share_path: String,
    username: String,
    password: String,
    label: String,
    device_type: String,
) -> Result<NetworkDrive, AppError> {
    let id = network::generate_drive_id(&protocol, &host, &share_path);
    let mount_point = format!("/Volumes/{}", label);

    let drive = NetworkDrive {
        id: id.clone(),
        label,
        protocol,
        host,
        share_path,
        username: username.clone(),
        mount_point,
        device_type,
        created_at: String::new(),
        is_mounted: false,
    };

    db::insert_network_drive(&state.pool, &drive).await?;

    if !password.is_empty() {
        network::keychain_store(&id, &username, &password)?;
    }

    let mut saved = db::get_network_drive(&state.pool, &id).await?;
    saved.is_mounted = network::is_mountpoint(&saved.mount_point);
    Ok(saved)
}

#[tauri::command]
pub async fn get_network_drives(state: State<'_, AppState>) -> Result<Vec<NetworkDrive>, AppError> {
    let mut drives = db::get_all_network_drives(&state.pool).await?;
    for drive in &mut drives {
        drive.is_mounted = network::is_mountpoint(&drive.mount_point);
    }
    Ok(drives)
}

#[tauri::command]
pub async fn mount_network_drive(state: State<'_, AppState>, drive_id: String) -> Result<(), AppError> {
    let drive = db::get_network_drive(&state.pool, &drive_id).await?;

    match drive.protocol.as_str() {
        "smb" => {
            let password = if !drive.username.is_empty() {
                network::keychain_load(&drive_id, &drive.username).unwrap_or_default()
            } else {
                String::new()
            };
            network::mount_smb(&drive.host, &drive.share_path, &drive.username, &password, &drive.mount_point)?;
        }
        "nfs" => {
            network::mount_nfs(&drive.host, &drive.share_path, &drive.mount_point)?;
        }
        _ => return Err(AppError::General(format!("Unsupported protocol: {}", drive.protocol))),
    }

    // Upsert into storage_devices so scanning/file tracking works
    let disk = DetectedDisk {
        id: drive.id.clone(),
        label: drive.label.clone(),
        mount_point: drive.mount_point.clone(),
        total_bytes: 0,
        available_bytes: 0,
        is_removable: false,
    };
    db::upsert_device(&state.pool, &disk).await?;
    db::set_device_type(&state.pool, &drive.id, &drive.device_type).await?;

    Ok(())
}

#[tauri::command]
pub async fn unmount_network_drive(state: State<'_, AppState>, drive_id: String) -> Result<(), AppError> {
    let drive = db::get_network_drive(&state.pool, &drive_id).await?;
    network::unmount_drive(&drive.mount_point)?;
    Ok(())
}

#[tauri::command]
pub async fn remove_network_drive(state: State<'_, AppState>, drive_id: String) -> Result<(), AppError> {
    let drive = db::get_network_drive(&state.pool, &drive_id).await?;

    // Unmount if mounted
    if network::is_mountpoint(&drive.mount_point) {
        let _ = network::unmount_drive(&drive.mount_point);
    }

    // Delete keychain entry
    if !drive.username.is_empty() {
        let _ = network::keychain_delete(&drive_id, &drive.username);
    }

    db::delete_network_drive(&state.pool, &drive_id).await?;
    Ok(())
}

// --- Add location as device ---

#[tauri::command]
pub async fn add_location(
    state: State<'_, AppState>,
    path: String,
    label: String,
    device_type: String,
) -> Result<StorageDevice, AppError> {
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err(AppError::General(format!("Path does not exist: {}", path)));
    }

    let id = blake3::hash(path.as_bytes()).to_hex()[..16].to_string();

    let disk = DetectedDisk {
        id: id.clone(),
        label,
        mount_point: path,
        total_bytes: 0,
        available_bytes: 0,
        is_removable: false,
    };
    db::upsert_device(&state.pool, &disk).await?;
    db::set_device_type(&state.pool, &id, &device_type).await?;

    let devices = db::get_all_devices(&state.pool).await?;
    devices
        .into_iter()
        .find(|d| d.id == id)
        .ok_or_else(|| AppError::General("Failed to retrieve added device".into()))
}

#[tauri::command]
pub async fn eject_device(mount_point: String) -> Result<(), AppError> {
    let output = std::process::Command::new("diskutil")
        .args(["eject", &mount_point])
        .output()?;
    if !output.status.success() {
        return Err(AppError::General(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }
    Ok(())
}
