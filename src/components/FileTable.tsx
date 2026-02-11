import { useState } from "react";
import type { FileLocation, FileSafety } from "../types";
import { SafetyBadge } from "./SafetyBadge";

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
}

interface Props {
  files: FileLocation[];
  onGetSafety?: (hash: string) => Promise<FileSafety | null>;
}

export function FileTable({ files, onGetSafety }: Props) {
  const [expanded, setExpanded] = useState<string | null>(null);
  const [safety, setSafety] = useState<FileSafety | null>(null);

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

  return (
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
        {files.map((f) => (
          <>
            <tr
              key={f.id}
              className={expanded === f.blake3_hash ? "expanded" : ""}
              onClick={() => toggleExpand(f.blake3_hash)}
              style={{ cursor: "pointer" }}
            >
              <td>{f.file_name}</td>
              <td>{formatBytes(f.file_size)}</td>
              <td className="path-cell">{f.file_path}</td>
              <td>{f.scan_mode}</td>
              <td>{f.modified_at || "-"}</td>
            </tr>
            {expanded === f.blake3_hash && safety && (
              <tr key={`${f.id}-detail`} className="detail-row">
                <td colSpan={5}>
                  <div className="safety-detail">
                    <SafetyBadge
                      totalCopies={safety.total_copies}
                      coldCopies={safety.cold_copies}
                      isSafe={safety.is_safe}
                    />
                    <span>
                      {safety.total_copies} copies ({safety.hot_copies} hot, {safety.cold_copies}{" "}
                      cold)
                    </span>
                    <div className="locations-list">
                      {safety.locations.map((loc) => (
                        <div key={loc.id} className="location-item">
                          [{loc.device_id.slice(0, 8)}] {loc.file_path}
                        </div>
                      ))}
                    </div>
                  </div>
                </td>
              </tr>
            )}
          </>
        ))}
      </tbody>
    </table>
  );
}
