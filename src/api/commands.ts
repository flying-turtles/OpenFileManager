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
  mode: string,
  onEvent: (event: ScanEvent) => void
): Promise<void> {
  const channel = new Channel<ScanEvent>();
  channel.onmessage = onEvent;
  return invoke("start_scan", { target, mode, onEvent: channel });
}

export async function cancelScan(): Promise<void> {
  return invoke("cancel_scan");
}

export async function getFilesOnDevice(
  deviceId: string
): Promise<FileLocation[]> {
  return invoke("get_files_on_device", { deviceId });
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
