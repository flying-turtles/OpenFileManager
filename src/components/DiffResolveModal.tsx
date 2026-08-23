import { useState } from "react";
import { useFocusTrap } from "../hooks/useFocusTrap";
import { PermanentToggle } from "./FileTable";
import { LazyThumb } from "./LazyThumb";
import { formatBytes } from "../utils/format";
import type { FileLocation } from "../types";
import type { ResolveResult } from "../hooks/useProjectDiff";

/** One file the user is about to delete, with a location that can be previewed. */
export interface DeletePreviewItem {
  key: string;
  fileName: string;
  fileSize: number;
  deviceLabel: string;
  location: FileLocation;
}

interface Props {
  sotLabel: string;
  deleteItems: DeletePreviewItem[];
  copyCount: number;
  copyBytes: number;
  purgeCount: number;
  busy: boolean;
  progressText: string;
  result: ResolveResult | null;
  onConfirm: (permanent: boolean) => void;
  onClose: () => void;
}

export function DiffResolveModal({
  sotLabel,
  deleteItems,
  copyCount,
  copyBytes,
  purgeCount,
  busy,
  progressText,
  result,
  onConfirm,
  onClose,
}: Props) {
  const trapRef = useFocusTrap<HTMLDivElement>();
  const [permanent, setPermanent] = useState(false);
  const [confirmText, setConfirmText] = useState("");

  const deleteBytes = deleteItems.reduce((sum, i) => sum + i.fileSize, 0);
  // A move to the Trash is recoverable and needs no extra friction; a
  // permanent delete follows SourceCleanupModal and asks for the label.
  const confirmed = !permanent || confirmText === sotLabel;
  const canResolve = confirmed && (deleteItems.length > 0 || copyCount > 0);

  return (
    <div
      className="modal-overlay"
      onClick={() => !busy && !result && onClose()}
      role="dialog"
      aria-label="Resolve diff"
      aria-modal="true"
      onKeyDown={(e) => {
        if (e.key === "Escape" && !busy) onClose();
      }}
    >
      <div
        className="modal-content diff-resolve-modal"
        ref={trapRef}
        onClick={(e) => e.stopPropagation()}
      >
        {result ? (
          <>
            <h2>Diff resolved</h2>
            <p>
              {result.deleted?.succeeded.length ?? 0} deleted, {result.copied?.copied ?? 0}{" "}
              copied, {result.purged} index row{result.purged !== 1 ? "s" : ""} cleaned up.
            </p>
            {(result.copied?.skipped.length ?? 0) > 0 && (
              <>
                <p className="bulk-delete-warning">
                  {result.copied!.skipped.length} copy target
                  {result.copied!.skipped.length !== 1 ? "s" : ""} already held a file and
                  were left alone:
                </p>
                <div className="bulk-delete-file-list">
                  {result.copied!.skipped.map((s, i) => (
                    <div key={i} className="bulk-delete-file-item">
                      <span className="bulk-delete-file-path">{s.fileName}</span>
                      <span className="text-muted-color text-xs">{s.error}</span>
                    </div>
                  ))}
                </div>
              </>
            )}
            {((result.deleted?.failed.length ?? 0) > 0 ||
              (result.copied?.failed.length ?? 0) > 0) && (
              <>
                <p className="bulk-delete-warning">Failures:</p>
                <div className="bulk-delete-file-list">
                  {result.deleted?.failed.map((f, i) => (
                    <div key={`d${i}`} className="bulk-delete-file-item">
                      <span className="bulk-delete-file-path">{f.filePath}</span>
                      <span className="text-muted-color text-xs">{f.error}</span>
                    </div>
                  ))}
                  {result.copied?.failed.map((f, i) => (
                    <div key={`c${i}`} className="bulk-delete-file-item">
                      <span className="bulk-delete-file-path">{f.fileName}</span>
                      <span className="text-muted-color text-xs">{f.error}</span>
                    </div>
                  ))}
                </div>
              </>
            )}
            <div className="form-actions">
              <button className="btn-primary" onClick={onClose}>
                Close
              </button>
            </div>
          </>
        ) : busy ? (
          <>
            <h2>Resolving...</h2>
            <p className="text-muted-color">{progressText || "Working..."}</p>
          </>
        ) : (
          <>
            <h2>Resolve diff</h2>
            <p>
              {deleteItems.length} file{deleteItems.length !== 1 ? "s" : ""} (
              {formatBytes(deleteBytes)}) will be removed from the backups, and {copyCount}{" "}
              file{copyCount !== 1 ? "s" : ""} ({formatBytes(copyBytes)}) copied to them.
            </p>
            {purgeCount > 0 && (
              <p className="text-muted-color text-xs">
                {purgeCount} index row{purgeCount !== 1 ? "s" : ""} for files already gone
                from {sotLabel} will be cleaned up, including for files you deselected.
              </p>
            )}

            {deleteItems.length > 0 && (
              <>
                <p className="bulk-delete-warning">About to be deleted:</p>
                <div className="diff-resolve-grid">
                  {deleteItems.map((item) => (
                    <div key={item.key} className="diff-resolve-tile">
                      <LazyThumb
                        locations={[item.location]}
                        fileName={item.fileName}
                        preferredDeviceId={item.location.deviceId}
                      />
                      <span className="diff-resolve-tile-name" title={item.fileName}>
                        {item.fileName}
                      </span>
                      <span className="diff-resolve-tile-device">{item.deviceLabel}</span>
                    </div>
                  ))}
                </div>
              </>
            )}

            <PermanentToggle permanent={permanent} onChange={setPermanent} />
            {permanent && (
              <div className="form-group" style={{ marginTop: 12 }}>
                <label>
                  Type "<strong>{sotLabel}</strong>" to confirm
                </label>
                <input
                  type="text"
                  value={confirmText}
                  onChange={(e) => setConfirmText(e.target.value)}
                  placeholder={sotLabel}
                  autoFocus
                />
              </div>
            )}

            <div className="form-actions">
              <button onClick={onClose}>Cancel</button>
              <button
                className="btn-danger"
                onClick={() => onConfirm(permanent)}
                disabled={!canResolve}
              >
                Resolve
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
