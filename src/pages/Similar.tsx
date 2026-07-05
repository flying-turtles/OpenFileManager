import { useState, useEffect, useMemo } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { useSimilar } from "../hooks/useSimilar";
import { useDevices } from "../hooks/useDevices";
import { useFocusTrap } from "../hooks/useFocusTrap";
import { resolveFilePath, getThumbnail } from "../api/commands";
import type { SimilarFile, SimilarGroup, BulkDeleteEvent, BulkDeleteResult } from "../types";
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

function DeleteSimilarModal({ files, onConfirm, onClose }: {
  files: { file: SimilarFile; locationIds: number[]; skippedOffline: number }[];
  onConfirm: (locationIds: number[], onEvent: (e: BulkDeleteEvent) => void) => Promise<BulkDeleteResult>;
  onClose: () => void;
}) {
  const trapRef = useFocusTrap<HTMLDivElement>();
  const [deleting, setDeleting] = useState(false);
  const [result, setResult] = useState<BulkDeleteResult | null>(null);
  const [processed, setProcessed] = useState(0);

  const allIds = files.flatMap((f) => f.locationIds);
  const totalBytes = files.reduce((s, f) => s + f.file.fileSize * f.locationIds.length, 0);
  const skippedOffline = files.reduce((s, f) => s + f.skippedOffline, 0);

  const handleDelete = async () => {
    setDeleting(true);
    try {
      const res = await onConfirm(allIds, (event) => {
        if ("Progress" in event) setProcessed(event.Progress.processed);
      });
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
            <h2>Delete similar pictures?</h2>
            <p className="bulk-delete-warning">
              This permanently deletes {allIds.length} file cop
              {allIds.length !== 1 ? "ies" : "y"} ({formatBytes(totalBytes)}) from disk. This
              cannot be undone.
            </p>
            {skippedOffline > 0 && (
              <p className="text-muted-color text-sm" style={{ marginBottom: 8 }}>
                {skippedOffline} cop{skippedOffline !== 1 ? "ies" : "y"} on disconnected drives will
                be kept.
              </p>
            )}
            <div className="bulk-delete-file-list">
              {files.map(({ file, locationIds }) =>
                file.locations
                  .filter((l) => locationIds.includes(l.id))
                  .map((l) => (
                    <div key={l.id} className="bulk-delete-file-item">
                      <span className="bulk-delete-file-path">{l.filePath}</span>
                      <span className="text-muted-color text-xs">{formatBytes(file.fileSize)}</span>
                    </div>
                  ))
              )}
            </div>
            <div className="form-actions">
              <button onClick={onClose}>Cancel</button>
              <button className="btn-danger" onClick={handleDelete}>
                Delete ({allIds.length})
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
  // keeper per group: blake3Hash of the file to keep
  const [keepers, setKeepers] = useState<Record<number, string>>({});
  const [deleteTarget, setDeleteTarget] = useState<number | null>(null);

  const connectedDeviceIds = useMemo(
    () => new Set(devices.filter((d) => d.isConnected).map((d) => d.id)),
    [devices]
  );

  const keeperFor = (groupIdx: number, group: SimilarGroup) =>
    keepers[groupIdx] ?? group.files[0].blake3Hash;

  const deletionPlan = (groupIdx: number, group: SimilarGroup) => {
    const keeper = keeperFor(groupIdx, group);
    return group.files
      .filter((f) => f.blake3Hash !== keeper)
      .map((file) => {
        const connected = file.locations.filter((l) => connectedDeviceIds.has(l.deviceId));
        return {
          file,
          locationIds: connected.map((l) => l.id),
          skippedOffline: file.locations.length - connected.length,
        };
      })
      .filter((p) => p.locationIds.length > 0);
  };

  const handleDistanceChange = (v: number) => {
    setMaxDistance(v);
    setKeepers({});
    if (phase === "ready") loadGroups(v);
  };

  return (
    <div className="page">
      <h1>Similar Pictures</h1>
      <p className="text-muted-color text-sm" style={{ marginBottom: 16 }}>
        Finds pictures that look alike but are not exact duplicates — pick the one to keep and
        delete the rest.
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
        {phase === "scanning" ? (
          <button className="btn-danger" onClick={cancel}>Cancel</button>
        ) : (
          <button
            className="btn-primary"
            onClick={() => scan(maxDistance)}
            disabled={phase === "loading"}
          >
            {phase === "ready" ? "Rescan" : "Find Similar Pictures"}
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
          const keeper = keeperFor(gi, group);
          const plan = deletionPlan(gi, group);
          const deletableCount = plan.reduce((s, p) => s + p.locationIds.length, 0);
          const savings = plan.reduce((s, p) => s + p.file.fileSize * p.locationIds.length, 0);
          return (
            <div key={group.files[0].blake3Hash} className="similar-group">
              <div className="similar-group-header">
                <span>
                  {group.files.length} similar pictures
                  {savings > 0 && ` · save ${formatBytes(savings)}`}
                </span>
                <button
                  className="btn-danger"
                  disabled={deletableCount === 0}
                  onClick={() => setDeleteTarget(gi)}
                >
                  Keep selected, delete rest ({deletableCount})
                </button>
              </div>
              <div className="similar-grid">
                {group.files.map((file) => {
                  const isKeeper = file.blake3Hash === keeper;
                  const offline = file.locations.every(
                    (l) => !connectedDeviceIds.has(l.deviceId)
                  );
                  return (
                    <div
                      key={file.blake3Hash}
                      className={`similar-card ${isKeeper ? "keeper" : ""}`}
                      onClick={() => setKeepers((k) => ({ ...k, [gi]: file.blake3Hash }))}
                      role="button"
                      tabIndex={0}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" || e.key === " ") {
                          e.preventDefault();
                          setKeepers((k) => ({ ...k, [gi]: file.blake3Hash }));
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
          onConfirm={deleteFiles}
          onClose={() => {
            setDeleteTarget(null);
            // Group indices shift after deletion — drop stale keeper picks
            setKeepers({});
          }}
        />
      )}
    </div>
  );
}
