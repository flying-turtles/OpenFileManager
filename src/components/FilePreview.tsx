import { useState, useEffect } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { resolveFilePath, getThumbnail } from "../api/commands";
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

type PreviewType = "standard" | "raw" | "video" | "heic" | "none";

function classifyFile(fileName: string): PreviewType {
  const ext = fileName.split(".").pop()?.toLowerCase() ?? "";
  if (STANDARD_IMAGE_EXTS.has(ext)) return "standard";
  if (HEIC_EXTS.has(ext)) return "heic";
  if (RAW_EXTS.has(ext)) return "raw";
  if (VIDEO_EXTS.has(ext)) return "video";
  return "none";
}

// Try to resolve a file path, trying the preferred device first,
// then falling back through other locations until one succeeds.
async function resolveFromLocations(
  locations: FileLocation[],
  preferredDeviceId?: string,
): Promise<string> {
  // Sort: preferred device first, then the rest in order
  const sorted = preferredDeviceId
    ? [
        ...locations.filter((l) => l.deviceId === preferredDeviceId),
        ...locations.filter((l) => l.deviceId !== preferredDeviceId),
      ]
    : locations;

  let lastError = "";
  for (const loc of sorted) {
    try {
      return await resolveFilePath(loc.deviceId, loc.filePath);
    } catch (e) {
      lastError = String(e);
    }
  }
  throw new Error(lastError || "No locations available");
}

interface Props {
  locations: FileLocation[];
  fileName: string;
  preferredDeviceId?: string;
}

export function FilePreview({ locations, fileName, preferredDeviceId }: Props) {
  const [src, setSrc] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const previewType = classifyFile(fileName);

  useEffect(() => {
    if (previewType === "none" || locations.length === 0) {
      setLoading(false);
      return;
    }

    let cancelled = false;
    setLoading(true);
    setError(null);
    setSrc(null);

    (async () => {
      try {
        const absPath = await resolveFromLocations(locations, preferredDeviceId);
        if (cancelled) return;

        if (previewType === "standard" || previewType === "heic") {
          setSrc(convertFileSrc(absPath));
        } else {
          const thumbPath = await getThumbnail(absPath);
          if (cancelled) return;
          setSrc(convertFileSrc(thumbPath));
        }
      } catch (e) {
        if (!cancelled) setError(String(e));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();

    return () => { cancelled = true; };
  }, [locations, preferredDeviceId, previewType]);

  if (previewType === "none") return null;

  return (
    <div className="file-preview">
      {loading && <div className="preview-loading">Loading preview...</div>}
      {error && <div className="preview-error">Preview unavailable</div>}
      {src && !error && (
        <div className="preview-container">
          <img
            className="preview-img"
            src={src}
            alt={fileName}
            onError={() => {
              if (previewType === "heic" && !error) {
                setError(null);
                setLoading(true);
                resolveFromLocations(locations, preferredDeviceId)
                  .then((absPath) => getThumbnail(absPath))
                  .then((thumbPath) => {
                    setSrc(convertFileSrc(thumbPath));
                    setLoading(false);
                  })
                  .catch((e) => {
                    setError(String(e));
                    setLoading(false);
                  });
              }
            }}
          />
          {previewType === "video" && <span className="preview-badge">Video</span>}
        </div>
      )}
    </div>
  );
}
