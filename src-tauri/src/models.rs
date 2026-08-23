use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
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
    pub drive_speed: String,
    #[sqlx(default)]
    pub is_connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub struct DetectedDisk {
    pub id: String,
    pub label: String,
    pub mount_point: String,
    pub total_bytes: i64,
    pub available_bytes: i64,
    pub is_removable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub struct WasteCandidate {
    pub blake3_hash: String,
    pub file_size: i64,
    pub representative_name: String,
    pub total_copies: i64,
    pub wasted_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStats {
    pub total_files: i64,
    pub total_locations: i64,
    pub unsafe_files: i64,
    pub total_devices: i64,
    pub total_size_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: i64,
    pub modified: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScanEvent {
    #[serde(rename_all = "camelCase")]
    Started { total_files: u64 },
    Progress { scanned: u64, total: u64 },
    #[serde(rename_all = "camelCase")]
    HashingStarted { to_hash: u64, skipped: u64 },
    FileHashed { path: String, hash: String },
    #[serde(rename_all = "camelCase")]
    Finished { scanned: u64, hashed: u64, added: u64, removed: u64 },
    Error { message: String },
    Cancelled,
    #[serde(rename_all = "camelCase")]
    Paused { scanned: u64, hashed: u64, added: u64, total: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PendingScan {
    pub id: i64,
    pub scan_type: String,
    pub target: String,
    pub device_id: String,
    pub mode: String,
    pub total_files: i64,
    pub processed: i64,
    pub hashed: i64,
    pub added: i64,
    pub paused_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub struct ImportAnalysis {
    pub sd_device_id: String,
    pub sd_label: String,
    pub files: Vec<ImportFile>,
    pub total_bytes: i64,
    pub new_file_count: u64,
    pub existing_file_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub struct FilePageResult {
    pub files: Vec<FileLocation>,
    pub next_cursor: Option<String>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnsafeFilePageResult {
    pub files: Vec<FileSafety>,
    pub total: i64,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct NetworkDrive {
    pub id: String,
    pub label: String,
    pub protocol: String,
    pub host: String,
    pub share_path: String,
    pub username: String,
    pub mount_point: String,
    pub device_type: String,
    pub created_at: String,
    #[sqlx(default)]
    pub is_mounted: bool,
}

#[derive(Debug, Clone)]
pub struct DirCacheEntry {
    pub dir_mtime: String,
    pub file_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkDeleteResult {
    pub succeeded: Vec<i64>,
    pub failed: Vec<BulkDeleteError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkDeleteError {
    pub location_id: i64,
    pub file_path: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BulkDeleteEvent {
    #[serde(rename_all = "camelCase")]
    Progress {
        processed: u64,
        total: u64,
        current_file: String,
    },
    Complete(BulkDeleteResult),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferCheck {
    pub available_count: u64,
    pub unavailable_files: Vec<UnavailableFile>,
    pub total_bytes: i64,
    pub already_on_target: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnavailableFile {
    pub blake3_hash: String,
    pub representative_name: String,
    pub file_size: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransferEvent {
    #[serde(rename_all = "camelCase")]
    CopyStarted { total_files: u64, total_bytes: i64 },
    CopyProgress(DeviceCopyProgress),
    CopyComplete,
    Error { message: String },
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedTransferFile {
    pub blake3_hash: String,
    pub file_size: i64,
    pub file_name: String,
    pub source_path: String,
    pub modified_at: Option<String>,
    pub source_device_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImportEvent {
    #[serde(rename_all = "camelCase")]
    AnalysisStarted { total_files: u64 },
    AnalysisProgress { processed: u64, total: u64 },
    AnalysisComplete(ImportAnalysis),
    #[serde(rename_all = "camelCase")]
    CopyStarted { total_files: u64, device_count: u64 },
    CopyProgress(DeviceCopyProgress),
    CopyComplete,
    Error { message: String },
    Cancelled,
    Paused { processed: u64, total: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceCleanupFile {
    pub source_path: String,
    pub relative_path: String,
    pub file_name: String,
    pub file_size: i64,
    pub backup_device_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceCleanupPreview {
    pub sd_device_id: String,
    pub sd_label: String,
    /// Files to show. May be truncated — `file_count` is the true total.
    pub files: Vec<SourceCleanupFile>,
    pub file_count: i64,
    pub total_bytes: i64,
    pub skipped_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProjectSummary {
    pub id: i64,
    pub title: String,
    pub file_count: i64,
    pub total_bytes: i64,
}

/// How many scanned files share the same set of other devices holding a copy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanDeviceGroup {
    /// Devices *other* than the scanned one, sorted. Empty means the files
    /// exist nowhere else.
    pub device_ids: Vec<String>,
    pub file_count: i64,
    pub total_bytes: i64,
}

/// What the index knows about a scanned location once a scan finishes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub device_id: String,
    pub device_label: String,
    /// Path relative to the device mount point; empty when the whole device
    /// was scanned.
    pub scan_prefix: String,
    pub total_files: i64,
    pub total_bytes: i64,
    pub oldest_modified: Option<String>,
    pub newest_modified: Option<String>,
    pub projects: Vec<ScanProjectSummary>,
    pub unassigned_files: i64,
    pub device_groups: Vec<ScanDeviceGroup>,
    pub redundant_files: i64,
    pub redundant_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceCleanupError {
    pub source_path: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceCleanupResult {
    pub deleted: u64,
    pub bytes_freed: i64,
    pub failed: Vec<SourceCleanupError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceCleanupEvent {
    #[serde(rename_all = "camelCase")]
    Progress {
        processed: u64,
        total: u64,
        current_file: String,
    },
    Complete(SourceCleanupResult),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSettings {
    pub host: String,
    pub port: i64,
    pub database: String,
    pub username: String,
    pub has_password: bool,
    pub last_backup_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackupEvent {
    #[serde(rename_all = "camelCase")]
    Started { total_tables: u64 },
    #[serde(rename_all = "camelCase")]
    TableProgress { table: String, rows_copied: u64, total_rows: u64 },
    #[serde(rename_all = "camelCase")]
    TableDone { table: String, rows: u64 },
    #[serde(rename_all = "camelCase")]
    Finished { total_rows: u64, finished_at: String },
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarFile {
    pub blake3_hash: String,
    pub representative_name: String,
    pub file_size: i64,
    pub locations: Vec<FileLocation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarGroup {
    pub files: Vec<SimilarFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SimilarScanEvent {
    #[serde(rename_all = "camelCase")]
    Started { total: u64 },
    Progress { processed: u64, total: u64 },
    Finished { hashed: u64, failed: u64 },
    Error { message: String },
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VerifyEvent {
    #[serde(rename_all = "camelCase")]
    Started { total: u64 },
    #[serde(rename_all = "camelCase")]
    Progress { processed: u64, total: u64, current_file: String },
    #[serde(rename_all = "camelCase")]
    Corrupted { location_id: i64, file_path: String, file_name: String },
    #[serde(rename_all = "camelCase")]
    Finished { verified: u64, baselined: u64, modified: u64, corrupted: u64, missing: u64 },
    Error { message: String },
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectCheck {
    pub marker_status: String, // "match" | "mismatch" | "missing"
    pub foreign_id: Option<String>,
    pub found_files: i64,
    pub sampled_files: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveFile {
    pub source_path: String,
    pub dest_path: String,
    pub file_name: String,
    pub file_size: i64,
    pub modified_at: Option<String>,
    pub same_volume: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MovePlan {
    pub files: Vec<MoveFile>,
    pub total_files: u64,
    pub total_bytes: i64,
    pub source_device_id: String,
    pub source_mount: String,
    pub source_roots: Vec<String>,
    pub dest_device_id: String,
    pub dest_mount: String,
    pub dest_label: String,
    pub same_volume_count: u64,
}

/// What the frontend needs to describe a planned move. The full `MovePlan`
/// stays in `AppState` — a 100k-file folder would otherwise ship tens of
/// megabytes of per-file JSON across the IPC boundary for four aggregates.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MovePlanSummary {
    pub total_files: u64,
    pub total_bytes: i64,
    pub source_device_id: String,
    pub dest_device_id: String,
    pub dest_label: String,
    pub same_volume_count: u64,
}

impl From<&MovePlan> for MovePlanSummary {
    fn from(plan: &MovePlan) -> Self {
        Self {
            total_files: plan.total_files,
            total_bytes: plan.total_bytes,
            source_device_id: plan.source_device_id.clone(),
            dest_device_id: plan.dest_device_id.clone(),
            dest_label: plan.dest_label.clone(),
            same_volume_count: plan.same_volume_count,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveError {
    pub source_path: String,
    pub file_name: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveResult {
    pub moved: u64,
    pub bytes_moved: i64,
    pub failed: Vec<MoveError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MoveEvent {
    #[serde(rename_all = "camelCase")]
    Progress {
        processed: u64,
        total: u64,
        bytes_moved: i64,
        total_bytes: i64,
        current_file: String,
        phase: String,
    },
    FileFailed(MoveError),
    Complete(MoveResult),
    /// Carries the partial result: a cancelled run still reports what it moved.
    Cancelled(MoveResult),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffDeviceOption {
    pub device_id: String,
    pub label: String,
    pub file_count: i64,
    pub is_connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffFileEntry {
    pub blake3_hash: String,
    pub file_size: i64,
    pub file_name: String,
    /// Path relative to the mount of the device this entry belongs to. For a
    /// copy entry that is the target device, and the value is the mirrored
    /// source-of-truth relative path.
    pub relative_path: String,
    /// The backup `file_locations` row to delete. `None` for copy entries.
    pub location_id: Option<i64>,
    /// Absolute path on the source of truth. Set for copy entries only.
    pub source_path: Option<String>,
    /// Source-of-truth mtime, carried so a copied file is indexed with the
    /// same `modified_at` as its source. Project membership is derived from
    /// `MIN(modified_at)` over a hash's rows, so a `NULL` here would drop the
    /// file out of its project once the source-of-truth row is purged.
    pub modified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffUnreadable {
    pub device_id: String,
    pub relative_path: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffDeviceResult {
    pub device_id: String,
    pub device_label: String,
    /// Set when the device was not diffed at all, e.g. it is offline.
    pub skip_reason: Option<String>,
    pub to_delete: Vec<DiffFileEntry>,
    pub to_copy: Vec<DiffFileEntry>,
    pub delete_bytes: i64,
    pub copy_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDiff {
    pub project_id: i64,
    pub sot_device_id: String,
    pub sot_label: String,
    pub devices: Vec<DiffDeviceResult>,
    /// Stale source-of-truth rows, purged on resolve.
    pub purge_location_ids: Vec<i64>,
    pub unreadable: Vec<DiffUnreadable>,
    pub total_delete_files: i64,
    pub total_delete_bytes: i64,
    pub total_copy_files: i64,
    pub total_copy_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffCopyItem {
    pub blake3_hash: String,
    pub file_size: i64,
    pub file_name: String,
    pub source_path: String,
    pub target_device_id: String,
    /// Relative to the target device's mount point.
    pub relative_path: String,
    /// Source-of-truth mtime, indexed with the copied row. See
    /// `DiffFileEntry::modified_at`.
    pub modified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffCopyError {
    pub file_name: String,
    pub target_device_id: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffCopyResult {
    pub copied: i64,
    pub bytes_copied: i64,
    /// Targets that already held a file at the mirrored path.
    pub skipped: Vec<DiffCopyError>,
    pub failed: Vec<DiffCopyError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiffEvent {
    Started { total: u64 },
    #[serde(rename_all = "camelCase")]
    Progress {
        checked: u64,
        total: u64,
        current_device: String,
    },
    Finished,
    Error { message: String },
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiffCopyEvent {
    Progress(DeviceCopyProgress),
    Complete(DiffCopyResult),
    Error { message: String },
    Cancelled,
}
