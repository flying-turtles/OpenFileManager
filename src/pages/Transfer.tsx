import { useEffect, useRef } from "react";
import { useTransfer } from "../hooks/useTransfer";
import { useDevices } from "../hooks/useDevices";
import { formatBytes } from "../utils/format";

interface Props {
  project: { id: number; title: string } | null;
}

export function Transfer({ project }: Props) {
  const {
    phase,
    projectTitle: hookTitle,
    selectedDeviceId,
    check,
    progress,
    errors,
    setProject,
    startCheck,
    continueWithAvailable,
    backToDeviceSelect,
    cancel,
    reset,
  } = useTransfer();

  const { devices } = useDevices();
  const handledRequest = useRef<{ id: number; title: string } | null>(null);

  // New object identity per "Transfer to..." click, so re-selecting the
  // same project after a reset still re-enters device selection. Deferred
  // while a copy is running so it can't wipe an active transfer's UI.
  useEffect(() => {
    if (project && project !== handledRequest.current && phase !== "copying") {
      handledRequest.current = project;
      setProject(project.id, project.title);
    }
  }, [project, phase, setProject]);

  const connectedDevices = devices.filter((d) => d.isConnected);
  const selectedDev = connectedDevices.find((d) => d.id === selectedDeviceId);
  const pct = progress
    ? Math.round((progress.bytesCopied / Math.max(progress.totalBytes, 1)) * 100)
    : 0;

  return (
    <div className="page">
      <h1>Transfer</h1>

      {phase === "idle" && (
        <p className="empty">Select a project to transfer from the Projects page</p>
      )}

      {phase === "select-device" && (
        <div className="import-section">
          <h3>Transfer "{hookTitle}"</h3>
          <p className="text-muted-color text-sm" style={{ marginBottom: 16 }}>
            Select a target drive to copy all project files to.
          </p>
          {connectedDevices.length === 0 ? (
            <p className="empty">No connected devices</p>
          ) : (
            <div className="bulk-delete-file-list">
              {connectedDevices.map((d) => (
                <div
                  key={d.id}
                  className="bulk-delete-file-item"
                  style={{
                    cursor: "pointer",
                    display: "flex",
                    flexDirection: "row",
                    justifyContent: "space-between",
                    alignItems: "center",
                    padding: "8px 12px",
                  }}
                  onClick={() => startCheck(d.id)}
                >
                  <div>
                    <strong>{d.label}</strong>
                    <div className="text-muted-color text-xs">{d.mountPoint}</div>
                  </div>
                  <div className="text-muted-color text-xs">
                    {d.availableBytes > 0 ? `${formatBytes(d.availableBytes)} free` : ""}
                    {d.driveSpeed === "fast" ? " · Fast" : ""}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {phase === "unavailable" && check && (
        <div className="import-section">
          <h3>Some Files Unavailable</h3>
          <p className="text-sm">
            {check.availableCount} file{check.availableCount !== 1 ? "s" : ""} ready to transfer.
            {check.alreadyOnTarget > 0 && ` ${check.alreadyOnTarget} already on target.`}
          </p>
          <p className="bulk-delete-warning">
            {check.unavailableFiles.length} file
            {check.unavailableFiles.length !== 1 ? "s" : ""} cannot be copied (source drive not
            connected):
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
            <button onClick={backToDeviceSelect}>Back</button>
            <button className="btn-primary" onClick={continueWithAvailable}>
              Continue ({check.availableCount} files)
            </button>
          </div>
        </div>
      )}

      {phase === "copying" && (
        <div className="import-section">
          <h3>Transferring to {selectedDev?.label ?? "..."}...</h3>
          {errors.length > 0 && (
            <div className="error-msg" style={{ marginBottom: 12 }}>
              {errors.length} error{errors.length !== 1 ? "s" : ""} during transfer
            </div>
          )}
          <div className="progress-container">
            <div className="progress-bar">
              <div className="progress-fill" style={{ width: `${pct}%` }} />
            </div>
            <div className="progress-stats">
              <span>
                {progress ? `${progress.filesCopied} / ${progress.totalFiles} files` : "Starting..."}
              </span>
              <span>{pct}%</span>
            </div>
            {progress && (
              <div className="progress-stats">
                <span>
                  {formatBytes(progress.bytesCopied)} / {formatBytes(progress.totalBytes)}
                </span>
              </div>
            )}
            {progress?.currentFile && <div className="progress-file">{progress.currentFile}</div>}
          </div>
          <div className="form-actions">
            <button className="btn-danger" onClick={cancel}>
              Cancel Transfer
            </button>
          </div>
        </div>
      )}

      {phase === "complete" && (
        <div className="import-section">
          <h3>Transfer Complete</h3>
          {progress && (
            <div className="stats-grid" style={{ marginBottom: 16 }}>
              <div className="stat-card">
                <div className="stat-value">{progress.filesCopied}</div>
                <div className="stat-label">Files Copied</div>
              </div>
              <div className="stat-card">
                <div className="stat-value">{formatBytes(progress.bytesCopied)}</div>
                <div className="stat-label">Data Transferred</div>
              </div>
              <div className="stat-card">
                <div className="stat-value">{selectedDev?.label ?? ""}</div>
                <div className="stat-label">Target</div>
              </div>
            </div>
          )}
          {!progress && check && check.availableCount === 0 && (
            <p>All files already exist on the target drive.</p>
          )}
          {errors.length > 0 && (
            <>
              <p className="bulk-delete-warning">
                {errors.length} error{errors.length !== 1 ? "s" : ""}:
              </p>
              <div className="bulk-delete-file-list" style={{ maxHeight: 150 }}>
                {errors.map((e, i) => (
                  <div key={i} className="bulk-delete-file-item text-xs">
                    {e}
                  </div>
                ))}
              </div>
            </>
          )}
          <div className="form-actions">
            <button className="btn-primary" onClick={reset}>
              Transfer Another
            </button>
          </div>
        </div>
      )}

      {phase === "error" && (
        <div className="import-section">
          <h3>Transfer Failed</h3>
          {errors.map((e, i) => (
            <p key={i} className="bulk-delete-warning">
              {e}
            </p>
          ))}
          <div className="form-actions">
            <button onClick={reset}>Back</button>
          </div>
        </div>
      )}
    </div>
  );
}
