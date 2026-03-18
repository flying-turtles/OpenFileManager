import { invoke, Channel } from "@tauri-apps/api/core";
import type {
  StorageDevice,
  FileLocation,
  FileSafety,
  WasteCandidate,
  DashboardStats,
  DirEntry,
  ScanEvent,
  ImportEvent,
  Project,
  ProjectDetail,
  NetworkDrive,
  PendingScan,
  FilePageResult,
  UnsafeFilePageResult,
  BulkDeleteResult,
  BulkDeleteEvent,
} from "../types";

export async function detectDevices(): Promise<StorageDevice[]> {
  return invoke("detect_devices");
}

export async function getDevices(): Promise<StorageDevice[]> {
  return invoke("get_devices");
}

export async function setDeviceType(
  deviceId: string,
  deviceType: string
): Promise<void> {
  return invoke("set_device_type", {
    deviceId,
    deviceType,
  });
}

export async function removeDevice(deviceId: string): Promise<void> {
  return invoke("remove_device", { deviceId });
}

export async function startScan(
  target: string,
  onEvent: (event: ScanEvent) => void
): Promise<void> {
  const channel = new Channel<ScanEvent>();
  channel.onmessage = onEvent;
  return invoke("start_scan", { target, onEvent: channel });
}

export async function cancelScan(): Promise<void> {
  return invoke("cancel_scan");
}

export async function pauseScan(): Promise<void> {
  return invoke("pause_scan");
}

export async function getPendingScans(): Promise<PendingScan[]> {
  return invoke("get_pending_scans");
}

export async function dismissPendingScan(id: number): Promise<void> {
  return invoke("dismiss_pending_scan", { id });
}

export async function getFilesOnDevice(
  deviceId: string
): Promise<FileLocation[]> {
  return invoke("get_files_on_device", { deviceId });
}

export async function getFilesOnDevicePage(
  deviceId: string,
  cursor?: string,
  limit?: number
): Promise<FilePageResult> {
  return invoke("get_files_on_device_page", { deviceId, cursor, limit });
}

export async function getUnsafeFilesPage(
  offset?: number,
  limit?: number
): Promise<UnsafeFilePageResult> {
  return invoke("get_unsafe_files_page", { offset, limit });
}

export async function getSafeFilesPage(
  offset?: number,
  limit?: number
): Promise<UnsafeFilePageResult> {
  return invoke("get_safe_files_page", { offset, limit });
}

export async function getDuplicateFilesPage(
  offset?: number,
  limit?: number,
  deviceId?: string,
  sameDriveOnly?: boolean
): Promise<UnsafeFilePageResult> {
  return invoke("get_duplicate_files_page", { offset, limit, deviceId, sameDriveOnly });
}

export async function deleteFileCopy(locationId: number): Promise<void> {
  return invoke("delete_file_copy", { locationId });
}

export async function bulkDeleteFileCopies(
  locationIds: number[],
  onEvent: (event: BulkDeleteEvent) => void
): Promise<BulkDeleteResult> {
  const channel = new Channel<BulkDeleteEvent>();
  channel.onmessage = onEvent;
  return invoke("bulk_delete_file_copies", { locationIds, onEvent: channel });
}

export async function getFileSafety(
  hash: string
): Promise<FileSafety | null> {
  return invoke("get_file_safety", { hash });
}

export async function getUnsafeFiles(): Promise<FileSafety[]> {
  return invoke("get_unsafe_files");
}

export async function getWasteCandidates(
  threshold?: number
): Promise<WasteCandidate[]> {
  return invoke("get_waste_candidates", { threshold });
}

export async function browseDirectory(path: string): Promise<DirEntry[]> {
  return invoke("browse_directory", { path });
}

export async function getFileLocations(
  hash: string
): Promise<FileLocation[]> {
  return invoke("get_file_locations", { hash });
}

export async function getDashboardStats(): Promise<DashboardStats> {
  return invoke("get_dashboard_stats");
}

export async function analyzeSdCard(
  sdMount: string,
  onEvent: (event: ImportEvent) => void
): Promise<void> {
  const channel = new Channel<ImportEvent>();
  channel.onmessage = onEvent;
  return invoke("analyze_sd_card", { sdMount, onEvent: channel });
}

export async function startImport(
  targetDeviceIds: string[],
  onEvent: (event: ImportEvent) => void
): Promise<void> {
  const channel = new Channel<ImportEvent>();
  channel.onmessage = onEvent;
  return invoke("start_import", { targetDeviceIds, onEvent: channel });
}

export async function cancelImport(): Promise<void> {
  return invoke("cancel_import");
}

export async function ejectDevice(mountPoint: string): Promise<void> {
  return invoke("eject_device", { mountPoint });
}

export async function createProject(
  title: string,
  description: string,
  startDate: string,
  endDate: string
): Promise<Project> {
  return invoke("create_project", { title, description, startDate, endDate });
}

export async function getProjects(): Promise<Project[]> {
  return invoke("get_projects");
}

export async function getProjectDetail(id: number): Promise<ProjectDetail> {
  return invoke("get_project_detail", { id });
}

export async function updateProject(
  id: number,
  title: string,
  description: string,
  startDate: string,
  endDate: string
): Promise<Project> {
  return invoke("update_project", { id, title, description, startDate, endDate });
}

export async function deleteProject(id: number): Promise<void> {
  return invoke("delete_project", { id });
}

export async function addLocation(
  path: string,
  label: string,
  deviceType: string
): Promise<StorageDevice> {
  return invoke("add_location", { path, label, deviceType });
}

export async function addNetworkDrive(
  protocol: string,
  host: string,
  sharePath: string,
  username: string,
  password: string,
  label: string,
  deviceType: string
): Promise<NetworkDrive> {
  return invoke("add_network_drive", { protocol, host, sharePath, username, password, label, deviceType });
}

export async function getNetworkDrives(): Promise<NetworkDrive[]> {
  return invoke("get_network_drives");
}

export async function mountNetworkDrive(driveId: string): Promise<void> {
  return invoke("mount_network_drive", { driveId });
}

export async function unmountNetworkDrive(driveId: string): Promise<void> {
  return invoke("unmount_network_drive", { driveId });
}

export async function removeNetworkDrive(driveId: string): Promise<void> {
  return invoke("remove_network_drive", { driveId });
}

export async function resolveFilePath(
  deviceId: string,
  filePath: string
): Promise<string> {
  return invoke("resolve_file_path", { deviceId, filePath });
}

export async function getThumbnail(
  path: string,
  maxSize?: number
): Promise<string> {
  return invoke("get_thumbnail", { path, maxSize });
}

export async function openFile(
  deviceId: string,
  filePath: string
): Promise<void> {
  return invoke("open_file", { deviceId, filePath });
}
