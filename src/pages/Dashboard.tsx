import { useEffect, useState } from "react";
import type { DashboardStats } from "../types";
import { getDashboardStats } from "../api/commands";

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
}

export function Dashboard() {
  const [stats, setStats] = useState<DashboardStats | null>(null);

  useEffect(() => {
    getDashboardStats().then(setStats).catch(console.error);
  }, []);

  if (!stats) return <div className="page">Loading...</div>;

  return (
    <div className="page">
      <h1>Dashboard</h1>
      <div className="stats-grid">
        <div className="stat-card">
          <div className="stat-value">{stats.total_files}</div>
          <div className="stat-label">Unique Files</div>
        </div>
        <div className="stat-card">
          <div className="stat-value">{stats.total_locations}</div>
          <div className="stat-label">File Copies</div>
        </div>
        <div className={`stat-card ${stats.unsafe_files > 0 ? "stat-danger" : ""}`}>
          <div className="stat-value">{stats.unsafe_files}</div>
          <div className="stat-label">Unsafe Files</div>
        </div>
        <div className="stat-card">
          <div className="stat-value">{stats.total_devices}</div>
          <div className="stat-label">Devices</div>
        </div>
        <div className="stat-card">
          <div className="stat-value">{formatBytes(stats.total_size_bytes)}</div>
          <div className="stat-label">Total Size</div>
        </div>
      </div>
    </div>
  );
}
