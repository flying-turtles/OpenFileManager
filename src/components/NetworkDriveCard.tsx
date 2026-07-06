import { useState } from "react";
import type { NetworkDrive } from "../types";

interface Props {
  drive: NetworkDrive;
  onMount: (id: string) => Promise<void>;
  onUnmount: (id: string) => Promise<void>;
  onScan: (drive: NetworkDrive) => void;
  onRemove: (id: string) => Promise<void>;
}

export function NetworkDriveCard({ drive, onMount, onUnmount, onScan, onRemove }: Props) {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const typeColors: Record<string, string> = {
    hot: "var(--type-hot)",
    cold: "var(--type-cold)",
    production: "var(--type-production)",
    unknown: "var(--type-unknown)",
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
    <div className={`device-card ${!drive.isMounted ? "disconnected" : ""}`}>
      <div className="device-header">
        <h3>{drive.label}</h3>
        <div className="device-badges">
          <span className="badge badge-network">{drive.protocol.toUpperCase()}</span>
          {drive.isMounted ? (
            <span className="badge badge-safe">Mounted</span>
          ) : (
            <span className="badge badge-disconnected">Unmounted</span>
          )}
          <span
            className="device-type-badge"
            style={{ backgroundColor: typeColors[drive.deviceType] || "var(--type-unknown)" }}
          >
            {drive.deviceType}
          </span>
        </div>
      </div>
      <div className="device-mount">
        {drive.host}/{drive.sharePath}
      </div>
      {error && <div className="error-msg" style={{ marginBottom: 8, fontSize: 12 }}>{error}</div>}
      <div className="device-actions">
        {drive.isMounted ? (
          <button onClick={handleUnmount} disabled={loading}>
            {loading ? "Unmounting..." : "Unmount"}
          </button>
        ) : (
          <button onClick={handleMount} disabled={loading}>
            {loading ? "Mounting..." : "Mount"}
          </button>
        )}
        <button
          onClick={() => onScan(drive)}
          disabled={!drive.isMounted || loading}
          title={drive.isMounted ? "Scan this share" : "Mount the drive first"}
        >
          Scan
        </button>
        <button className="btn-danger" onClick={handleRemove} disabled={loading}>
          Remove
        </button>
      </div>
    </div>
  );
}
