import { useState } from "react";
import { useFocusTrap } from "../hooks/useFocusTrap";
import type { FileLocation, BulkDeleteResult, BulkDeleteEvent } from "../types";

interface Props {
  deviceName: string;
  files: FileLocation[];
  onConfirm: (locationIds: number[], onEvent: (event: BulkDeleteEvent) => void) => Promise<BulkDeleteResult>;
  onClose: () => void;
}

export function BulkDeleteModal({ deviceName, files, onConfirm, onClose }: Props) {
  const trapRef = useFocusTrap<HTMLDivElement>();
  const [confirmText, setConfirmText] = useState("");
  const [deleting, setDeleting] = useState(false);
  const [result, setResult] = useState<BulkDeleteResult | null>(null);
  const [processed, setProcessed] = useState(0);
  const [currentFile, setCurrentFile] = useState("");

  const canDelete = confirmText === deviceName;
  const total = files.length;
  const pct = total > 0 ? Math.round((processed / total) * 100) : 0;

  const handleDelete = async () => {
    setDeleting(true);
    setProcessed(0);
    setCurrentFile("");
    try {
      const res = await onConfirm(files.map((f) => f.id), (event) => {
        if ("Progress" in event) {
          setProcessed(event.Progress.processed);
          setCurrentFile(event.Progress.currentFile);
        }
      });
      setResult(res);
    } catch (e: any) {
      setResult({ succeeded: [], failed: [{ locationId: 0, filePath: "", error: e.toString() }] });
    } finally {
      setDeleting(false);
    }
  };

  return (
    <div className="modal-overlay" onClick={() => !deleting && !result && onClose()} role="dialog" aria-label="Bulk delete" aria-modal="true">
      <div className="modal-content" ref={trapRef} onClick={(e) => e.stopPropagation()} style={{ maxWidth: 560 }}>
        {result ? (
          <>
            <h2>Bulk Delete Complete</h2>
            <p>{result.succeeded.length} file{result.succeeded.length !== 1 ? "s" : ""} deleted.</p>
            {result.failed.length > 0 && (
              <>
                <p className="bulk-delete-warning">{result.failed.length} failed:</p>
                <div className="bulk-delete-file-list">
                  {result.failed.map((f, i) => (
                    <div key={i} className="bulk-delete-file-item">
                      <span className="bulk-delete-file-path">{f.filePath}</span>
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
        ) : (
          <>
            <h2>Bulk Delete from {deviceName}</h2>
            <p className="bulk-delete-warning">
              This will permanently delete {files.length} file{files.length !== 1 ? "s" : ""} from "{deviceName}". This cannot be undone.
            </p>
            <div className="bulk-delete-file-list">
              {files.map((f) => (
                <div key={f.id} className="bulk-delete-file-item">
                  <span className="bulk-delete-file-path">{f.filePath}</span>
                </div>
              ))}
            </div>
            <div className="form-group" style={{ marginTop: 16 }}>
              <label>Type "<strong>{deviceName}</strong>" to confirm</label>
              <input
                type="text"
                value={confirmText}
                onChange={(e) => setConfirmText(e.target.value)}
                placeholder={deviceName}
                autoFocus
              />
            </div>
            <div className="form-actions">
              <button onClick={onClose}>Cancel</button>
              <button className="btn-danger" onClick={handleDelete} disabled={!canDelete}>
                Delete All ({files.length})
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
