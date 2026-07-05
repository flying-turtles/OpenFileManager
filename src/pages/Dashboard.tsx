import { useEffect, useState } from "react";
import type { DashboardStats, WasteCandidate } from "../types";
import { getDashboardStats, getWasteCandidates } from "../api/commands";
import { formatBytes } from "../utils/format";

export type FilesFilter = "all" | "safe" | "unsafe" | "duplicates";

interface Props {
  onOpenFiles: (filter: FilesFilter) => void;
  onOpenDevices: () => void;
}

const WASTE_LIMIT = 20;

export function Dashboard({ onOpenFiles, onOpenDevices }: Props) {
  const [stats, setStats] = useState<DashboardStats | null>(null);
  const [waste, setWaste] = useState<WasteCandidate[] | null>(null);

  useEffect(() => {
    getDashboardStats().then(setStats).catch(console.error);
    getWasteCandidates().then(setWaste).catch(console.error);
  }, []);

  const totalWasted = waste?.reduce((s, w) => s + w.wastedBytes, 0) ?? 0;

  return (
    <div className="page">
      <h1>Dashboard</h1>
      {!stats ? (
        <div className="stats-grid">
          {[...Array(5)].map((_, i) => <div key={i} className="skeleton skeleton-stat" />)}
        </div>
      ) : (
      <div className="stats-grid">
        <div className="stat-card stat-clickable" onClick={() => onOpenFiles("all")} role="button" tabIndex={0}
          onKeyDown={(e) => { if (e.key === "Enter") onOpenFiles("all"); }}>
          <div className="stat-value">{stats.totalFiles}</div>
          <div className="stat-label">Unique Files</div>
        </div>
        <div className="stat-card stat-clickable" onClick={() => onOpenFiles("all")} role="button" tabIndex={0}
          onKeyDown={(e) => { if (e.key === "Enter") onOpenFiles("all"); }}>
          <div className="stat-value">{stats.totalLocations}</div>
          <div className="stat-label">File Copies</div>
        </div>
        <div
          className={`stat-card stat-clickable ${stats.unsafeFiles > 0 ? "stat-danger" : ""}`}
          onClick={() => onOpenFiles("unsafe")}
          role="button"
          tabIndex={0}
          onKeyDown={(e) => { if (e.key === "Enter") onOpenFiles("unsafe"); }}
        >
          <div className="stat-value">{stats.unsafeFiles}</div>
          <div className="stat-label">Unsafe Files</div>
        </div>
        <div className="stat-card stat-clickable" onClick={onOpenDevices} role="button" tabIndex={0}
          onKeyDown={(e) => { if (e.key === "Enter") onOpenDevices(); }}>
          <div className="stat-value">{stats.totalDevices}</div>
          <div className="stat-label">Devices</div>
        </div>
        <div className="stat-card">
          <div className="stat-value">{formatBytes(stats.totalSizeBytes)}</div>
          <div className="stat-label">Total Size</div>
        </div>
      </div>
      )}

      {waste && waste.length > 0 && (
        <div className="waste-section">
          <div className="waste-header">
            <h2 className="text-base text-muted-color">Duplicate Waste</h2>
            <span className="text-muted-color text-sm">
              {formatBytes(totalWasted)} in files with more than 2 copies
            </span>
            <button onClick={() => onOpenFiles("duplicates")}>Review Duplicates</button>
          </div>
          <div className="waste-list">
            {waste.slice(0, WASTE_LIMIT).map((w) => (
              <div key={w.blake3Hash} className="waste-row">
                <span className="waste-name" title={w.representativeName}>
                  {w.representativeName}
                </span>
                <span className="text-muted-color text-xs">{w.totalCopies} copies</span>
                <span className="text-muted-color text-xs">{formatBytes(w.fileSize)} each</span>
                <span className="waste-bytes">{formatBytes(w.wastedBytes)} wasted</span>
              </div>
            ))}
            {waste.length > WASTE_LIMIT && (
              <div className="waste-row text-muted-color text-xs">
                ...and {waste.length - WASTE_LIMIT} more
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
