import { useState, useEffect } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useImport } from "../hooks/useImport";
import { detectDevices } from "../api/commands";
import type { StorageDevice } from "../types";

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
    (d) => d.is_connected && d.is_removable
  );
  const targetDevices = devices.filter(
    (d) => d.is_connected && d.id !== analysis?.sd_device_id
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
                <option key={d.id} value={d.mount_point}>
                  {d.label} (
                  {formatBytes(d.total_bytes - d.available_bytes)} used)
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
                <div className="stat-value">{analysis.new_file_count}</div>
                <div className="stat-label">New Files</div>
              </div>
              <div className="stat-card">
                <div className="stat-value">
                  {analysis.existing_file_count}
                </div>
                <div className="stat-label">Already Backed Up</div>
              </div>
              <div className="stat-card">
                <div className="stat-value">
                  {formatBytes(analysis.total_bytes)}
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
                  <span className="import-file-name">{f.file_name}</span>
                  <span className="import-file-size">
                    {formatBytes(f.file_size)}
                  </span>
                  <span className="import-file-date">{f.created_date}</span>
                  {f.existing_locations.length > 0 ? (
                    <span className="badge badge-safe">
                      {f.existing_locations.length} backup
                      {f.existing_locations.length > 1 ? "s" : ""}
                    </span>
                  ) : (
                    <span className="badge badge-warn">New</span>
                  )}
                </div>
              ))}
              {analysis.files.length > 100 && (
                <div
                  className="import-file-row"
                  style={{
                    justifyContent: "center",
                    color: "var(--text-muted)",
                  }}
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
                      !f.existing_locations.some((l) => l.device_id === d.id)
                  )
                  .reduce((sum, f) => sum + f.file_size, 0);
                const fits = needed <= d.available_bytes;
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
                        {formatBytes(d.available_bytes)} free
                        {needed > 0 && ` · ${formatBytes(needed)} needed`}
                        {!fits && " · Not enough space"}
                      </span>
                    </div>
                    <span
                      className="device-type-badge"
                      style={{
                        background:
                          d.device_type === "hot"
                            ? "var(--danger)"
                            : d.device_type === "cold"
                              ? "var(--accent)"
                              : "var(--text-muted)",
                      }}
                    >
                      {d.device_type}
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
            <div key={p.device_id} className="progress-container">
              <div className="progress-label">{p.device_label}</div>
              <div className="progress-bar">
                <div
                  className="progress-fill"
                  style={{
                    width: `${p.total_bytes ? (p.bytes_copied / p.total_bytes) * 100 : 100}%`,
                  }}
                />
              </div>
              <div className="progress-stats">
                <span>
                  {p.files_copied} / {p.total_files} files
                </span>
                <span>
                  {formatBytes(p.bytes_copied)} / {formatBytes(p.total_bytes)}
                </span>
              </div>
              <div className="progress-file">{p.current_file}</div>
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
          <div className="import-actions" style={{ marginTop: 16 }}>
            <button
              onClick={() =>
                eject(
                  devices.find((d) => d.id === analysis.sd_device_id)
                    ?.mount_point || ""
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
