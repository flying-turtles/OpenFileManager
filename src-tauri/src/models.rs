use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct StorageDevice {
    pub id: String,
    pub label: String,
    pub mount_point: String,
    pub device_type: String,
    pub total_bytes: i64,
    pub available_bytes: i64,
    pub is_removable: bool,
    pub first_seen: String,
    pub last_seen: String,
    #[sqlx(default)]
    pub is_connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct FileLocation {
    pub id: i64,
    pub blake3_hash: String,
    pub device_id: String,
    pub file_path: String,
    pub file_name: String,
    pub file_size: i64,
    pub modified_at: Option<String>,
    pub last_verified: String,
    pub scan_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedDisk {
    pub id: String,
    pub label: String,
    pub mount_point: String,
    pub total_bytes: i64,
    pub available_bytes: i64,
    pub is_removable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSafety {
    pub blake3_hash: String,
    pub file_size: i64,
    pub representative_name: String,
    pub total_copies: i64,
    pub hot_copies: i64,
    pub cold_copies: i64,
    pub is_safe: bool,
    pub locations: Vec<FileLocation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct WasteCandidate {
    pub blake3_hash: String,
    pub file_size: i64,
    pub representative_name: String,
    pub total_copies: i64,
    pub wasted_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardStats {
    pub total_files: i64,
    pub total_locations: i64,
    pub unsafe_files: i64,
    pub total_devices: i64,
    pub total_size_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: i64,
    pub modified: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScanEvent {
    Started { total_files: u64 },
    Progress { scanned: u64, total: u64 },
    FileHashed { path: String, hash: String },
    Finished { scanned: u64, hashed: u64, added: u64, removed: u64 },
    Error { message: String },
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportFile {
    pub source_path: String,
    pub relative_path: String,
    pub file_name: String,
    pub blake3_hash: String,
    pub file_size: i64,
    pub created_date: String,
    pub modified_at: Option<String>,
    pub existing_locations: Vec<FileLocation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportAnalysis {
    pub sd_device_id: String,
    pub sd_label: String,
    pub files: Vec<ImportFile>,
    pub total_bytes: i64,
    pub new_file_count: u64,
    pub existing_file_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCopyProgress {
    pub device_id: String,
    pub device_label: String,
    pub bytes_copied: i64,
    pub total_bytes: i64,
    pub files_copied: u64,
    pub total_files: u64,
    pub current_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Project {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub start_date: String,
    pub end_date: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionCount {
    pub extension: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectStats {
    pub total_files: i64,
    pub total_size_bytes: i64,
    pub backed_up_pct: f64,
    pub extensions: Vec<ExtensionCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDetail {
    pub project: Project,
    pub stats: ProjectStats,
    pub files: Vec<FileSafety>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImportEvent {
    AnalysisStarted { total_files: u64 },
    AnalysisProgress { processed: u64, total: u64 },
    AnalysisComplete(ImportAnalysis),
    CopyStarted { total_files: u64, device_count: u64 },
    CopyProgress(DeviceCopyProgress),
    CopyComplete,
    Error { message: String },
    Cancelled,
}
