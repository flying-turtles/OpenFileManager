import { useState } from "react";
import { useFocusTrap } from "../hooks/useFocusTrap";
import type { StorageDevice, TransferCheck, TransferEvent, DeviceCopyProgress } from "../types";
import { startProjectTransfer, cancelProjectTransfer } from "../api/commands";

function formatBytes(bytes: number): string {
  if (!bytes || bytes <= 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
}

interface Props {
  projectId: number;
  projectTitle: string;
  devices: StorageDevice[];
  onClose: () => void;
}

type Phase = "select" | "unavailable" | "copying" | "complete" | "error";

export function TransferModal({ projectId, projectTitle, devices, onClose }: Props) {
  const trapRef = useFocusTrap<HTMLDivElement>();
  const [phase, setPhase] = useState<Phase>("select");
  const [selectedDevice, setSelectedDevice] = useState<string>("");
  const [check, setCheck] = useState<TransferCheck | null>(null);
  const [progress, setProgress] = useState<DeviceCopyProgress | null>(null);
  const [errors, setErrors] = useState<string[]>([]);
  const [copying, setCopying] = useState(false);

  const connectedDevices = devices.filter((d) => d.isConnected);

  const handleEvent = (event: TransferEvent) => {
    if (event === "CopyComplete") {
      setCopying(false);
      setPhase("complete");
    } else if (event === "Cancelled") {
      setCopying(false);
      setPhase("complete");
    } else if (typeof event === "object" && "CheckComplete" in event) {
      const c = event.CheckComplete;
      setCheck(c);
      if (c.unavailableFiles.length > 0) {
        setPhase("unavailable");
      } else if (c.availableCount > 0) {
        setCopying(true);
        setPhase("copying");
      } else {
        setPhase("complete");
      }
    } else if (typeof event === "object" && "CopyStarted" in event) {
      setCopying(true);
      setPhase("copying");
    } else if (typeof event === "object" && "CopyProgress" in event) {
      setProgress(event.CopyProgress);
    } else if (typeof event === "object" && "Error" in event) {
      setErrors((prev) => [...prev, event.Error.message]);
    }
  };

  const startCheck = async (deviceId: string) => {
    setSelectedDevice(deviceId);
    setErrors([]);
    try {
      await startProjectTransfer(projectId, deviceId, false, handleEvent);
    } catch (e: any) {
      setErrors([e.toString()]);
      setPhase("error");
    }
  };

  const continueWithAvailable = async () => {
    setErrors([]);
    setCopying(true);
    setPhase("copying");
    try {
      await startProjectTransfer(projectId, selectedDevice, true, handleEvent);
    } catch (e: any) {
      setErrors([e.toString()]);
      setPhase("error");
    }
  };

  const handleCancel = async () => {
    await cancelProjectTransfer();
  };

  const selectedDev = connectedDevices.find((d) => d.id === selectedDevice);
  const pct = progress ? Math.round((progress.bytesCopied / Math.max(progress.totalBytes, 1)) * 100) : 0;

  return (
    <div className="modal-overlay" onClick={() => !copying && onClose()} role="dialog" aria-label="Transfer project" aria-modal="true">
      <div className="modal-content" ref={trapRef} onClick={(e) => e.stopPropagation()} style={{ maxWidth: 560 }}>
        {phase === "select" && (
          <>
            <h2>Transfer "{projectTitle}"</h2>
            <p className="text-muted-color text-sm">Select a target drive to copy all project files to.</p>
            {connectedDevices.length === 0 ? (
              <p className="empty">No connected devices</p>
            ) : (
              <div className="bulk-delete-file-list">
                {connectedDevices.map((d) => (
                  <div
                    key={d.id}
                    className="bulk-delete-file-item"
                    style={{ cursor: "pointer", display: "flex", justifyContent: "space-between", alignItems: "center", padding: "8px 12px" }}
                    onClick={() => startCheck(d.id)}
                  >
                    <div>
                      <strong>{d.label}</strong>
                      <div className="text-muted-color text-xs">{d.mountPoint}</div>
                    </div>
                    <div className="text-muted-color text-xs">
                      {d.availableBytes > 0 ? `${formatBytes(d.availableBytes)} free` : ""}
                      {d.driveSpeed === "fast" ? " \u00B7 Fast" : ""}
                    </div>
                  </div>
                ))}
              </div>
            )}
            <div className="form-actions">
              <button onClick={onClose}>Cancel</button>
            </div>
          </>
        )}

        {phase === "unavailable" && check && (
          <>
            <h2>Some Files Unavailable</h2>
            <p className="text-sm">
              {check.availableCount} file{check.availableCount !== 1 ? "s" : ""} ready to transfer.
              {check.alreadyOnTarget > 0 && ` ${check.alreadyOnTarget} already on target.`}
            </p>
            <p className="bulk-delete-warning">
              {check.unavailableFiles.length} file{check.unavailableFiles.length !== 1 ? "s" : ""} cannot be copied (source drive not connected):
            </p>
            <div className="bulk-delete-file-list" style={{ maxHeight: 200 }}>
              {check.unavailableFiles.map((f) => (
                <div key={f.blake3Hash} className="bulk-delete-file-item">
                  <span className="bulk-delete-file-path">{f.representativeName}</span>
                  <span className="text-muted-color text-xs">{formatBytes(f.fileSize)}</span>
                </div>
              ))}
            </div>
            <div className="form-actions">
              <button onClick={onClose}>Cancel</button>
              <button className="btn-primary" onClick={continueWithAvailable}>
                Continue ({check.availableCount} files)
              </button>
            </div>
          </>
        )}

        {phase === "copying" && (
          <>
            <h2>Transferring to {selectedDev?.label}...</h2>
            <div className="progress-container">
              <div className="progress-bar">
                <div className="progress-fill" style={{ width: `${pct}%` }} />
              </div>
              <div className="progress-stats">
                <span>{progress ? `${progress.filesCopied} / ${progress.totalFiles} files` : "Starting..."}</span>
                <span>{pct}%</span>
              </div>
              {progress && (
                <div className="progress-stats">
                  <span>{formatBytes(progress.bytesCopied)} / {formatBytes(progress.totalBytes)}</span>
                </div>
              )}
              {progress?.currentFile && <div className="progress-file">{progress.currentFile}</div>}
            </div>
            {errors.length > 0 && (
              <div className="bulk-delete-warning text-xs" style={{ marginTop: 8 }}>
                {errors.length} error{errors.length !== 1 ? "s" : ""} during transfer
              </div>
            )}
            <div className="form-actions">
              <button className="btn-danger" onClick={handleCancel}>Cancel Transfer</button>
            </div>
          </>
        )}

        {phase === "complete" && (
          <>
            <h2>Transfer Complete</h2>
            {progress && (
              <p>{progress.filesCopied} file{progress.filesCopied !== 1 ? "s" : ""} ({formatBytes(progress.bytesCopied)}) transferred to {selectedDev?.label}.</p>
            )}
            {!progress && check && check.availableCount === 0 && (
              <p>All files already exist on the target drive.</p>
            )}
            {errors.length > 0 && (
              <>
                <p className="bulk-delete-warning">{errors.length} error{errors.length !== 1 ? "s" : ""}:</p>
                <div className="bulk-delete-file-list" style={{ maxHeight: 150 }}>
                  {errors.map((e, i) => (
                    <div key={i} className="bulk-delete-file-item text-xs">{e}</div>
                  ))}
                </div>
              </>
            )}
            <div className="form-actions">
              <button className="btn-primary" onClick={onClose}>Close</button>
            </div>
          </>
        )}

        {phase === "error" && (
          <>
            <h2>Transfer Failed</h2>
            {errors.map((e, i) => (
              <p key={i} className="bulk-delete-warning">{e}</p>
            ))}
            <div className="form-actions">
              <button onClick={onClose}>Close</button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
