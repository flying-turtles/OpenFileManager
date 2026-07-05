import { useEffect, useState } from "react";
import type { DashboardStats } from "../types";
import { getDashboardStats } from "../api/commands";
import { formatBytes } from "../utils/format";

export function Dashboard() {
  const [stats, setStats] = useState<DashboardStats | null>(null);

  useEffect(() => {
    getDashboardStats().then(setStats).catch(console.error);
  }, []);

  return (
    <div className="page">
      <h1>Dashboard</h1>
      {!stats ? (
        <div className="stats-grid">
          {[...Array(5)].map((_, i) => <div key={i} className="skeleton skeleton-stat" />)}
        </div>
      ) : (
      <div className="stats-grid">
        <div className="stat-card">
          <div className="stat-value">{stats.totalFiles}</div>
          <div className="stat-label">Unique Files</div>
        </div>
        <div className="stat-card">
          <div className="stat-value">{stats.totalLocations}</div>
          <div className="stat-label">File Copies</div>
        </div>
        <div className={`stat-card ${stats.unsafeFiles > 0 ? "stat-danger" : ""}`}>
          <div className="stat-value">{stats.unsafeFiles}</div>
          <div className="stat-label">Unsafe Files</div>
        </div>
        <div className="stat-card">
          <div className="stat-value">{stats.totalDevices}</div>
          <div className="stat-label">Devices</div>
        </div>
        <div className="stat-card">
          <div className="stat-value">{formatBytes(stats.totalSizeBytes)}</div>
          <div className="stat-label">Total Size</div>
        </div>
      </div>
      )}
    </div>
  );
}
