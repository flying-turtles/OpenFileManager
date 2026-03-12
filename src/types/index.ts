export interface StorageDevice {
  id: string;
  label: string;
  mount_point: string;
  device_type: string; // "hot" | "cold" | "unknown"
  total_bytes: number;
  available_bytes: number;
  is_removable: boolean;
  first_seen: string;
  last_seen: string;
  is_connected: boolean;
}

export interface FileLocation {
  id: number;
  blake3_hash: string;
  device_id: string;
  file_path: string;
  file_name: string;
  file_size: number;
  modified_at: string | null;
  last_verified: string;
  scan_mode: string;
}

export interface FileSafety {
  blake3_hash: string;
  file_size: number;
  representative_name: string;
  total_copies: number;
  hot_copies: number;
  cold_copies: number;
  is_safe: boolean;
  locations: FileLocation[];
}

export interface WasteCandidate {
  blake3_hash: string;
  file_size: number;
  representative_name: string;
  total_copies: number;
  wasted_bytes: number;
}

export interface DashboardStats {
  total_files: number;
  total_locations: number;
  unsafe_files: number;
  total_devices: number;
  total_size_bytes: number;
}

export interface DirEntry {
  name: string;
  is_dir: boolean;
  size: number;
  modified: string | null;
}

export type ScanEvent =
  | { Started: { total_files: number } }
  | { Progress: { scanned: number; total: number } }
  | { HashingStarted: { to_hash: number; skipped: number } }
  | { FileHashed: { path: string; hash: string } }
  | { Finished: { scanned: number; hashed: number; added: number; removed: number } }
  | { Error: { message: string } }
  | "Cancelled"
  | { Paused: { scanned: number; hashed: number; added: number; total: number } };

export interface PendingScan {
  id: number;
  scan_type: string;
  target: string;
  device_id: string;
  mode: string;
  total_files: number;
  processed: number;
  hashed: number;
  added: number;
  paused_at: string;
}

export interface Project {
  id: number;
  title: string;
  description: string;
  start_date: string;
  end_date: string;
  created_at: string;
}

export interface ExtensionCount {
  extension: string;
  count: number;
}

export interface ProjectStats {
  total_files: number;
  total_size_bytes: number;
  backed_up_pct: number;
  extensions: ExtensionCount[];
}

export interface ProjectDetail {
  project: Project;
  stats: ProjectStats;
  files: FileSafety[];
}

export interface FilePageResult {
  files: FileLocation[];
  next_cursor: string | null;
  total: number;
}

export interface UnsafeFilePageResult {
  files: FileSafety[];
  total: number;
  has_more: boolean;
}

export interface NetworkDrive {
  id: string;
  label: string;
  protocol: string;
  host: string;
  share_path: string;
  username: string;
  mount_point: string;
  device_type: string;
  created_at: string;
  is_mounted: boolean;
}

export interface ImportFile {
  source_path: string;
  relative_path: string;
  file_name: string;
  blake3_hash: string;
  file_size: number;
  created_date: string;
  modified_at: string | null;
  existing_locations: FileLocation[];
}

export interface ImportAnalysis {
  sd_device_id: string;
  sd_label: string;
  files: ImportFile[];
  total_bytes: number;
  new_file_count: number;
  existing_file_count: number;
}

export interface DeviceCopyProgress {
  device_id: string;
  device_label: string;
  bytes_copied: number;
  total_bytes: number;
  files_copied: number;
  total_files: number;
  current_file: string;
}

export type ImportEvent =
  | { AnalysisStarted: { total_files: number } }
  | { AnalysisProgress: { processed: number; total: number } }
  | { AnalysisComplete: ImportAnalysis }
  | { CopyStarted: { total_files: number; device_count: number } }
  | { CopyProgress: DeviceCopyProgress }
  | "CopyComplete"
  | { Error: { message: string } }
  | "Cancelled"
  | { Paused: { processed: number; total: number } };
