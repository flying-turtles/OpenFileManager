import { useState, useRef, useMemo } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { FileLocation, FileSafety } from "../types";
import { SafetyBadge } from "./SafetyBadge";

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
}

type RowItem =
  | { type: "file"; file: FileLocation }
  | { type: "detail"; file: FileLocation; safety: FileSafety };

interface Props {
  files: FileLocation[];
  totalCount?: number;
  onGetSafety?: (hash: string) => Promise<FileSafety | null>;
}

export function FileTable({ files, totalCount, onGetSafety }: Props) {
  const [expanded, setExpanded] = useState<string | null>(null);
  const [safety, setSafety] = useState<FileSafety | null>(null);
  const parentRef = useRef<HTMLDivElement>(null);

  const toggleExpand = async (hash: string) => {
    if (expanded === hash) {
      setExpanded(null);
      setSafety(null);
      return;
    }
    setExpanded(hash);
    if (onGetSafety) {
      const s = await onGetSafety(hash);
      setSafety(s);
    }
  };

  const rows: RowItem[] = useMemo(() => {
    const result: RowItem[] = [];
    for (const f of files) {
      result.push({ type: "file", file: f });
      if (expanded === f.blake3_hash && safety) {
        result.push({ type: "detail", file: f, safety });
      }
    }
    return result;
  }, [files, expanded, safety]);

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: (i) => (rows[i]?.type === "detail" ? 120 : 36),
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
                      <div className="safety-detail">
                        <SafetyBadge
                          totalCopies={row.safety.total_copies}
                          coldCopies={row.safety.cold_copies}
                          isSafe={row.safety.is_safe}
                        />
                        <span>
                          {row.safety.total_copies} copies (
                          {row.safety.hot_copies} hot, {row.safety.cold_copies}{" "}
                          cold)
                        </span>
                        <div className="locations-list">
                          {row.safety.locations.map((loc) => (
                            <div key={loc.id} className="location-item">
                              [{loc.device_id.slice(0, 8)}] {loc.file_path}
                            </div>
                          ))}
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
                    expanded === row.file.blake3_hash ? "expanded" : ""
                  }
                  onClick={() => toggleExpand(row.file.blake3_hash)}
                  style={{ cursor: "pointer" }}
                >
                  <td>{row.file.file_name}</td>
                  <td>{formatBytes(row.file.file_size)}</td>
                  <td className="path-cell">{row.file.file_path}</td>
                  <td>{row.file.scan_mode}</td>
                  <td>{row.file.modified_at || "-"}</td>
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
    </div>
  );
}
