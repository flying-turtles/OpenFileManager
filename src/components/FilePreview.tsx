import { useState, useEffect } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { getThumbnail } from "../api/commands";
import { classifyFile, resolveFromLocations } from "../utils/preview";
import type { FileLocation } from "../types";

interface Props {
  locations: FileLocation[];
  fileName: string;
  preferredDeviceId?: string;
  /** When set, the thumbnail becomes a button that opens the larger view. */
  onClick?: () => void;
}

export function FilePreview({ locations, fileName, preferredDeviceId, onClick }: Props) {
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

  // Only an image that actually rendered is worth clicking — a failed or
  // still-loading preview would open a larger view of the same failure.
  const clickable = onClick !== undefined && src !== null && !error;

  return (
    <div className="file-preview">
      {loading && <div className="preview-loading">Loading preview...</div>}
      {error && <div className="preview-error">Preview unavailable</div>}
      {src && !error && (
        <div
          className={`preview-container${clickable ? " preview-clickable" : ""}`}
          role={clickable ? "button" : undefined}
          tabIndex={clickable ? 0 : undefined}
          title={clickable ? "Click to enlarge" : undefined}
          onClick={
            clickable
              ? (e) => {
                  // The diff list wraps each thumbnail in a <label> for its
                  // row checkbox, so a bare click would enlarge the image
                  // AND flip the selection. Enlarging is the only intent.
                  e.preventDefault();
                  e.stopPropagation();
                  onClick!();
                }
              : undefined
          }
          onKeyDown={
            clickable
              ? (e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    e.stopPropagation();
                    onClick!();
                  }
                }
              : undefined
          }
        >
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
