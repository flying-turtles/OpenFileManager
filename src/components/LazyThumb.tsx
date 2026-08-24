import { useEffect, useRef, useState } from "react";
import { FilePreview } from "./FilePreview";
import type { FileLocation } from "../types";

interface Props {
  locations: FileLocation[];
  fileName: string;
  preferredDeviceId?: string;
  /** Forwarded to the preview: makes the thumbnail open the larger view. */
  onClick?: () => void;
}

/**
 * Mounts a `<FilePreview>` only once the row is on screen.
 *
 * A diff can list thousands of files, and every mounted preview kicks off a
 * thumbnail generation for RAW and video. Mounting them all makes the page
 * unusable, so the work waits until the user actually scrolls there.
 */
export function LazyThumb({ locations, fileName, preferredDeviceId, onClick }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    if (visible) return;
    const node = ref.current;
    if (!node) return;

    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          setVisible(true);
          observer.disconnect();
        }
      },
      // Start a little early so a thumbnail is usually ready by the time the
      // row reaches the viewport.
      { rootMargin: "200px" }
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, [visible]);

  return (
    <div className="lazy-thumb" ref={ref}>
      {visible && (
        <FilePreview
          locations={locations}
          fileName={fileName}
          preferredDeviceId={preferredDeviceId}
          onClick={onClick}
        />
      )}
    </div>
  );
}
