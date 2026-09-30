import { useEffect, useState } from "react";
import { useFocusTrap } from "../hooks/useFocusTrap";
import { PermanentToggle } from "./FileTable";
import { getImportCleanupPreview, deleteImportedSourceFiles } from "../api/commands";
import { formatBytes } from "../utils/format";
import type { SourceCleanupPreview, SourceCleanupResult, SourceCleanupEvent } from "../types";

interface Props {
  deviceNames: Record<string, string>;
  onDeleted: () => void;
  onClose: () => void;
  /** Defaults to the import-source flow. */
  loadPreview?: () => Promise<SourceCleanupPreview>;
  runDelete?: (
    permanent: boolean,
    onEvent: (event: SourceCleanupEvent) => void
  ) => Promise<SourceCleanupResult>;
}

/**
 * Deletes files that have copies on at least two other devices — from the
 * import source device by default, or from any location the caller supplies
 * via `loadPreview`/`runDelete`. Eligibility is always computed and
 * re-checked in the backend; the list shown here is display only.
 */
export function SourceCleanupModal({
  deviceNames,
  onDeleted,
  onClose,
  loadPreview = getImportCleanupPreview,
  runDelete = deleteImportedSourceFiles,
}: Props) {
  const trapRef = useFocusTrap<HTMLDivElement>();
  const [preview, setPreview] = useState<SourceCleanupPreview | null>(null);
  const [loadError, setLoadError] = useState("");
  const [confirmText, setConfirmText] = useState("");
  const [permanent, setPermanent] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [processed, setProcessed] = useState(0);
  const [currentFile, setCurrentFile] = useState("");
  const [result, setResult] = useState<SourceCleanupResult | null>(null);

  useEffect(() => {
    loadPreview()
      .then(setPreview)
      .catch((e) => setLoadError(String(e)));
  }, [loadPreview]);

  // The list may be truncated for display, so counts come from fileCount.
  const total = preview?.fileCount ?? 0;
  const shown = preview?.files.length ?? 0;
  const pct = total > 0 ? Math.round((processed / total) * 100) : 0;
  // Typing the label is only warranted for permanent deletion. A move to the
  // Trash is recoverable, so it does not need the extra friction.
  const confirmed = !permanent || confirmText === preview?.sdLabel;
  const canDelete = preview !== null && confirmed && total > 0;

  const handleDelete = async () => {
    setDeleting(true);
    setProcessed(0);
    setCurrentFile("");
    try {
      const res = await runDelete(permanent, (event) => {
        if ("Progress" in event) {
          setProcessed(event.Progress.processed);
          setCurrentFile(event.Progress.currentFile);
        }
      });
      setResult(res);
      if (res.deleted > 0) onDeleted();
    } catch (e: any) {
      setResult({ deleted: 0, bytesFreed: 0, failed: [{ sourcePath: "", error: String(e) }] });
    } finally {
      setDeleting(false);
    }
  };

  return (
    <div
      className="modal-overlay"
      onClick={() => !deleting && !result && onClose()}
      role="dialog"
      aria-label="Delete backed-up files from source"
      aria-modal="true"
      onKeyDown={(e) => {
        if (e.key === "Escape" && !deleting) onClose();
      }}
    >
      <div className="modal-content" ref={trapRef} onClick={(e) => e.stopPropagation()} style={{ maxWidth: 560 }}>
        {result ? (
          <>
            <h2>Source Cleanup Complete</h2>
            <p>
              {result.deleted} file{result.deleted !== 1 ? "s" : ""} deleted
              {result.deleted > 0 && <> · {formatBytes(result.bytesFreed)} freed</>}
            </p>
            {result.failed.length > 0 && (
              <>
                <p className="bulk-delete-warning">{result.failed.length} failed:</p>
                <div className="bulk-delete-file-list">
                  {result.failed.map((f, i) => (
                    <div key={i} className="bulk-delete-file-item">
                      <span className="bulk-delete-file-path">{f.sourcePath}</span>
                      <span className="text-muted-color text-xs">{f.error}</span>
                    </div>
                  ))}
                </div>
              </>
            )}
            <div className="form-actions">
              <button className="btn-primary" onClick={onClose}>Close</button>
            </div>
          </>
        ) : deleting ? (
          <>
            <h2>Deleting files...</h2>
            <div className="progress-container">
              <div className="progress-bar">
                <div className="progress-fill" style={{ width: `${pct}%` }} />
              </div>
              <div className="progress-stats">
                <span>{processed} / {total} files</span>
                <span>{pct}%</span>
              </div>
              {currentFile && <div className="progress-file">{currentFile}</div>}
            </div>
          </>
        ) : loadError ? (
          <>
            <h2>Delete Backed-Up Files</h2>
            <div className="error-msg">{loadError}</div>
            <div className="form-actions">
              <button onClick={onClose}>Close</button>
            </div>
          </>
        ) : !preview ? (
          <>
            <h2>Delete Backed-Up Files</h2>
            <p>Checking backup locations...</p>
          </>
        ) : total === 0 ? (
          <>
            <h2>Delete Backed-Up Files from {preview.sdLabel}</h2>
            <p>
              No files are safe to delete yet — every file needs copies on at
              least 2 other devices first.
            </p>
            {preview.skippedCount > 0 && (
              <p className="text-muted-color">
                {preview.skippedCount} file{preview.skippedCount !== 1 ? "s have" : " has"} fewer than 2 backups.
              </p>
            )}
            <div className="form-actions">
              <button className="btn-primary" onClick={onClose}>Close</button>
            </div>
          </>
        ) : (
          <>
            <h2>Delete Backed-Up Files from {preview.sdLabel}</h2>
            <p className="bulk-delete-warning">
              {total} file{total !== 1 ? "s" : ""} ({formatBytes(preview.totalBytes)}) have copies on
              at least 2 other devices and will be {permanent ? "permanently deleted" : "moved to the Trash"}.
            </p>
            {preview.skippedCount > 0 && (
              <p className="text-muted-color text-xs">
                {preview.skippedCount} file{preview.skippedCount !== 1 ? "s" : ""} with fewer than 2 backups will be kept.
              </p>
            )}
            <div className="bulk-delete-file-list">
              {preview.files.map((f, i) => (
                <div key={i} className="bulk-delete-file-item">
                  <span className="bulk-delete-file-path">{f.relativePath}</span>
                  <span className="text-muted-color text-xs">
                    {f.backupDeviceIds.map((id) => deviceNames[id] || id).join(", ")}
                  </span>
                </div>
              ))}
              {shown < total && (
                <div className="bulk-delete-file-item text-muted-color text-xs">
                  Showing the {shown} largest of {total} files — all {total} will be deleted.
                </div>
              )}
            </div>
            <PermanentToggle permanent={permanent} onChange={setPermanent} disabled={deleting} />
            {permanent && (
              <div className="form-group" style={{ marginTop: 16 }}>
                <label>Type "<strong>{preview.sdLabel}</strong>" to confirm</label>
                <input
                  type="text"
                  value={confirmText}
                  onChange={(e) => setConfirmText(e.target.value)}
                  placeholder={preview.sdLabel}
                  autoFocus
                />
              </div>
            )}
            <div className="form-actions">
              <button onClick={onClose}>Cancel</button>
              <button className="btn-danger" onClick={handleDelete} disabled={!canDelete}>
                Delete ({total})
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
