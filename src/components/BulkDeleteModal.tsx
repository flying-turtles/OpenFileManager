import { useState } from "react";
import { useFocusTrap } from "../hooks/useFocusTrap";
import type { FileLocation, BulkDeleteResult } from "../types";

interface Props {
  deviceName: string;
  files: FileLocation[];
  onConfirm: (locationIds: number[]) => Promise<BulkDeleteResult>;
  onClose: () => void;
}

export function BulkDeleteModal({ deviceName, files, onConfirm, onClose }: Props) {
  const trapRef = useFocusTrap<HTMLDivElement>();
  const [confirmText, setConfirmText] = useState("");
  const [deleting, setDeleting] = useState(false);
  const [result, setResult] = useState<BulkDeleteResult | null>(null);

  const canDelete = confirmText === deviceName;

  const handleDelete = async () => {
    setDeleting(true);
    try {
      const res = await onConfirm(files.map((f) => f.id));
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
                disabled={deleting}
                autoFocus
              />
            </div>
            <div className="form-actions">
              <button onClick={onClose} disabled={deleting}>Cancel</button>
              <button className="btn-danger" onClick={handleDelete} disabled={!canDelete || deleting}>
                {deleting ? "Deleting..." : `Delete All (${files.length})`}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
