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

const SECTIONS = [
  { key: "hot", label: "Hot Storage" },
  { key: "cold", label: "Cold Storage" },
  { key: "production", label: "Production" },
  { key: "unknown", label: "Uncategorized" },
] as const;

export function Devices({ onScanDevice }: Props) {
  const { devices, loading, refresh, setType, remove } = useDevices();
  const network = useNetworkDrives();
  const [showAddModal, setShowAddModal] = useState(false);

  const grouped = new Map<string, StorageDevice[]>();
  for (const section of SECTIONS) grouped.set(section.key, []);
  for (const d of devices) {
    const key = grouped.has(d.device_type) ? d.device_type : "unknown";
    grouped.get(key)!.push(d);
  }

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

      {SECTIONS.map(({ key, label }) => {
        const sectionDevices = grouped.get(key)!;
        if (sectionDevices.length === 0) return null;
        return (
          <div key={key} style={{ marginBottom: 24 }}>
            <h2 style={{ fontSize: 16, marginBottom: 12, color: "var(--text-muted)" }}>{label}</h2>
            <div className="device-grid">
              {sectionDevices.map((d) => (
                <DeviceCard key={d.id} device={d} onSetType={setType} onScan={onScanDevice} onRemove={remove} />
              ))}
            </div>
          </div>
        );
      })}

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
