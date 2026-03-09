import { useState } from "react";
import type { NetworkDrive } from "../types";

interface Props {
  drive: NetworkDrive;
  onMount: (id: string) => Promise<void>;
  onUnmount: (id: string) => Promise<void>;
  onRemove: (id: string) => Promise<void>;
}

export function NetworkDriveCard({ drive, onMount, onUnmount, onRemove }: Props) {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const typeColors: Record<string, string> = {
    hot: "#e74c3c",
    cold: "#3498db",
    unknown: "#95a5a6",
  };

  const handleMount = async () => {
    setLoading(true);
    setError("");
    try {
      await onMount(drive.id);
    } catch (e: any) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const handleUnmount = async () => {
    setLoading(true);
    setError("");
    try {
      await onUnmount(drive.id);
    } catch (e: any) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const handleRemove = async () => {
    setLoading(true);
    try {
      await onRemove(drive.id);
    } catch (e: any) {
      setError(String(e));
      setLoading(false);
    }
  };

  return (
    <div className={`device-card ${!drive.is_mounted ? "disconnected" : ""}`}>
      <div className="device-header">
        <h3>{drive.label}</h3>
        <div className="device-badges">
          <span className="badge badge-network">{drive.protocol.toUpperCase()}</span>
          {drive.is_mounted ? (
            <span className="badge badge-safe">Mounted</span>
          ) : (
            <span className="badge badge-disconnected">Unmounted</span>
          )}
          <span
            className="device-type-badge"
            style={{ backgroundColor: typeColors[drive.device_type] || "#95a5a6" }}
          >
            {drive.device_type}
          </span>
        </div>
      </div>
      <div className="device-mount">
        {drive.host}/{drive.share_path}
      </div>
      {error && <div className="error-msg" style={{ marginBottom: 8, fontSize: 12 }}>{error}</div>}
      <div className="device-actions">
        {drive.is_mounted ? (
          <button onClick={handleUnmount} disabled={loading}>
            {loading ? "Unmounting..." : "Unmount"}
          </button>
        ) : (
          <button onClick={handleMount} disabled={loading}>
            {loading ? "Mounting..." : "Mount"}
          </button>
        )}
        <button className="btn-danger" onClick={handleRemove} disabled={loading}>
          Remove
        </button>
      </div>
    </div>
  );
}
