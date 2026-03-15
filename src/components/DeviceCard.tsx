import type { StorageDevice } from "../types";
import "./DeviceCard.css";

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
}

interface Props {
  device: StorageDevice;
  onSetType: (deviceId: string, type_: string) => void;
  onScan: (device: StorageDevice) => void;
  onRemove: (deviceId: string) => void;
}

export function DeviceCard({ device, onSetType, onScan, onRemove }: Props) {
  const usedBytes = device.totalBytes - device.availableBytes;
  const usedPct = device.totalBytes > 0 ? (usedBytes / device.totalBytes) * 100 : 0;

  const typeColors: Record<string, string> = {
    hot: "var(--type-hot)",
    cold: "var(--type-cold)",
    production: "var(--type-production)",
    unknown: "var(--type-unknown)",
  };

  return (
    <div className={`device-card ${!device.isConnected ? "disconnected" : ""}`}>
      <div className="device-header">
        <h3>{device.label}</h3>
        <div className="device-badges">
          {!device.isConnected && <span className="badge badge-disconnected">Disconnected</span>}
          <span
            className="device-type-badge"
            style={{ backgroundColor: typeColors[device.deviceType] || "var(--type-unknown)" }}
          >
            {device.deviceType}
          </span>
        </div>
      </div>
      <div className="device-mount">{device.mountPoint}</div>
      <div className="capacity-bar">
        <div className="capacity-used" style={{ width: `${usedPct}%` }} />
      </div>
      <div className="capacity-text">
        {formatBytes(usedBytes)} / {formatBytes(device.totalBytes)}
      </div>
      {device.isRemovable && <div className="removable-tag">Removable</div>}
      <div className="device-actions">
        <select
          value={device.deviceType}
          onChange={(e) => onSetType(device.id, e.target.value)}
        >
          <option value="unknown">Unknown</option>
          <option value="hot">Hot</option>
          <option value="cold">Cold</option>
          <option value="production">Production</option>
        </select>
        <button onClick={() => onScan(device)} disabled={!device.isConnected}>
          Scan
        </button>
        <button className="btn-danger" onClick={() => onRemove(device.id)}>
          Remove
        </button>
      </div>
    </div>
  );
}
