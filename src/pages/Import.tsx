import { useState, useEffect } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useImport } from "../hooks/useImport";
import { detectDevices } from "../api/commands";
import type { StorageDevice } from "../types";
import "./Import.css";

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
}

export function Import() {
  const {
    phase,
    analysis,
    analysisProgress,
    copyProgress,
    errors,
    analyze,
    startCopy,
    cancel,
    eject,
    reset,
  } = useImport();

  const [devices, setDevices] = useState<StorageDevice[]>([]);
  const [selectedSource, setSelectedSource] = useState("");
  const [sourcePath, setSourcePath] = useState("");
  const [selectedTargets, setSelectedTargets] = useState<string[]>([]);

  useEffect(() => {
    detectDevices().then(setDevices);
  }, [phase]);

  const removableDevices = devices.filter(
    (d) => d.isConnected && d.isRemovable
  );
  const targetDevices = devices.filter(
    (d) => d.isConnected && d.id !== analysis?.sdDeviceId
  );

  const toggleTarget = (id: string) => {
    setSelectedTargets((prev) =>
      prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id]
    );
  };

  return (
    <div className="page">
      <h1>Import</h1>

      {errors.length > 0 && (
        <div className="error-msg">{errors[errors.length - 1]}</div>
      )}

      {phase === "idle" && (
        <div className="import-section">
          <div className="form-group">
            <label>Source Device</label>
            <select
              value={selectedSource}
              onChange={(e) => {
                setSelectedSource(e.target.value);
                setSourcePath(e.target.value);
              }}
            >
              <option value="">Select SD card / removable drive...</option>
              {removableDevices.map((d) => (
                <option key={d.id} value={d.mountPoint}>
                  {d.label} (
                  {formatBytes(d.totalBytes - d.availableBytes)} used)
                </option>
              ))}
            </select>
          </div>
          {selectedSource && (
            <div className="form-group">
              <label>Path (narrow to a specific folder)</label>
              <div className="path-input-row">
                <input
                  type="text"
                  value={sourcePath}
                  onChange={(e) => setSourcePath(e.target.value)}
                  placeholder={selectedSource}
                />
                <button
                  onClick={async () => {
                    const selected = await open({
                      directory: true,
                      multiple: false,
                      defaultPath: selectedSource,
                    });
                    if (selected) setSourcePath(selected);
                  }}
                >
                  Browse
                </button>
              </div>
            </div>
          )}
          <button
            className="btn-primary"
            disabled={!sourcePath}
            onClick={() => analyze(sourcePath)}
          >
            Analyze
          </button>
        </div>
      )}

      {phase === "analyzing" && (
        <div className="import-section">
          <div className="progress-container">
            <div className="progress-bar">
              <div
                className="progress-fill"
                style={{
                  width: `${analysisProgress.total ? (analysisProgress.processed / analysisProgress.total) * 100 : 0}%`,
                }}
              />
            </div>
            <div className="progress-stats">
              <span>
                Hashing files... {analysisProgress.processed} /{" "}
                {analysisProgress.total}
              </span>
              <span>
                {analysisProgress.total
                  ? Math.round(
                      (analysisProgress.processed / analysisProgress.total) *
                        100
                    )
                  : 0}
                %
              </span>
            </div>
          </div>
          <button className="btn-danger" onClick={cancel}>
            Cancel
          </button>
        </div>
      )}

      {phase === "reviewed" && analysis && (
        <div className="import-section">
          <div className="import-summary">
            <div className="stats-grid">
              <div className="stat-card">
                <div className="stat-value">{analysis.files.length}</div>
                <div className="stat-label">Total Files</div>
              </div>
              <div className="stat-card">
                <div className="stat-value">{analysis.newFileCount}</div>
                <div className="stat-label">New Files</div>
              </div>
              <div className="stat-card">
                <div className="stat-value">
                  {analysis.existingFileCount}
                </div>
                <div className="stat-label">Already Backed Up</div>
              </div>
              <div className="stat-card">
                <div className="stat-value">
                  {formatBytes(analysis.totalBytes)}
                </div>
                <div className="stat-label">Total Size</div>
              </div>
            </div>
          </div>

          <div className="import-files-preview">
            <h3>Files ({analysis.files.length})</h3>
            <div className="import-file-list">
              {analysis.files.slice(0, 100).map((f, i) => (
                <div key={i} className="import-file-row">
                  <span className="import-file-name">{f.fileName}</span>
                  <span className="import-file-size">
                    {formatBytes(f.fileSize)}
                  </span>
                  <span className="import-file-date">{f.createdDate}</span>
                  {f.existingLocations.length > 0 ? (
                    <span className="badge badge-safe">
                      {f.existingLocations.length} backup
                      {f.existingLocations.length > 1 ? "s" : ""}
                    </span>
                  ) : (
                    <span className="badge badge-warn">New</span>
                  )}
                </div>
              ))}
              {analysis.files.length > 100 && (
                <div
                  className="import-file-row text-center text-muted-color"
                >
                  ...and {analysis.files.length - 100} more files
                </div>
              )}
            </div>
          </div>

          <div className="import-targets">
            <h3>Copy To</h3>
            <div className="target-device-list">
              {targetDevices.map((d) => {
                const needed = analysis.files
                  .filter(
                    (f) =>
                      !f.existingLocations.some((l) => l.deviceId === d.id)
                  )
                  .reduce((sum, f) => sum + f.fileSize, 0);
                const fits = needed <= d.availableBytes;
                return (
                  <label
                    key={d.id}
                    className={`target-device-option ${!fits ? "no-fit" : ""}`}
                  >
                    <input
                      type="checkbox"
                      checked={selectedTargets.includes(d.id)}
                      onChange={() => toggleTarget(d.id)}
                      disabled={!fits}
                    />
                    <div className="target-device-info">
                      <span className="target-device-name">{d.label}</span>
                      <span className="target-device-space">
                        {formatBytes(d.availableBytes)} free
                        {needed > 0 && ` · ${formatBytes(needed)} needed`}
                        {!fits && " · Not enough space"}
                      </span>
                    </div>
                    <span
                      className="device-type-badge"
                      style={{
                        background:
                          d.deviceType === "hot"
                            ? "var(--danger)"
                            : d.deviceType === "cold"
                              ? "var(--accent)"
                              : "var(--text-muted)",
                      }}
                    >
                      {d.deviceType}
                    </span>
                  </label>
                );
              })}
            </div>
          </div>

          <div className="import-actions">
            <button onClick={reset}>Back</button>
            <button
              className="btn-primary"
              disabled={selectedTargets.length === 0}
              onClick={() => startCopy(selectedTargets)}
            >
              Start Import
            </button>
          </div>
        </div>
      )}

      {phase === "copying" && (
        <div className="import-section">
          <h3>Copying...</h3>
          {Object.values(copyProgress).map((p) => (
            <div key={p.deviceId} className="progress-container">
              <div className="progress-label">{p.deviceLabel}</div>
              <div className="progress-bar">
                <div
                  className="progress-fill"
                  style={{
                    width: `${p.totalBytes ? (p.bytesCopied / p.totalBytes) * 100 : 100}%`,
                  }}
                />
              </div>
              <div className="progress-stats">
                <span>
                  {p.filesCopied} / {p.totalFiles} files
                </span>
                <span>
                  {formatBytes(p.bytesCopied)} / {formatBytes(p.totalBytes)}
                </span>
              </div>
              <div className="progress-file">{p.currentFile}</div>
            </div>
          ))}
          <button className="btn-danger" onClick={cancel}>
            Cancel
          </button>
        </div>
      )}

      {phase === "complete" && analysis && (
        <div className="import-section">
          <div className="scan-result">
            <span>Import complete!</span>
          </div>
          <div className="import-actions mt-16">
            <button
              onClick={() =>
                eject(
                  devices.find((d) => d.id === analysis.sdDeviceId)
                    ?.mountPoint || ""
                )
              }
            >
              Eject SD Card
            </button>
            <button className="btn-primary" onClick={reset}>
              Import Another
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
