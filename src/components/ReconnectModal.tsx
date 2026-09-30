import { useState } from "react";
import type { ReconnectCheck, StorageDevice } from "../types";
import { reconnectDevice } from "../api/commands";
import { useFocusTrap } from "../hooks/useFocusTrap";

interface Props {
  device: StorageDevice;
  path: string;
  check: ReconnectCheck;
  onDone: () => void;
  onClose: () => void;
}

export function ReconnectModal({ device, path, check, onDone, onClose }: Props) {
  const trapRef = useFocusTrap<HTMLDivElement>();
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");

  const cleanMatch = check.markerStatus === "match";
  const markerLine =
    check.markerStatus === "match"
      ? "✓ ID marker matches this device"
      : check.markerStatus === "missing"
        ? "⚠ No ID marker at this location — it will be created"
        : `✕ Folder is marked as a different device (${check.foreignId})`;
  const sampleLine =
    check.sampledFiles === 0
      ? "No indexed files to verify (device was never scanned)"
      : `${check.foundFiles} of ${check.sampledFiles} known files found at this location`;
  const sampleOk = check.sampledFiles === 0 || check.foundFiles === check.sampledFiles;

  const handleConnect = async () => {
    setSubmitting(true);
    setError("");
    try {
      await reconnectDevice(device.id, path);
      onDone();
    } catch (e) {
      setError(String(e));
      setSubmitting(false);
    }
  };

  return (
    <div
      className="modal-overlay"
      onClick={onClose}
      role="dialog"
      aria-label="Reconnect device"
      aria-modal="true"
      onKeyDown={(e) => {
        if (e.key === "Escape") onClose();
      }}
    >
      <div className="modal-content" ref={trapRef} onClick={(e) => e.stopPropagation()}>
        <h2>Reconnect {device.label}</h2>
        {error && <div className="error-msg">{error}</div>}
        <div className="form-group">
          <label>Old path</label>
          <div className="device-mount">{device.mountPoint}</div>
        </div>
        <div className="form-group">
          <label>New path</label>
          <div className="device-mount">{path}</div>
        </div>
        <div className={cleanMatch ? "success-msg" : "error-msg"}>{markerLine}</div>
        <div className={sampleOk ? "success-msg" : "error-msg"}>{sampleLine}</div>
        <div className="form-actions">
          <button onClick={onClose}>Cancel</button>
          <button className="btn-primary" onClick={handleConnect} disabled={submitting}>
            {submitting
              ? "Connecting..."
              : cleanMatch && sampleOk
                ? "Connect"
                : "Connect anyway"}
          </button>
        </div>
      </div>
    </div>
  );
}
