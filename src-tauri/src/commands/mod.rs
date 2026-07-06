mod backup;
mod devices;
mod files;
mod import;
mod network;
mod preview;
mod projects;
mod scan;
mod similar;
mod transfer;
mod verify;

use std::sync::Arc;
use std::collections::HashSet;

use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::db::DbPool;
use crate::models::*;
use crate::scanner::ScanProgress;

pub struct AppState {
    pub pool: DbPool,
    pub cancel_token: Arc<Mutex<Option<CancellationToken>>>,
    pub scan_progress: Arc<Mutex<Option<Arc<ScanProgress>>>>,
    pub scan_target: Arc<Mutex<Option<String>>>,
    pub import_cancel_token: Arc<Mutex<Option<CancellationToken>>>,
    pub import_analysis: Arc<Mutex<Option<Arc<ImportAnalysis>>>>,
    pub transfer_cancel_token: Arc<Mutex<Option<CancellationToken>>>,
    pub similar_cancel_token: Arc<Mutex<Option<CancellationToken>>>,
    pub verify_cancel_token: Arc<Mutex<Option<CancellationToken>>>,
    pub transfer_resolved: Arc<Mutex<Option<(Vec<ResolvedTransferFile>, String, String, String)>>>,
}

/// Filesystem stat with a deadline. A stalled network mount can hang stat()
/// in uninterruptible I/O for minutes, blocking async runtime workers and
/// starving everything (including the DB pool) — treat "slow" as offline.
pub(crate) async fn path_online(path: impl Into<std::path::PathBuf>) -> bool {
    path_online_within(path, 2).await
}

/// Longer deadlines suit one-shot checks where a sleeping NAS may need to
/// spin up; the short default suits frequent polling.
pub(crate) async fn path_online_within(path: impl Into<std::path::PathBuf>, secs: u64) -> bool {
    let path: std::path::PathBuf = path.into();
    tokio::time::timeout(
        std::time::Duration::from_secs(secs),
        tokio::task::spawn_blocking(move || path.exists()),
    )
    .await
    .map(|r| r.unwrap_or(false))
    .unwrap_or(false)
}

async fn mark_connected(mut devices: Vec<StorageDevice>, connected_ids: &HashSet<String>) -> Vec<StorageDevice> {
    for dev in &mut devices {
        dev.is_connected =
            connected_ids.contains(&dev.id) || path_online(&dev.mount_point).await;
    }
    devices
}

pub use backup::*;
pub use devices::*;
pub use files::*;
pub use import::*;
pub use network::*;
pub use preview::*;
pub use projects::*;
pub use scan::*;
pub use similar::*;
pub use transfer::*;
pub use verify::*;
