import { useState } from "react";
import type { StorageDevice } from "../types";
import { useDevices } from "../hooks/useDevices";
import { useNetworkDrives } from "../hooks/useNetworkDrives";
import { useVerify } from "../hooks/useVerify";
import { DeviceCard } from "../components/DeviceCard";
import { NetworkDriveCard } from "../components/NetworkDriveCard";
import { AddLocationModal } from "../components/AddLocationModal";
import { AddNetworkDriveModal } from "../components/AddNetworkDriveModal";
import { addLocation } from "../api/commands";
import "./Devices.css";

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
  const { devices, loading, refresh, setType, setSpeed, remove } = useDevices();
  const network = useNetworkDrives();
  const verify = useVerify();
  const [showAddModal, setShowAddModal] = useState(false);
  const [showAddNetworkModal, setShowAddNetworkModal] = useState(false);

  const verifyingDevice = devices.find((d) => d.id === verify.deviceId);
  const verifyPct =
    verify.progress.total > 0
      ? Math.round((verify.progress.processed / verify.progress.total) * 100)
      : 0;

  const handleAdd = async (path: string, label: string, deviceType: string) => {
    await addLocation(path, label, deviceType);
    refresh();
  };

  const handleRemove = (deviceId: string) => {
    const device = devices.find((d) => d.id === deviceId);
    if (
      !confirm(
        `Remove "${device?.label ?? deviceId}"? All indexed file records for this device will be deleted.`
      )
    )
      return;
    remove(deviceId);
  };

  const grouped = new Map<string, StorageDevice[]>();
  for (const section of SECTIONS) grouped.set(section.key, []);
  for (const d of devices) {
    const key = grouped.has(d.deviceType) ? d.deviceType : "unknown";
    grouped.get(key)!.push(d);
  }

  return (
    <div className="page">
      <div className="page-header">
        <h1>Devices</h1>
        <div className="flex-row gap-8">
          <button onClick={() => setShowAddModal(true)}>Add Location</button>
          <button onClick={() => setShowAddNetworkModal(true)}>Add Network Drive</button>
          <button onClick={() => { refresh(); network.refresh(); }} disabled={loading}>
            {loading ? "Refreshing..." : "Refresh"}
          </button>
        </div>
      </div>

      {verify.phase === "running" && (
        <div className="progress-container">
          <div className="progress-bar">
            <div className="progress-fill" style={{ width: `${verifyPct}%` }} />
          </div>
          <div className="progress-stats">
            <span>
              Verifying {verifyingDevice?.label ?? ""}... {verify.progress.processed} /{" "}
              {verify.progress.total} files
            </span>
            <button className="btn-danger" onClick={verify.cancel}>Cancel</button>
          </div>
          {verify.progress.currentFile && (
            <div className="progress-file">{verify.progress.currentFile}</div>
          )}
        </div>
      )}

      {verify.phase === "done" && verify.summary && (
        <div className={verify.summary.corrupted > 0 ? "error-msg" : "success-msg"}>
          <div className="flex-row gap-8" style={{ justifyContent: "space-between", alignItems: "center" }}>
            <span>
              Verification of {verifyingDevice?.label ?? "drive"} complete:{" "}
              {verify.summary.verified} verified, {verify.summary.baselined} baselined
              {verify.summary.modified > 0 && `, ${verify.summary.modified} modified`}
              {verify.summary.missing > 0 && `, ${verify.summary.missing} missing`}
              {verify.summary.corrupted > 0 && `, ${verify.summary.corrupted} POSSIBLY CORRUPTED`}
            </span>
            <button onClick={verify.reset}>Dismiss</button>
          </div>
          {verify.corrupted.length > 0 && (
            <div className="bulk-delete-file-list" style={{ marginTop: 8 }}>
              {verify.corrupted.map((c) => (
                <div key={c.locationId} className="bulk-delete-file-item">
                  <span className="bulk-delete-file-path">{c.filePath}</span>
                  <span className="text-xs">content changed, but size and date did not</span>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {verify.errors.length > 0 && verify.phase !== "done" && (
        <div className="error-msg">{verify.errors[verify.errors.length - 1]}</div>
      )}

      {network.drives.length > 0 && (
        <div className="mb-24">
          <h2 className="text-base mb-12 text-muted-color">Network Drives</h2>
          <div className="device-grid">
            {network.drives.map((d) => (
              <NetworkDriveCard
                key={d.id}
                drive={d}
                onMount={network.mount}
                onUnmount={network.unmount}
                onScan={(drive) =>
                  onScanDevice({
                    id: drive.id,
                    label: drive.label,
                    mountPoint: drive.mountPoint,
                    deviceType: drive.deviceType,
                    totalBytes: 0,
                    availableBytes: 0,
                    isRemovable: false,
                    firstSeen: "",
                    lastSeen: "",
                    driveSpeed: "slow",
                    isConnected: true,
                  })
                }
                onRemove={network.remove}
              />
            ))}
          </div>
        </div>
      )}

      {loading && devices.length === 0 ? (
        <div className="device-grid">
          {[...Array(3)].map((_, i) => <div key={i} className="skeleton skeleton-card" />)}
        </div>
      ) : SECTIONS.map(({ key, label }) => {
        const sectionDevices = grouped.get(key)!;
        if (sectionDevices.length === 0) return null;
        return (
          <div key={key} className="mb-24">
            <h2 className="text-base mb-12 text-muted-color">{label}</h2>
            <div className="device-grid">
              {sectionDevices.map((d) => (
                <DeviceCard
                  key={d.id}
                  device={d}
                  onSetType={setType}
                  onSetSpeed={setSpeed}
                  onScan={onScanDevice}
                  onVerify={(dev) => verify.verify(dev.id, dev.label)}
                  verifyDisabled={verify.phase === "running"}
                  onRemove={handleRemove}
                />
              ))}
            </div>
          </div>
        );
      })}

      {!loading && devices.length === 0 && network.drives.length === 0 && (
        <p className="empty">No devices detected</p>
      )}

      {showAddModal && (
        <AddLocationModal
          onAdd={handleAdd}
          onClose={() => setShowAddModal(false)}
        />
      )}

      {showAddNetworkModal && (
        <AddNetworkDriveModal
          onAdd={network.add}
          onClose={() => setShowAddNetworkModal(false)}
        />
      )}
    </div>
  );
}
