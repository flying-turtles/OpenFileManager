import { resolveFilePath } from "../api/commands";
import type { FileLocation } from "../types";

const STANDARD_IMAGE_EXTS = new Set([
  "jpg", "jpeg", "png", "webp", "gif", "tiff", "tif", "bmp", "avif",
]);
const RAW_EXTS = new Set([
  "cr2", "cr3", "arw", "nef", "dng", "orf", "raf", "rw2", "pef", "srw", "3fr", "iiq",
]);
const VIDEO_EXTS = new Set([
  "mp4", "mov", "avi", "mkv", "m4v", "webm", "mts",
]);
const HEIC_EXTS = new Set(["heic", "heif"]);

export type PreviewType = "standard" | "raw" | "video" | "heic" | "none";

export function classifyFile(fileName: string): PreviewType {
  const ext = fileName.split(".").pop()?.toLowerCase() ?? "";
  if (STANDARD_IMAGE_EXTS.has(ext)) return "standard";
  if (HEIC_EXTS.has(ext)) return "heic";
  if (RAW_EXTS.has(ext)) return "raw";
  if (VIDEO_EXTS.has(ext)) return "video";
  return "none";
}

/** The preferred device's copies first, then the rest in their original order. */
export function orderedLocations(
  locations: FileLocation[],
  preferredDeviceId?: string,
): FileLocation[] {
  if (!preferredDeviceId) return locations;
  return [
    ...locations.filter((l) => l.deviceId === preferredDeviceId),
    ...locations.filter((l) => l.deviceId !== preferredDeviceId),
  ];
}

/**
 * Resolve a file to an absolute path, trying the preferred device first and
 * falling back through the other locations until one succeeds.
 *
 * A copy on a disconnected drive throws rather than resolving, so falling
 * through is how a file with several copies still previews when only one of
 * its drives is plugged in.
 */
export async function resolveFromLocations(
  locations: FileLocation[],
  preferredDeviceId?: string,
): Promise<string> {
  let lastError = "";
  for (const loc of orderedLocations(locations, preferredDeviceId)) {
    try {
      return await resolveFilePath(loc.deviceId, loc.filePath);
    } catch (e) {
      lastError = String(e);
    }
  }
  throw new Error(lastError || "No locations available");
}

