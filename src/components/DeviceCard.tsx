import type { StorageDevice } from "../types";

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
}

export function DeviceCard({ device, onSetType, onScan }: Props) {
  const usedBytes = device.total_bytes - device.available_bytes;
  const usedPct = device.total_bytes > 0 ? (usedBytes / device.total_bytes) * 100 : 0;

  const typeColors: Record<string, string> = {
    hot: "#e74c3c",
    cold: "#3498db",
    unknown: "#95a5a6",
  };

  return (
    <div className="device-card">
      <div className="device-header">
        <h3>{device.label}</h3>
        <span
          className="device-type-badge"
          style={{ backgroundColor: typeColors[device.device_type] || "#95a5a6" }}
        >
          {device.device_type}
        </span>
      </div>
      <div className="device-mount">{device.mount_point}</div>
      <div className="capacity-bar">
        <div className="capacity-used" style={{ width: `${usedPct}%` }} />
      </div>
      <div className="capacity-text">
        {formatBytes(usedBytes)} / {formatBytes(device.total_bytes)}
      </div>
      {device.is_removable && <div className="removable-tag">Removable</div>}
      <div className="device-actions">
        <select
          value={device.device_type}
          onChange={(e) => onSetType(device.id, e.target.value)}
        >
          <option value="unknown">Unknown</option>
          <option value="hot">Hot</option>
          <option value="cold">Cold</option>
        </select>
        <button onClick={() => onScan(device)}>Scan</button>
      </div>
    </div>
  );
}
