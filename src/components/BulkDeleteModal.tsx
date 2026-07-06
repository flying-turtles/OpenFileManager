import { useState, useMemo } from "react";
import { useFocusTrap } from "../hooks/useFocusTrap";
import { PermanentToggle } from "./FileTable";
import type { FileLocation, BulkDeleteResult, BulkDeleteEvent } from "../types";

interface Props {
  deviceName: string;
  files: FileLocation[];
  onConfirm: (locationIds: number[], onEvent: (event: BulkDeleteEvent) => void, permanent?: boolean) => Promise<BulkDeleteResult>;
  onClose: () => void;
}

type Mode = "automatic" | "manual";
type KeepStrategy = "shortest" | "longest";

export function BulkDeleteModal({ deviceName, files, onConfirm, onClose }: Props) {
  const trapRef = useFocusTrap<HTMLDivElement>();
  const [confirmText, setConfirmText] = useState("");
  const [deleting, setDeleting] = useState(false);
  const [result, setResult] = useState<BulkDeleteResult | null>(null);
  const [processed, setProcessed] = useState(0);
  const [currentFile, setCurrentFile] = useState("");
  const [mode, setMode] = useState<Mode>("automatic");
  const [permanent, setPermanent] = useState(false);
  const [keepStrategy, setKeepStrategy] = useState<KeepStrategy>("shortest");
  // For manual mode: set of location IDs marked for deletion
  const [markedForDeletion, setMarkedForDeletion] = useState<Set<number>>(new Set());

  // Group files by hash to find same-drive duplicates
  const { sameDriveGroups, uniqueFiles } = useMemo(() => {
    const byHash = new Map<string, FileLocation[]>();
    for (const f of files) {
      const group = byHash.get(f.blake3Hash) ?? [];
      group.push(f);
      byHash.set(f.blake3Hash, group);
    }
    const sameDriveGroups: FileLocation[][] = [];
    const uniqueFiles: FileLocation[] = [];
    for (const group of byHash.values()) {
      if (group.length > 1) {
        sameDriveGroups.push(group);
      } else {
        uniqueFiles.push(group[0]);
      }
    }
    return { sameDriveGroups, uniqueFiles };
  }, [files]);

  const hasSameDriveDuplicates = sameDriveGroups.length > 0;

  // Initialize manual selection: for each same-drive group, mark all but first for deletion
  const initManualSelection = () => {
    const initial = new Set<number>();
    for (const group of sameDriveGroups) {
      // Mark all copies except the first for deletion
      for (let i = 1; i < group.length; i++) {
        initial.add(group[i].id);
      }
    }
    // All unique files are always marked for deletion
    for (const f of uniqueFiles) {
      initial.add(f.id);
    }
    setMarkedForDeletion(initial);
  };

  const handleModeChange = (newMode: Mode) => {
    setMode(newMode);
    if (newMode === "manual") {
      initManualSelection();
    }
  };

  // Toggle a file within a same-drive group: selecting one deselects the others
  const toggleInGroup = (group: FileLocation[], targetId: number) => {
    setMarkedForDeletion((prev) => {
      const next = new Set(prev);
      const isCurrentlyMarked = next.has(targetId);
      if (isCurrentlyMarked) {
        // Deselecting this one for deletion (keeping it) — mark all others in group for deletion
        next.delete(targetId);
        for (const f of group) {
          if (f.id !== targetId) next.add(f.id);
        }
      } else {
        // Selecting this one for deletion — deselect all others in group
        next.add(targetId);
        for (const f of group) {
          if (f.id !== targetId) next.delete(f.id);
        }
      }
      return next;
    });
  };

  // In automatic mode, for same-drive groups keep the file with shortest/longest path
  const autoDeleteIds = useMemo(() => {
    if (!hasSameDriveDuplicates) return new Set<number>();
    const ids = new Set<number>();
    for (const group of sameDriveGroups) {
      const sorted = [...group].sort((a, b) => a.filePath.length - b.filePath.length);
      const keepIndex = keepStrategy === "shortest" ? 0 : sorted.length - 1;
      for (let i = 0; i < sorted.length; i++) {
        if (i !== keepIndex) ids.add(sorted[i].id);
      }
    }
    // Unique files are always deleted
    for (const f of uniqueFiles) ids.add(f.id);
    return ids;
  }, [sameDriveGroups, uniqueFiles, keepStrategy, hasSameDriveDuplicates]);

  const filesToDelete = mode === "manual"
    ? files.filter((f) => markedForDeletion.has(f.id))
    : hasSameDriveDuplicates
      ? files.filter((f) => autoDeleteIds.has(f.id))
      : files;

  const canDelete = confirmText === deviceName && filesToDelete.length > 0;
  const total = filesToDelete.length;
  const pct = total > 0 ? Math.round((processed / total) * 100) : 0;

  const handleDelete = async () => {
    setDeleting(true);
    setProcessed(0);
    setCurrentFile("");
    try {
      const res = await onConfirm(filesToDelete.map((f) => f.id), (event) => {
        if ("Progress" in event) {
          setProcessed(event.Progress.processed);
          setCurrentFile(event.Progress.currentFile);
        }
      }, permanent);
      setResult(res);
    } catch (e: any) {
      setResult({ succeeded: [], failed: [{ locationId: 0, filePath: "", error: e.toString() }] });
    } finally {
      setDeleting(false);
    }
  };

  return (
    <div className="modal-overlay" onClick={() => !deleting && !result && onClose()} role="dialog" aria-label="Bulk delete" aria-modal="true" onKeyDown={(e) => { if (e.key === "Escape" && !deleting) onClose(); }}>
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
            {hasSameDriveDuplicates && (
              <div className="bulk-delete-mode-toggle">
                <span className="text-xs text-muted-color">{sameDriveGroups.length} file{sameDriveGroups.length !== 1 ? "s have" : " has"} multiple copies on this drive</span>
                <div className="filter-toggle" style={{ marginTop: 8 }}>
                  <button className={mode === "automatic" ? "active" : ""} onClick={() => handleModeChange("automatic")}>
                    Automatic
                  </button>
                  <button className={mode === "manual" ? "active" : ""} onClick={() => handleModeChange("manual")}>
                    Manual Selection
                  </button>
                </div>
              </div>
            )}
            {mode === "automatic" ? (
              <>
                {hasSameDriveDuplicates && (
                  <div className="form-group" style={{ marginTop: 4, marginBottom: 12 }}>
                    <label>Keep file in</label>
                    <select value={keepStrategy} onChange={(e) => setKeepStrategy(e.target.value as KeepStrategy)}>
                      <option value="shortest">Shortest path</option>
                      <option value="longest">Longest path</option>
                    </select>
                  </div>
                )}
                <p className="bulk-delete-warning">
                  {filesToDelete.length} file{filesToDelete.length !== 1 ? "s" : ""} from "{deviceName}" will be moved to the Trash.
                </p>
                <div className="bulk-delete-file-list">
                  {hasSameDriveDuplicates ? (
                    <>
                      {sameDriveGroups.map((group) => (
                        <div key={group[0].blake3Hash} className="bulk-delete-group">
                          <span className="bulk-delete-group-label">{group[0].fileName}</span>
                          {group.map((f) => (
                            <div key={f.id} className={`bulk-delete-select-item ${autoDeleteIds.has(f.id) ? "marked" : "kept"}`}>
                              <span className="bulk-delete-file-path">{f.filePath}</span>
                              <span className={`bulk-delete-badge ${autoDeleteIds.has(f.id) ? "badge-delete" : "badge-keep"}`}>
                                {autoDeleteIds.has(f.id) ? "Delete" : "Keep"}
                              </span>
                            </div>
                          ))}
                        </div>
                      ))}
                      {uniqueFiles.length > 0 && (
                        <>
                          <div className="bulk-delete-divider" />
                          {uniqueFiles.map((f) => (
                            <div key={f.id} className="bulk-delete-file-item">
                              <span className="bulk-delete-file-path">{f.filePath}</span>
                            </div>
                          ))}
                        </>
                      )}
                    </>
                  ) : (
                    files.map((f) => (
                      <div key={f.id} className="bulk-delete-file-item">
                        <span className="bulk-delete-file-path">{f.filePath}</span>
                      </div>
                    ))
                  )}
                </div>
              </>
            ) : (
              <>
                <p className="bulk-delete-warning">
                  {filesToDelete.length} file{filesToDelete.length !== 1 ? "s" : ""} selected — they will be moved to the Trash.
                </p>
                <div className="bulk-delete-file-list">
                  {sameDriveGroups.map((group) => (
                    <div key={group[0].blake3Hash} className="bulk-delete-group">
                      <span className="bulk-delete-group-label">{group[0].fileName}</span>
                      {group.map((f) => (
                        <label key={f.id} className={`bulk-delete-select-item ${markedForDeletion.has(f.id) ? "marked" : "kept"}`}>
                          <input
                            type="checkbox"
                            checked={markedForDeletion.has(f.id)}
                            onChange={() => toggleInGroup(group, f.id)}
                          />
                          <span className="bulk-delete-file-path">{f.filePath}</span>
                          <span className={`bulk-delete-badge ${markedForDeletion.has(f.id) ? "badge-delete" : "badge-keep"}`}>
                            {markedForDeletion.has(f.id) ? "Delete" : "Keep"}
                          </span>
                        </label>
                      ))}
                    </div>
                  ))}
                  {uniqueFiles.length > 0 && (
                    <>
                      {sameDriveGroups.length > 0 && <div className="bulk-delete-divider" />}
                      {uniqueFiles.map((f) => (
                        <div key={f.id} className="bulk-delete-file-item">
                          <span className="bulk-delete-file-path">{f.filePath}</span>
                        </div>
                      ))}
                    </>
                  )}
                </div>
              </>
            )}
            <PermanentToggle permanent={permanent} onChange={setPermanent} disabled={deleting} />
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
                Delete ({filesToDelete.length})
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
