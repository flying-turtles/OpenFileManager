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

export interface SourceCleanupFile {
  sourcePath: string;
  relativePath: string;
  fileName: string;
  fileSize: number;
  backupDeviceIds: string[];
}

export interface SourceCleanupPreview {
  sdDeviceId: string;
  sdLabel: string;
  /** May be truncated for display — `fileCount` is the true total. */
  files: SourceCleanupFile[];
  fileCount: number;
  totalBytes: number;
  skippedCount: number;
}

export interface ScanProjectSummary {
  id: number;
  title: string;
  fileCount: number;
  totalBytes: number;
}

export interface ScanDeviceGroup {
  /** Devices other than the scanned one, sorted. Empty = nowhere else. */
  deviceIds: string[];
  fileCount: number;
  totalBytes: number;
}

export interface ScanSummary {
  deviceId: string;
  deviceLabel: string;
  /** Empty when the whole device was scanned. */
  scanPrefix: string;
  totalFiles: number;
  totalBytes: number;
  oldestModified: string | null;
  newestModified: string | null;
  projects: ScanProjectSummary[];
  unassignedFiles: number;
  deviceGroups: ScanDeviceGroup[];
  redundantFiles: number;
  redundantBytes: number;
}

export interface SourceCleanupError {
  sourcePath: string;
  error: string;
}

export interface SourceCleanupResult {
  deleted: number;
  bytesFreed: number;
  failed: SourceCleanupError[];
}

export type SourceCleanupEvent =
  | { Progress: { processed: number; total: number; currentFile: string } }
  | { Complete: SourceCleanupResult };

export interface BackupSettings {
  host: string;
  port: number;
  database: string;
  username: string;
  hasPassword: boolean;
  lastBackupAt: string | null;
}

export type BackupEvent =
  | { Started: { totalTables: number } }
  | { TableProgress: { table: string; rowsCopied: number; totalRows: number } }
  | { TableDone: { table: string; rows: number } }
  | { Finished: { totalRows: number; finishedAt: string } }
  | { Error: { message: string } };

export interface SimilarFile {
  blake3Hash: string;
  representativeName: string;
  fileSize: number;
  locations: FileLocation[];
}

export interface SimilarGroup {
  files: SimilarFile[];
}

export type SimilarScanEvent =
  | { Started: { total: number } }
  | { Progress: { processed: number; total: number } }
  | { Finished: { hashed: number; failed: number } }
  | { Error: { message: string } }
  | "Cancelled";

export type VerifyEvent =
  | { Started: { total: number } }
  | { Progress: { processed: number; total: number; currentFile: string } }
  | { Corrupted: { locationId: number; filePath: string; fileName: string } }
  | { Finished: { verified: number; baselined: number; modified: number; corrupted: number; missing: number } }
  | { Error: { message: string } }
  | "Cancelled";

export interface ReconnectCheck {
  markerStatus: "match" | "mismatch" | "missing";
  foreignId: string | null;
  foundFiles: number;
  sampledFiles: number;
}

/// What `plan_move` returns. The per-file list stays in the Rust AppState —
/// shipping it would mean tens of megabytes of JSON for a large folder.
export interface MovePlanSummary {
  totalFiles: number;
  totalBytes: number;
  sourceDeviceId: string;
  destDeviceId: string;
  destLabel: string;
  sameVolumeCount: number;
}

export interface MoveErrorItem {
  sourcePath: string;
  fileName: string;
  error: string;
}

export interface MoveResult {
  moved: number;
  bytesMoved: number;
  failed: MoveErrorItem[];
}

export type MovePhase = "copying" | "verifying" | "deleting";

export type MoveEvent =
  | {
      Progress: {
        processed: number;
        total: number;
        bytesMoved: number;
        totalBytes: number;
        currentFile: string;
        phase: MovePhase;
      };
    }
  | { FileFailed: MoveErrorItem }
  | { Complete: MoveResult }
  // Unlike the other event unions, Move's Cancelled carries the partial
  // result — a cancelled run reports what it completed.
  | { Cancelled: MoveResult };
