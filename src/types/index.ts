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
  | { FileHashed: { path: string; hash: string } }
  | { Finished: { scanned: number; hashed: number } }
  | { Error: { message: string } }
  | "Cancelled";
