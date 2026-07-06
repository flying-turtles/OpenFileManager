import { useState, useRef, useMemo, useCallback } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import type { FileLocation, FileSafety, BulkDeleteEvent, BulkDeleteResult } from "../types";
import { resolveFilePath, openFile } from "../api/commands";
import { SafetyBadge } from "./SafetyBadge";
import { FilePreview } from "./FilePreview";
import { useFocusTrap } from "../hooks/useFocusTrap";
import { formatBytes } from "../utils/format";
import "./FileTable.css";

export function PermanentToggle({ permanent, onChange, disabled }: {
  permanent: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <label className="permanent-toggle">
      <input
        type="checkbox"
        checked={permanent}
        onChange={(e) => onChange(e.target.checked)}
        disabled={disabled}
      />
      Delete permanently (skip the Trash — cannot be undone)
    </label>
  );
}

function DeleteCopyModal({ path, deleting, onCancel, onDelete }: {
  path: string;
  deleting: boolean;
  onCancel: () => void;
  onDelete: (permanent: boolean) => void;
}) {
  const trapRef = useFocusTrap<HTMLDivElement>();
  const [permanent, setPermanent] = useState(false);
  return (
    <div className="modal-overlay" onClick={() => !deleting && onCancel()} role="dialog" aria-label="Delete file copy" aria-modal="true" onKeyDown={(e) => { if (e.key === "Escape" && !deleting) onCancel(); }}>
      <div className="modal-content" ref={trapRef} onClick={(e) => e.stopPropagation()}>
        <h2>Delete file copy?</h2>
        <p style={{ marginBottom: 12, wordBreak: "break-all", color: "var(--text-muted)", fontSize: 13 }}>
          {path}
        </p>
        <p style={{ marginBottom: 12, color: "var(--danger)", fontSize: 13 }}>
          {permanent
            ? "The file will be permanently deleted from disk. Other copies will remain."
            : "The file will be moved to the Trash. Other copies will remain."}
        </p>
        <PermanentToggle permanent={permanent} onChange={setPermanent} disabled={deleting} />
        <div className="form-actions">
          <button onClick={onCancel} disabled={deleting}>
            Cancel
          </button>
          <button className="btn-danger" onClick={() => onDelete(permanent)} disabled={deleting}>
            {deleting ? "Deleting..." : permanent ? "Delete Permanently" : "Delete"}
          </button>
        </div>
      </div>
    </div>
  );
}

