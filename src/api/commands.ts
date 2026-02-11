import { invoke, Channel } from "@tauri-apps/api/core";
import type {
  StorageDevice,
  FileLocation,
  FileSafety,
  WasteCandidate,
  DashboardStats,
  DirEntry,
  ScanEvent,
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
