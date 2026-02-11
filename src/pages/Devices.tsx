import type { StorageDevice } from "../types";
import { useDevices } from "../hooks/useDevices";
import { DeviceCard } from "../components/DeviceCard";

interface Props {
  onScanDevice: (device: StorageDevice) => void;
}

export function Devices({ onScanDevice }: Props) {
  const { devices, loading, refresh, setType } = useDevices();

  return (
    <div className="page">
      <div className="page-header">
        <h1>Devices</h1>
        <button onClick={refresh} disabled={loading}>
          {loading ? "Refreshing..." : "Refresh"}
        </button>
      </div>
      <div className="device-grid">
        {devices.map((d) => (
          <DeviceCard key={d.id} device={d} onSetType={setType} onScan={onScanDevice} />
        ))}
      </div>
      {!loading && devices.length === 0 && (
        <p className="empty">No devices detected</p>
      )}
    </div>
  );
}
