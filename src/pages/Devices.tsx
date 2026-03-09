import { useState } from "react";
import type { StorageDevice } from "../types";
import { useDevices } from "../hooks/useDevices";
import { useNetworkDrives } from "../hooks/useNetworkDrives";
import { DeviceCard } from "../components/DeviceCard";
import { NetworkDriveCard } from "../components/NetworkDriveCard";
import { AddNetworkDriveModal } from "../components/AddNetworkDriveModal";

interface Props {
  onScanDevice: (device: StorageDevice) => void;
}

export function Devices({ onScanDevice }: Props) {
  const { devices, loading, refresh, setType } = useDevices();
  const network = useNetworkDrives();
  const [showAddModal, setShowAddModal] = useState(false);

  return (
    <div className="page">
      <div className="page-header">
        <h1>Devices</h1>
        <div style={{ display: "flex", gap: 8 }}>
          <button onClick={() => setShowAddModal(true)}>Add Network Drive</button>
          <button onClick={() => { refresh(); network.refresh(); }} disabled={loading}>
            {loading ? "Refreshing..." : "Refresh"}
          </button>
        </div>
      </div>

      {network.drives.length > 0 && (
        <>
          <h2 style={{ fontSize: 16, marginBottom: 12, color: "var(--text-muted)" }}>Network Drives</h2>
          <div className="device-grid" style={{ marginBottom: 24 }}>
            {network.drives.map((d) => (
              <NetworkDriveCard
                key={d.id}
                drive={d}
                onMount={network.mount}
                onUnmount={network.unmount}
                onRemove={network.remove}
              />
            ))}
          </div>
        </>
      )}

      <h2 style={{ fontSize: 16, marginBottom: 12, color: "var(--text-muted)" }}>Local Devices</h2>
      <div className="device-grid">
        {devices.map((d) => (
          <DeviceCard key={d.id} device={d} onSetType={setType} onScan={onScanDevice} />
        ))}
      </div>
      {!loading && devices.length === 0 && (
        <p className="empty">No devices detected</p>
      )}

      {showAddModal && (
        <AddNetworkDriveModal
          onAdd={network.add}
          onClose={() => setShowAddModal(false)}
        />
      )}
    </div>
  );
}