function DeleteAllCopiesModal({ fileName, locations, deviceNames, onConfirm, onClose }: {
  fileName: string;
  locations: FileLocation[];
  deviceNames?: Record<string, string>;
  onConfirm: (locationIds: number[], onEvent: (e: BulkDeleteEvent) => void, permanent?: boolean) => Promise<BulkDeleteResult>;
  onClose: (deletedIds: number[]) => void;
}) {
  const trapRef = useFocusTrap<HTMLDivElement>();
  const [confirmText, setConfirmText] = useState("");
  const [deleting, setDeleting] = useState(false);
  const [permanent, setPermanent] = useState(false);
  const [result, setResult] = useState<BulkDeleteResult | null>(null);

  const handleDelete = async () => {
    setDeleting(true);
    try {
      const res = await onConfirm(locations.map((l) => l.id), () => {}, permanent);
      setResult(res);
    } catch (e: any) {
      setResult({ succeeded: [], failed: [{ locationId: 0, filePath: "", error: String(e) }] });
    } finally {
      setDeleting(false);
    }
  };

  const close = () => onClose(result?.succeeded ?? []);

  return (
    <div className="modal-overlay" onClick={() => !deleting && close()} role="dialog" aria-label="Delete file everywhere" aria-modal="true" onKeyDown={(e) => { if (e.key === "Escape" && !deleting) close(); }}>
      <div className="modal-content" ref={trapRef} onClick={(e) => e.stopPropagation()} style={{ maxWidth: 560 }}>
        {result ? (
          <>
            <h2>Deleted</h2>
            <p>{result.succeeded.length} cop{result.succeeded.length !== 1 ? "ies" : "y"} of "{fileName}" moved to the Trash.</p>
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
              <button className="btn-primary" onClick={close}>Close</button>
            </div>
          </>
        ) : (
          <>
            <h2>Delete "{fileName}" everywhere?</h2>
            <p className="bulk-delete-warning">
              All {locations.length} cop{locations.length !== 1 ? "ies" : "y"} on connected drives
              will be {permanent ? "permanently deleted" : "moved to the Trash"}. Copies on
              disconnected drives are kept.
            </p>
            <div className="bulk-delete-file-list">
              {locations.map((l) => (
                <div key={l.id} className="bulk-delete-file-item">
                  <span className="bulk-delete-file-path">
                    [{deviceNames?.[l.deviceId] ?? l.deviceId.slice(0, 8)}] {l.filePath}
                  </span>
                  <span className="text-muted-color text-xs">{formatBytes(l.fileSize)}</span>
                </div>
              ))}
            </div>
            <PermanentToggle permanent={permanent} onChange={setPermanent} disabled={deleting} />
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
            <div className="form-actions">
              <button onClick={close}>Cancel</button>
              <button className="btn-danger" onClick={handleDelete} disabled={confirmText !== "delete" || deleting}>
                {deleting ? "Deleting..." : `Delete all copies (${locations.length})`}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

type RowItem =
  | { type: "file"; file: FileLocation }
  | { type: "detail"; file: FileLocation; safety: FileSafety };

interface Props {
  files: FileLocation[];
  totalCount?: number;
  deviceNames?: Record<string, string>;
  connectedDeviceIds?: Set<string>;
  selectedDeviceId?: string;
  onGetSafety?: (hash: string) => Promise<FileSafety | null>;
  onDeleteLocation?: (locationId: number, filePath: string, permanent?: boolean) => Promise<void>;
  onBulkDelete?: (locationIds: number[], onEvent: (e: BulkDeleteEvent) => void, permanent?: boolean) => Promise<BulkDeleteResult>;
}

export function FileTable({ files, totalCount, deviceNames, connectedDeviceIds, selectedDeviceId, onGetSafety, onDeleteLocation, onBulkDelete }: Props) {
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const [safety, setSafety] = useState<FileSafety | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<{ id: number; path: string } | null>(null);
  const [confirmDeleteAll, setConfirmDeleteAll] = useState<{ fileName: string; locations: FileLocation[] } | null>(null);
  const [deleting, setDeleting] = useState(false);
  const parentRef = useRef<HTMLDivElement>(null);

  const handleOpenFile = useCallback(async (loc: FileLocation) => {
    try {
      await openFile(loc.deviceId, loc.filePath);
    } catch (e) {
      console.error("Failed to open file:", e);
    }
  }, []);

  const handleRevealFile = useCallback(async (loc: FileLocation) => {
    try {
      const absPath = await resolveFilePath(loc.deviceId, loc.filePath);
      await revealItemInDir(absPath);
    } catch (e) {
      console.error("Failed to reveal file:", e);
    }
  }, []);

  const toggleExpand = async (file: FileLocation) => {
    if (expandedId === file.id) {
      setExpandedId(null);
      setSafety(null);
      return;
    }
    setExpandedId(file.id);
    if (onGetSafety) {
      const s = await onGetSafety(file.blake3Hash);
      setSafety(s);
    }
  };

  const handleDelete = async (permanent: boolean) => {
    if (!confirmDelete || !onDeleteLocation) return;
    setDeleting(true);
    try {
      await onDeleteLocation(confirmDelete.id, confirmDelete.path, permanent);
      // Update local safety state to reflect removal
      if (safety) {
        const updated = {
          ...safety,
          totalCopies: safety.totalCopies - 1,
          locations: safety.locations.filter((l) => l.id !== confirmDelete.id),
        };
        if (updated.locations.length === 0) {
          setExpandedId(null);
          setSafety(null);
        } else {
          setSafety(updated);
        }
      }
    } finally {
      setDeleting(false);
      setConfirmDelete(null);
    }
  };

  const rows: RowItem[] = useMemo(() => {
    const result: RowItem[] = [];
    for (const f of files) {
      result.push({ type: "file", file: f });
      if (expandedId === f.id && safety) {
        result.push({ type: "detail", file: f, safety });
      }
    }
    return result;
  }, [files, expandedId, safety]);

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: (i) => (rows[i]?.type === "detail" ? 320 : 36),
    overscan: 20,
  });

  const virtualItems = virtualizer.getVirtualItems();
  const [paddingTop, paddingBottom] =
    virtualItems.length > 0
      ? [
          virtualItems[0].start,
          virtualizer.getTotalSize() - virtualItems[virtualItems.length - 1].end,
        ]
      : [0, 0];

  const showLoadingCount =
    totalCount !== undefined && totalCount > files.length;

  return (
    <div>
      {showLoadingCount && (
        <div className="file-table-status">
          Showing {files.length.toLocaleString()} of{" "}
          {totalCount.toLocaleString()} files (loading…)
        </div>
      )}
      <div ref={parentRef} className="file-table-scroll">
        <table className="file-table">
          <thead>
            <tr>
              <th>Name</th>
              <th>Size</th>
              <th>Path</th>
              <th>Mode</th>
              <th>Modified</th>
            </tr>
          </thead>
          <tbody>
            {paddingTop > 0 && (
              <tr>
                <td style={{ height: paddingTop, padding: 0 }} />
              </tr>
            )}
            {virtualItems.map((virtualRow) => {
              const row = rows[virtualRow.index];
              if (row.type === "detail") {
                return (
                  <tr
                    key={`detail-${row.file.id}`}
                    className="detail-row"
                    ref={virtualizer.measureElement}
                    data-index={virtualRow.index}
                  >
                    <td colSpan={5}>
                      <FilePreview
                        locations={row.safety.locations}
                        fileName={row.file.fileName}
                        preferredDeviceId={selectedDeviceId}
                      />
                      <div className="safety-detail">
                        <SafetyBadge
                          totalCopies={row.safety.totalCopies}
                          coldCopies={row.safety.coldCopies}
                          isSafe={row.safety.isSafe}
                        />
                        <span>
                          {row.safety.totalCopies} copies (
                          {row.safety.hotCopies} hot, {row.safety.coldCopies}{" "}
                          cold)
                        </span>
                        {onBulkDelete && (() => {
                          const connected = row.safety.locations.filter(
                            (l) => connectedDeviceIds?.has(l.deviceId) ?? false
                          );
                          return (
                            <button
                              className="btn-danger"
                              disabled={connected.length === 0}
                              title={
                                connected.length === 0
                                  ? "No copies on connected drives"
                                  : "Delete every copy on connected drives"
                              }
                              onClick={(e) => {
                                e.stopPropagation();
                                setConfirmDeleteAll({
                                  fileName: row.file.fileName,
                                  locations: connected,
                                });
                              }}
                            >
                              Delete Everywhere
                            </button>
                          );
                        })()}
                        <div className="locations-list">
                          {row.safety.locations.map((loc) => {
                            const connected = connectedDeviceIds?.has(loc.deviceId) ?? false;
                            return (
                              <div key={loc.id} className="location-item location-item-row">
                                <span className="location-item-path">
                                  [{deviceNames?.[loc.deviceId] ?? loc.deviceId.slice(0, 8)}] {loc.filePath}
                                </span>
                                <div className="location-actions">
                                  <button
                                    className="btn-location-action"
                                    disabled={!connected}
                                    title={connected ? "Show in Finder" : "Device not connected"}
                                    onClick={(e) => { e.stopPropagation(); handleRevealFile(loc); }}
                                  >
                                    Reveal
                                  </button>
                                  <button
                                    className="btn-location-action"
                                    disabled={!connected}
                                    title={connected ? "Open file" : "Device not connected"}
                                    onClick={(e) => { e.stopPropagation(); handleOpenFile(loc); }}
                                  >
                                    Open
                                  </button>
                                  {onDeleteLocation && (
                                    <button
                                      className="btn-delete-copy"
                                      disabled={!connected}
                                      title={connected ? "Delete this copy" : "Device not connected"}
                                      onClick={(e) => {
                                        e.stopPropagation();
                                        setConfirmDelete({ id: loc.id, path: loc.filePath });
                                      }}
                                    >
                                      Delete
                                    </button>
                                  )}
                                </div>
                              </div>
                            );
                          })}
                        </div>
                      </div>
                    </td>
                  </tr>
                );
              }
              return (
                <tr
                  key={row.file.id}
                  className={
                    expandedId === row.file.id ? "expanded" : ""
                  }
                  onClick={() => toggleExpand(row.file)}
                  role="button"
                  tabIndex={0}
                  aria-expanded={expandedId === row.file.id}
                  onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); toggleExpand(row.file); } }}
                  style={{ cursor: "pointer" }}
                >
                  <td>{row.file.fileName}</td>
                  <td>{formatBytes(row.file.fileSize)}</td>
                  <td className="path-cell">{row.file.filePath}</td>
                  <td>{row.file.scanMode}</td>
                  <td>{row.file.modifiedAt || "-"}</td>
                </tr>
              );
            })}
            {paddingBottom > 0 && (
              <tr>
                <td style={{ height: paddingBottom, padding: 0 }} />
              </tr>
            )}
          </tbody>
        </table>
      </div>

      {confirmDelete && (
        <DeleteCopyModal
          path={confirmDelete.path}
          deleting={deleting}
          onCancel={() => setConfirmDelete(null)}
          onDelete={handleDelete}
        />
      )}

      {confirmDeleteAll && onBulkDelete && (
        <DeleteAllCopiesModal
          fileName={confirmDeleteAll.fileName}
          locations={confirmDeleteAll.locations}
          deviceNames={deviceNames}
          onConfirm={onBulkDelete}
          onClose={(deletedIds) => {
            setConfirmDeleteAll(null);
            if (deletedIds.length > 0 && safety) {
              const deleted = new Set(deletedIds);
              const remaining = safety.locations.filter((l) => !deleted.has(l.id));
              if (remaining.length === 0) {
                setExpandedId(null);
                setSafety(null);
              } else {
                setSafety({
                  ...safety,
                  totalCopies: safety.totalCopies - (safety.locations.length - remaining.length),
                  locations: remaining,
                });
              }
            }
          }}
        />
      )}
    </div>
  );
}
