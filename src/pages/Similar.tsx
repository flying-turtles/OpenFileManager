import { useState, useEffect, useMemo } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useSimilar } from "../hooks/useSimilar";
import { useDevices } from "../hooks/useDevices";
import { useFocusTrap } from "../hooks/useFocusTrap";
import { resolveFilePath, getThumbnail, openFile, revealInFinder } from "../api/commands";
import type { SimilarFile, SimilarGroup, BulkDeleteEvent, BulkDeleteResult } from "../types";
import { PermanentToggle } from "../components/FileTable";
import { formatBytes } from "../utils/format";
import "./Similar.css";

function SimilarThumb({ file, connectedDeviceIds }: {
  file: SimilarFile;
  connectedDeviceIds: Set<string>;
}) {
  const [src, setSrc] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let alive = true;
    (async () => {
      for (const loc of file.locations) {
        if (!connectedDeviceIds.has(loc.deviceId)) continue;
        try {
          const abs = await resolveFilePath(loc.deviceId, loc.filePath);
          const thumb = await getThumbnail(abs, 256);
          if (alive) setSrc(convertFileSrc(thumb));
          return;
        } catch {
          // try next location
        }
      }
      if (alive) setFailed(true);
    })();
    return () => {
      alive = false;
    };
  }, [file, connectedDeviceIds]);

  if (failed) return <div className="similar-thumb-placeholder">No preview</div>;
  if (!src) return <div className="similar-thumb-placeholder skeleton" />;
  return <img className="similar-thumb-img" src={src} alt={file.representativeName} loading="lazy" />;
}

