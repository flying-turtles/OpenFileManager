import type { StorageDevice } from "../types";
import { formatBytes } from "../utils/format";
import "./DeviceCard.css";

interface Props {
  device: StorageDevice;
  onSetType: (deviceId: string, type_: string) => void;
  onSetSpeed: (deviceId: string, speed: string) => void;
  onScan: (device: StorageDevice) => void;
  onVerify?: (device: StorageDevice) => void;
  verifyDisabled?: boolean;
  onRemove: (deviceId: string) => void;
}

export function DeviceCard({ device, onSetType, onSetSpeed, onScan, onVerify, verifyDisabled, onRemove }: Props) {
  const hasTotal = device.totalBytes > 0;
  const usedBytes = hasTotal ? device.totalBytes - device.availableBytes : 0;
  const usedPct = hasTotal ? (usedBytes / device.totalBytes) * 100 : 0;

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
      {hasTotal ? (
        <>
          <div className="capacity-bar">
            <div className="capacity-used" style={{ width: `${usedPct}%` }} />
          </div>
          <div className="capacity-text">
            {formatBytes(usedBytes)} / {formatBytes(device.totalBytes)}
          </div>
        </>
      ) : (
        <div className="capacity-text">
          {device.availableBytes > 0
            ? `${formatBytes(device.availableBytes)} free`
            : "Storage info unavailable"}
        </div>
      )}
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
        <select
          value={device.driveSpeed || "slow"}
          onChange={(e) => onSetSpeed(device.id, e.target.value)}
        >
          <option value="slow">Slow</option>
          <option value="fast">Fast</option>
        </select>
        <button onClick={() => onScan(device)} disabled={!device.isConnected}>
          Scan
        </button>
        {onVerify && (
          <button
            onClick={() => onVerify(device)}
            disabled={!device.isConnected || verifyDisabled}
            title="Fully re-hash all indexed files on this drive to detect corruption"
          >
            Verify
          </button>
        )}
        <button className="btn-danger" onClick={() => onRemove(device.id)}>
          Remove
        </button>
      </div>
    </div>
  );
}
