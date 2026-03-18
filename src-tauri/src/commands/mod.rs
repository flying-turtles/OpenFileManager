mod devices;
mod files;
mod import;
mod network;
mod preview;
mod projects;
mod scan;
mod transfer;

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
}

fn mark_connected(mut devices: Vec<StorageDevice>, connected_ids: &HashSet<String>) -> Vec<StorageDevice> {
    for dev in &mut devices {
        dev.is_connected = connected_ids.contains(&dev.id)
            || std::path::Path::new(&dev.mount_point).exists();
    }
    devices
}

pub use devices::*;
pub use files::*;
pub use import::*;
pub use network::*;
pub use preview::*;
pub use projects::*;
pub use scan::*;
pub use transfer::*;
