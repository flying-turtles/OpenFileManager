import { useState, useEffect, useCallback } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { getThumbnail, openFile } from "../api/commands";
import { useFocusTrap } from "../hooks/useFocusTrap";
import { classifyFile, resolveFromLocations, orderedLocations } from "../utils/preview";
import type { FileLocation } from "../types";
import "./PreviewLightbox.css";

/** One file the lightbox can show. Mirrors what `FilePreview` already takes. */
export interface LightboxItem {
  locations: FileLocation[];
  fileName: string;
  preferredDeviceId?: string;
  /** Shown in the chrome, e.g. the device the copy lives on. */
  caption?: string;
}

interface Props {
  items: LightboxItem[];
  index: number;
  onIndexChange: (index: number) => void;
  onClose: () => void;
}

/** Bigger than the 512 default so a full-screen RAW render is not soft. */
const LARGE_THUMB_SIZE = 1600;

export function PreviewLightbox({ items, index, onIndexChange, onClose }: Props) {
  const trapRef = useFocusTrap<HTMLDivElement>();
  const [src, setSrc] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [opening, setOpening] = useState(false);
  const [openError, setOpenError] = useState("");

  const item = items[index];
  const previewType = item ? classifyFile(item.fileName) : "none";
  const hasSiblings = items.length > 1;

  const step = useCallback(
    (delta: number) => {
      if (items.length === 0) return;
      // Wrap, so arrowing off either end continues rather than dead-ending.
      onIndexChange((index + delta + items.length) % items.length);
    },
    [index, items.length, onIndexChange]
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
      } else if (e.key === "ArrowLeft" && hasSiblings) {
        e.preventDefault();
        step(-1);
      } else if (e.key === "ArrowRight" && hasSiblings) {
        e.preventDefault();
        step(1);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [step, onClose, hasSiblings]);

  useEffect(() => {
    if (!item || previewType === "none" || item.locations.length === 0) {
      setLoading(false);
      setSrc(null);
      return;
    }

    let cancelled = false;
    setLoading(true);
    setError(null);
    setSrc(null);
    setOpenError("");

    (async () => {
      try {
        const absPath = await resolveFromLocations(item.locations, item.preferredDeviceId);
        if (cancelled) return;

        if (previewType === "standard" || previewType === "heic") {
          // The original, not a thumbnail — this is the whole point of the
          // larger view.
          setSrc(convertFileSrc(absPath));
        } else {
          const thumbPath = await getThumbnail(absPath, LARGE_THUMB_SIZE);
          if (cancelled) return;
          setSrc(convertFileSrc(thumbPath));
        }
      } catch (e) {
        if (!cancelled) setError(String(e));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [item, previewType]);

  const handleOpenExternally = useCallback(async () => {
    if (!item) return;
    setOpening(true);
    setOpenError("");
    // Same fallback the preview uses: the preferred copy first, then any
    // other, so a file still opens when only one of its drives is mounted.
    let lastError = "";
    for (const loc of orderedLocations(item.locations, item.preferredDeviceId)) {
      try {
        await openFile(loc.deviceId, loc.filePath);
        setOpening(false);
        return;
      } catch (e) {
        lastError = String(e);
      }
    }
    setOpenError(lastError || "No copy could be opened");
    setOpening(false);
  }, [item]);

  if (!item) return null;

  return (
    <div
      className="lightbox-overlay"
      role="dialog"
      aria-modal="true"
      aria-label={`Preview of ${item.fileName}`}
      onClick={onClose}
    >
      <div className="lightbox-frame" ref={trapRef} onClick={(e) => e.stopPropagation()}>
        <div className="lightbox-chrome">
          <div className="lightbox-title">
            <strong title={item.fileName}>{item.fileName}</strong>
            <span className="text-muted-color text-xs">
              {hasSiblings && `${index + 1} of ${items.length}`}
              {hasSiblings && item.caption ? " — " : ""}
              {item.caption}
            </span>
          </div>
          <div className="lightbox-actions">
            <button onClick={handleOpenExternally} disabled={opening || item.locations.length === 0}>
              {opening ? "Opening..." : "Open in default app"}
            </button>
            <button onClick={onClose} aria-label="Close preview">
              Close
            </button>
          </div>
        </div>

        {openError && <div className="error-msg">{openError}</div>}

        <div className="lightbox-stage">
          {hasSiblings && (
            <button
              className="lightbox-arrow lightbox-arrow-prev"
              onClick={() => step(-1)}
              aria-label="Previous image"
            >
              ‹
            </button>
          )}

          {loading && <div className="preview-loading">Loading preview...</div>}
          {error && <div className="preview-error">Preview unavailable</div>}
          {previewType === "none" && !loading && (
            <div className="preview-error">No preview for this file type</div>
          )}
          {src && !error && (
            <img className="lightbox-img" src={src} alt={item.fileName} />
          )}

          {hasSiblings && (
            <button
              className="lightbox-arrow lightbox-arrow-next"
              onClick={() => step(1)}
              aria-label="Next image"
            >
              ›
            </button>
          )}
        </div>

        {previewType === "video" && (
          <div className="lightbox-note text-muted-color text-xs">
            Showing the poster frame. Use Open in default app to play it.
          </div>
        )}
      </div>
    </div>
  );
}