function DeleteSimilarModal({ files, nothingKept, deviceNames, onConfirm, onClose }: {
  files: { file: SimilarFile; locationIds: number[]; skippedOffline: number }[];
  nothingKept: boolean;
  deviceNames?: Record<string, string>;
  onConfirm: (locationIds: number[], onEvent: (e: BulkDeleteEvent) => void, permanent?: boolean) => Promise<BulkDeleteResult>;
  onClose: () => void;
}) {
  const trapRef = useFocusTrap<HTMLDivElement>();
  const [deleting, setDeleting] = useState(false);
  const [result, setResult] = useState<BulkDeleteResult | null>(null);
  const [processed, setProcessed] = useState(0);
  const [confirmText, setConfirmText] = useState("");
  const [permanent, setPermanent] = useState(false);

  const allIds = files.flatMap((f) => f.locationIds);
  const totalBytes = files.reduce((s, f) => s + f.file.fileSize * f.locationIds.length, 0);
  const offlineLocations = files.flatMap(({ file, locationIds }) =>
    file.locations.filter((l) => !locationIds.includes(l.id))
  );
  const canDelete = !nothingKept || confirmText === "delete";

  const handleDelete = async () => {
    setDeleting(true);
    try {
      const res = await onConfirm(allIds, (event) => {
        if ("Progress" in event) setProcessed(event.Progress.processed);
      }, permanent);
      setResult(res);
    } catch (e: any) {
      setResult({ succeeded: [], failed: [{ locationId: 0, filePath: "", error: String(e) }] });
    } finally {
      setDeleting(false);
    }
  };

  return (
    <div
      className="modal-overlay"
      onClick={() => !deleting && !result && onClose()}
      role="dialog"
      aria-label="Delete similar pictures"
      aria-modal="true"
      onKeyDown={(e) => { if (e.key === "Escape" && !deleting) onClose(); }}
    >
      <div className="modal-content" ref={trapRef} onClick={(e) => e.stopPropagation()} style={{ maxWidth: 560 }}>
        {result ? (
          <>
            <h2>Deleted</h2>
            <p>{result.succeeded.length} file cop{result.succeeded.length !== 1 ? "ies" : "y"} deleted.</p>
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
            <h2>Deleting...</h2>
            <div className="progress-container">
              <div className="progress-bar">
                <div
                  className="progress-fill"
                  style={{ width: `${allIds.length ? (processed / allIds.length) * 100 : 0}%` }}
                />
              </div>
              <div className="progress-stats">
                <span>{processed} / {allIds.length}</span>
              </div>
            </div>
          </>
        ) : (
          <>
            <h2>{nothingKept ? "Delete ALL copies?" : "Delete similar pictures?"}</h2>
            <p className="bulk-delete-warning">
              {allIds.length} file cop{allIds.length !== 1 ? "ies" : "y"} (
              {formatBytes(totalBytes)}) will be {permanent ? "permanently deleted" : "moved to the Trash"}.
            </p>
            {nothingKept && (
              <p className="bulk-delete-warning">
                Nothing is marked as Keep — every copy of these files on connected drives will be
                deleted.
              </p>
            )}
            <div className="bulk-delete-file-list">
              {files.map(({ file, locationIds }) =>
                file.locations
                  .filter((l) => locationIds.includes(l.id))
                  .map((l) => (
                    <div key={l.id} className="bulk-delete-file-item">
                      <span className="bulk-delete-file-path">
                        [{deviceNames?.[l.deviceId] ?? l.deviceId.slice(0, 8)}] {l.filePath}
                      </span>
                      <span className="text-muted-color text-xs">{formatBytes(file.fileSize)}</span>
                    </div>
                  ))
              )}
            </div>
            {offlineLocations.length > 0 && (
              <>
                <p className="text-muted-color text-sm" style={{ marginTop: 12, marginBottom: 4 }}>
                  Kept — drive not connected:
                </p>
                <div className="bulk-delete-file-list offline-kept-list">
                  {offlineLocations.map((l) => (
                    <div key={l.id} className="bulk-delete-file-item">
                      <span className="bulk-delete-file-path">
                        [{deviceNames?.[l.deviceId] ?? l.deviceId.slice(0, 8)}] {l.filePath}
                      </span>
                      <span className="text-muted-color text-xs">offline</span>
                    </div>
                  ))}
                </div>
              </>
            )}
            <PermanentToggle permanent={permanent} onChange={setPermanent} disabled={deleting} />
            {nothingKept && (
              <div className="form-group" style={{ marginTop: 16 }}>
                <label>Type "<strong>delete</strong>" to confirm</label>
                <input
                  type="text"
                  value={confirmText}
                  onChange={(e) => setConfirmText(e.target.value)}
                  placeholder="delete"
                  autoFocus
                />
              </div>
            )}
            <div className="form-actions">
              <button onClick={onClose}>Cancel</button>
              <button className="btn-danger" onClick={handleDelete} disabled={!canDelete}>
                {nothingKept ? `Delete all copies (${allIds.length})` : `Delete (${allIds.length})`}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

const DISTANCE_OPTIONS = [
  { value: 2, label: "Strict" },
  { value: 5, label: "Normal" },
  { value: 7, label: "Loose" },
];

export function Similar() {
  const { phase, progress, groups, error, scan, cancel, loadGroups, deleteFiles } = useSimilar();
  const { devices } = useDevices();
  const [maxDistance, setMaxDistance] = useState(5);
  const [deviceFilter, setDeviceFilter] = useState("");
  const [folder, setFolder] = useState("");
  // keepers per group: blake3 hashes of the files to keep (multi-select)
  const [keepers, setKeepers] = useState<Record<number, string[]>>({});
  const [deleteTarget, setDeleteTarget] = useState<number | null>(null);
  // Delete every connected copy of one specific file
  const [deleteFileTarget, setDeleteFileTarget] = useState<SimilarFile | null>(null);

  const connectedDeviceIds = useMemo(
    () => new Set(devices.filter((d) => d.isConnected).map((d) => d.id)),
    [devices]
  );

  const deviceNames = useMemo(() => {
    const map: Record<string, string> = {};
    for (const d of devices) map[d.id] = d.label;
    return map;
  }, [devices]);

  const keepersFor = (groupIdx: number, group: SimilarGroup): string[] =>
    keepers[groupIdx] ?? [group.files[0].blake3Hash];

  const toggleKeeper = (groupIdx: number, group: SimilarGroup, hash: string) => {
    setKeepers((k) => {
      const current = k[groupIdx] ?? [group.files[0].blake3Hash];
      const next = current.includes(hash)
        ? current.filter((h) => h !== hash)
        : [...current, hash];
      return { ...k, [groupIdx]: next };
    });
  };

  const planFor = (file: SimilarFile) => {
    const connected = file.locations.filter((l) => connectedDeviceIds.has(l.deviceId));
    return {
      file,
      locationIds: connected.map((l) => l.id),
      skippedOffline: file.locations.length - connected.length,
    };
  };

  const deletionPlan = (groupIdx: number, group: SimilarGroup) => {
    const keep = new Set(keepersFor(groupIdx, group));
    return group.files
      .filter((f) => !keep.has(f.blake3Hash))
      .map(planFor)
      .filter((p) => p.locationIds.length > 0);
  };

  const handleDistanceChange = (v: number) => {
    setMaxDistance(v);
    setKeepers({});
    if (phase === "ready") loadGroups(v, deviceFilter || undefined, folder || undefined);
  };

  const handleDeviceChange = (id: string) => {
    setDeviceFilter(id);
    setKeepers({});
    if (phase === "ready") loadGroups(maxDistance, id || undefined, folder || undefined);
  };

  const handlePickFolder = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string" && selected) {
      setFolder(selected);
      setKeepers({});
      if (phase === "ready") loadGroups(maxDistance, deviceFilter || undefined, selected);
    }
  };

  const handleClearFolder = () => {
    setFolder("");
    setKeepers({});
    if (phase === "ready") loadGroups(maxDistance, deviceFilter || undefined);
  };

  // Open the file in its default app (Preview for images) — first
  // connected copy that resolves wins
  const handlePreview = async (file: SimilarFile) => {
    for (const loc of file.locations) {
      if (!connectedDeviceIds.has(loc.deviceId)) continue;
      try {
        await openFile(loc.deviceId, loc.filePath);
        return;
      } catch {
        // try next location
      }
    }
  };

  const handleReveal = async (file: SimilarFile) => {
    for (const loc of file.locations) {
      if (!connectedDeviceIds.has(loc.deviceId)) continue;
      try {
        const abs = await resolveFilePath(loc.deviceId, loc.filePath);
        await revealInFinder(abs);
        return;
      } catch {
        // try next location
      }
    }
  };

  return (
    <div className="page">
      <h1>Similar Media</h1>
      <p className="text-muted-color text-sm" style={{ marginBottom: 16 }}>
        Finds pictures and videos that look alike but are not exact duplicates (videos compare by poster frame) — click pictures to toggle
        keep/delete, then delete everything not kept.
      </p>

      {error && <div className="error-msg">{error}</div>}

      <div className="browser-controls">
        <div className="filter-toggle">
          {DISTANCE_OPTIONS.map((o) => (
            <button
              key={o.value}
              className={maxDistance === o.value ? "active" : ""}
              onClick={() => handleDistanceChange(o.value)}
              disabled={phase === "scanning" || phase === "loading"}
            >
              {o.label}
            </button>
          ))}
        </div>
        {!folder && (
          <select
            value={deviceFilter}
            onChange={(e) => handleDeviceChange(e.target.value)}
            disabled={phase === "scanning" || phase === "loading"}
          >
            <option value="">All connected drives</option>
            {devices.filter((d) => d.isConnected).map((d) => (
              <option key={d.id} value={d.id}>
                {d.label} ({d.mountPoint})
              </option>
            ))}
          </select>
        )}
        {folder ? (
          <span className="folder-chip" title={folder}>
            {folder}
            <button
              className="folder-chip-clear"
              onClick={handleClearFolder}
              disabled={phase === "scanning" || phase === "loading"}
              aria-label="Clear folder filter"
            >
              ×
            </button>
          </span>
        ) : (
          <button
            onClick={handlePickFolder}
            disabled={phase === "scanning" || phase === "loading"}
            title="Limit to one folder (subfolders included)"
          >
            Folder...
          </button>
        )}
        {phase === "scanning" ? (
          <button className="btn-danger" onClick={cancel}>Cancel</button>
        ) : (
          <button
            className="btn-primary"
            onClick={() => scan(maxDistance, deviceFilter || undefined, folder || undefined)}
            disabled={phase === "loading"}
          >
            {phase === "ready" ? "Rescan" : "Find Similar Media"}
          </button>
        )}
      </div>

      {phase === "scanning" && (
        <div className="progress-container">
          <div className="progress-bar">
            <div
              className="progress-fill"
              style={{ width: `${progress.total ? (progress.processed / progress.total) * 100 : 0}%` }}
            />
          </div>
          <div className="progress-stats">
            <span>
              Analyzing pictures... {progress.processed} / {progress.total}
            </span>
          </div>
        </div>
      )}

      {phase === "loading" && <div className="skeleton skeleton-card" />}

      {phase === "ready" && groups.length === 0 && (
        <p className="empty">No similar pictures found</p>
      )}

      {phase === "ready" &&
        groups.map((group, gi) => {
          const keeperSet = new Set(keepersFor(gi, group));
          const plan = deletionPlan(gi, group);
          const deletableCount = plan.reduce((s, p) => s + p.locationIds.length, 0);
          const savings = plan.reduce((s, p) => s + p.file.fileSize * p.locationIds.length, 0);
          return (
            <div key={group.files[0].blake3Hash} className="similar-group">
              <div className="similar-group-header">
                <span>
                  {group.files.length} similar items · {keeperSet.size} kept
                  {savings > 0 && ` · save ${formatBytes(savings)}`}
                </span>
                <button
                  className="btn-danger"
                  disabled={deletableCount === 0}
                  title={
                    keeperSet.size === 0
                      ? "Nothing kept — deletes every copy of these files"
                      : undefined
                  }
                  onClick={() => setDeleteTarget(gi)}
                >
                  {keeperSet.size === 0
                    ? `Delete all copies (${deletableCount})`
                    : `Keep selected, delete rest (${deletableCount})`}
                </button>
              </div>
              <div className="similar-grid">
                {group.files.map((file) => {
                  const isKeeper = keeperSet.has(file.blake3Hash);
                  const offline = file.locations.every(
                    (l) => !connectedDeviceIds.has(l.deviceId)
                  );
                  return (
                    <div
                      key={file.blake3Hash}
                      className={`similar-card ${isKeeper ? "keeper" : ""}`}
                      onClick={() => toggleKeeper(gi, group, file.blake3Hash)}
                      role="button"
                      tabIndex={0}
                      aria-pressed={isKeeper}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" || e.key === " ") {
                          e.preventDefault();
                          toggleKeeper(gi, group, file.blake3Hash);
                        }
                      }}
                    >
                      <SimilarThumb file={file} connectedDeviceIds={connectedDeviceIds} />
                      <div className="similar-card-info">
                        <span className="similar-card-name" title={file.representativeName}>
                          {file.representativeName}
                        </span>
                        <span className="text-muted-color text-xs">
                          {formatBytes(file.fileSize)}
                          {file.locations.length > 1 && ` · ${file.locations.length} copies`}
                          {offline && " · offline"}
                        </span>
                      </div>
                      <span className={`similar-badge ${isKeeper ? "badge-keep" : "badge-delete"}`}>
                        {isKeeper ? "Keep" : "Delete"}
                      </span>
                      <div className="similar-card-actions">
                        <button
                          disabled={offline}
                          title={offline ? "No copies on connected drives" : "Open in Preview"}
                          onClick={(e) => {
                            e.stopPropagation();
                            handlePreview(file);
                          }}
                        >
                          Preview
                        </button>
                        <button
                          disabled={offline}
                          title={offline ? "No copies on connected drives" : "Show in Finder"}
                          onClick={(e) => {
                            e.stopPropagation();
                            handleReveal(file);
                          }}
                        >
                          Finder
                        </button>
                        <button
                          className="similar-action-delete"
                          disabled={offline}
                          title={
                            offline
                              ? "No copies on connected drives"
                              : "Delete every connected copy of this file"
                          }
                          onClick={(e) => {
                            e.stopPropagation();
                            setDeleteFileTarget(file);
                          }}
                        >
                          Delete
                        </button>
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
          );
        })}

      {deleteTarget !== null && groups[deleteTarget] && (
        <DeleteSimilarModal
          files={deletionPlan(deleteTarget, groups[deleteTarget])}
          nothingKept={keepersFor(deleteTarget, groups[deleteTarget]).length === 0}
          deviceNames={deviceNames}
          onConfirm={deleteFiles}
          onClose={() => {
            setDeleteTarget(null);
            // Group indices shift after deletion — drop stale keeper picks
            setKeepers({});
          }}
        />
      )}

      {deleteFileTarget && (
        <DeleteSimilarModal
          files={[planFor(deleteFileTarget)]}
          nothingKept
          deviceNames={deviceNames}
          onConfirm={deleteFiles}
          onClose={() => {
            setDeleteFileTarget(null);
            setKeepers({});
          }}
        />
      )}
    </div>
  );
}
