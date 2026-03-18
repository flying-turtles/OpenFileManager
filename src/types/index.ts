export interface StorageDevice {
  id: string;
  label: string;
  mountPoint: string;
  deviceType: string; // "hot" | "cold" | "unknown"
  totalBytes: number;
  availableBytes: number;
  isRemovable: boolean;
  firstSeen: string;
  lastSeen: string;
  driveSpeed: string;
  isConnected: boolean;
}

export interface FileLocation {
  id: number;
  blake3Hash: string;
  deviceId: string;
  filePath: string;
  fileName: string;
  fileSize: number;
  modifiedAt: string | null;
  lastVerified: string;
  scanMode: string;
}

export interface FileSafety {
  blake3Hash: string;
  fileSize: number;
  representativeName: string;
  totalCopies: number;
  hotCopies: number;
  coldCopies: number;
  isSafe: boolean;
  locations: FileLocation[];
}

export interface WasteCandidate {
  blake3Hash: string;
  fileSize: number;
  representativeName: string;
  totalCopies: number;
  wastedBytes: number;
}

export interface DashboardStats {
  totalFiles: number;
  totalLocations: number;
  unsafeFiles: number;
  totalDevices: number;
  totalSizeBytes: number;
}

export interface DirEntry {
  name: string;
  isDir: boolean;
  size: number;
  modified: string | null;
}

export type ScanEvent =
  | { Started: { totalFiles: number } }
  | { Progress: { scanned: number; total: number } }
  | { HashingStarted: { toHash: number; skipped: number } }
  | { FileHashed: { path: string; hash: string } }
  | { Finished: { scanned: number; hashed: number; added: number; removed: number } }
  | { Error: { message: string } }
  | "Cancelled"
  | { Paused: { scanned: number; hashed: number; added: number; total: number } };

export interface PendingScan {
  id: number;
  scanType: string;
  target: string;
  deviceId: string;
  mode: string;
  totalFiles: number;
  processed: number;
  hashed: number;
  added: number;
  pausedAt: string;
}

export interface Project {
  id: number;
  title: string;
  description: string;
  startDate: string;
  endDate: string;
  createdAt: string;
}

export interface ExtensionCount {
  extension: string;
  count: number;
}

export interface ProjectStats {
  totalFiles: number;
  totalSizeBytes: number;
  backedUpPct: number;
  extensions: ExtensionCount[];
}

export interface ProjectDetail {
  project: Project;
  stats: ProjectStats;
  files: FileSafety[];
}

export interface FilePageResult {
  files: FileLocation[];
  nextCursor: string | null;
  total: number;
}

export interface UnsafeFilePageResult {
  files: FileSafety[];
  total: number;
  hasMore: boolean;
}

export interface NetworkDrive {
  id: string;
  label: string;
  protocol: string;
  host: string;
  sharePath: string;
  username: string;
  mountPoint: string;
  deviceType: string;
  createdAt: string;
  isMounted: boolean;
}

export interface ImportFile {
  sourcePath: string;
  relativePath: string;
  fileName: string;
  blake3Hash: string;
  fileSize: number;
  createdDate: string;
  modifiedAt: string | null;
  existingLocations: FileLocation[];
}

export interface ImportAnalysis {
  sdDeviceId: string;
  sdLabel: string;
  files: ImportFile[];
  totalBytes: number;
  newFileCount: number;
  existingFileCount: number;
}

export interface DeviceCopyProgress {
  deviceId: string;
  deviceLabel: string;
  bytesCopied: number;
  totalBytes: number;
  filesCopied: number;
  totalFiles: number;
  currentFile: string;
}

export interface BulkDeleteResult {
  succeeded: number[];
  failed: BulkDeleteError[];
}

export interface BulkDeleteError {
  locationId: number;
  filePath: string;
  error: string;
}

export type BulkDeleteEvent =
  | { Progress: { processed: number; total: number; currentFile: string } }
  | { Complete: BulkDeleteResult };

export interface TransferCheck {
  availableCount: number;
  unavailableFiles: UnavailableFile[];
  totalBytes: number;
  alreadyOnTarget: number;
}

export interface UnavailableFile {
  blake3Hash: string;
  representativeName: string;
  fileSize: number;
}

export type TransferEvent =
  | { CopyStarted: { totalFiles: number; totalBytes: number } }
  | { CopyProgress: DeviceCopyProgress }
  | "CopyComplete"
  | { Error: { message: string } }
  | "Cancelled";

export type ImportEvent =
  | { AnalysisStarted: { totalFiles: number } }
  | { AnalysisProgress: { processed: number; total: number } }
  | { AnalysisComplete: ImportAnalysis }
  | { CopyStarted: { totalFiles: number; deviceCount: number } }
  | { CopyProgress: DeviceCopyProgress }
  | "CopyComplete"
  | { Error: { message: string } }
  | "Cancelled"
  | { Paused: { processed: number; total: number } };
