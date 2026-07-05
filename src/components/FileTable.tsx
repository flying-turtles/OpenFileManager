import { useState, useRef, useMemo, useCallback } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import type { FileLocation, FileSafety } from "../types";
import { resolveFilePath, openFile } from "../api/commands";
import { SafetyBadge } from "./SafetyBadge";
import { FilePreview } from "./FilePreview";
import { useFocusTrap } from "../hooks/useFocusTrap";
import { formatBytes } from "../utils/format";
import "./FileTable.css";

function DeleteCopyModal({ path, deleting, onCancel, onDelete }: {
  path: string;
  deleting: boolean;
  onCancel: () => void;
  onDelete: () => void;
}) {
  const trapRef = useFocusTrap<HTMLDivElement>();
  return (
    <div className="modal-overlay" onClick={() => !deleting && onCancel()} role="dialog" aria-label="Delete file copy" aria-modal="true" onKeyDown={(e) => { if (e.key === "Escape" && !deleting) onCancel(); }}>
      <div className="modal-content" ref={trapRef} onClick={(e) => e.stopPropagation()}>
        <h2>Delete file copy?</h2>
        <p style={{ marginBottom: 12, wordBreak: "break-all", color: "var(--text-muted)", fontSize: 13 }}>
          {path}
        </p>
        <p style={{ marginBottom: 20, color: "var(--danger)", fontSize: 13 }}>
          The file will be moved to the Trash. Other copies will remain.
        </p>
        <div className="form-actions">
          <button onClick={onCancel} disabled={deleting}>
            Cancel
          </button>
          <button className="btn-danger" onClick={onDelete} disabled={deleting}>
            {deleting ? "Deleting..." : "Delete"}
          </button>
        </div>
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
  onDeleteLocation?: (locationId: number, filePath: string) => Promise<void>;
}

export function FileTable({ files, totalCount, deviceNames, connectedDeviceIds, selectedDeviceId, onGetSafety, onDeleteLocation }: Props) {
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const [safety, setSafety] = useState<FileSafety | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<{ id: number; path: string } | null>(null);
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

  const handleDelete = async () => {
    if (!confirmDelete || !onDeleteLocation) return;
    setDeleting(true);
    try {
      await onDeleteLocation(confirmDelete.id, confirmDelete.path);
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
    </div>
  );
}
