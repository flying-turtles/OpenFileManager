import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useImport } from "../hooks/useImport";
import { useDevices } from "../hooks/useDevices";
import { formatBytes } from "../utils/format";
import "./Import.css";

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

  const { devices } = useDevices();
  const [selectedSource, setSelectedSource] = useState("");
  const [sourcePath, setSourcePath] = useState("");
  const [selectedTargets, setSelectedTargets] = useState<string[]>([]);

  const removableDevices = devices.filter(
    (d) => d.isConnected && d.mountPoint !== "/"
  );
  const targetDevices = devices.filter(
    (d) => d.isConnected && d.id !== analysis?.sdDeviceId
  );
  const deviceNames: Record<string, string> = {};
  for (const d of devices) deviceNames[d.id] = d.label;

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
                    <span className="badge badge-safe" title={f.existingLocations.map((l) => deviceNames[l.deviceId] || l.deviceId).join(", ")}>
                      {f.existingLocations.map((l) => deviceNames[l.deviceId] || l.deviceId).join(", ")}
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
                const storageUnknown = d.totalBytes === 0 && d.availableBytes === 0;
                const fits = storageUnknown || needed <= d.availableBytes;
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
                        {storageUnknown
                          ? "Storage info unavailable"
                          : `${formatBytes(d.availableBytes)} free`}
                        {!storageUnknown && needed > 0 && ` · ${formatBytes(needed)} needed`}
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
          <h3>Importing to {selectedTargets.length} device{selectedTargets.length !== 1 ? "s" : ""}...</h3>
          {errors.length > 0 && (
            <div className="error-msg" style={{ marginBottom: 12 }}>
              {errors[errors.length - 1]}
            </div>
          )}
          {selectedTargets.map((id) => {
            const p = copyProgress[id];
            const label = deviceNames[id] || id;
            if (p) {
              const pct = p.totalBytes ? (p.bytesCopied / p.totalBytes) * 100 : 0;
              return (
                <div key={id} className="progress-container">
                  <div className="progress-label">{p.deviceLabel}</div>
                  <div className="progress-bar">
                    <div className="progress-fill" style={{ width: `${pct}%` }} />
                  </div>
                  <div className="progress-stats">
                    <span>{p.filesCopied} / {p.totalFiles} files</span>
                    <span>{formatBytes(p.bytesCopied)} / {formatBytes(p.totalBytes)}</span>
                  </div>
                  <div className="progress-file">{p.currentFile}</div>
                </div>
              );
            }
            return (
              <div key={id} className="progress-container">
                <div className="progress-label">{label}</div>
                <div className="progress-bar">
                  <div className="progress-fill" style={{ width: "0%" }} />
                </div>
                <div className="progress-stats">
                  <span>Waiting...</span>
                </div>
              </div>
            );
          })}
          <button className="btn-danger" onClick={cancel}>
            Cancel
          </button>
        </div>
      )}

      {phase === "complete" && analysis && (() => {
        const sourceDevice = devices.find((d) => d.id === analysis.sdDeviceId);
        const totalCopied = Object.values(copyProgress);
        const totalFilesCopied = totalCopied.reduce((s, p) => s + Number(p.filesCopied), 0);
        const totalBytesCopied = totalCopied.reduce((s, p) => s + p.bytesCopied, 0);
        const newBefore = analysis.files.filter((f) => f.existingLocations.length === 0).length;
        const copiedToNames = selectedTargets.map((id) => deviceNames[id] || id);
        // Post-import: each file's backup count = pre-existing + number of targets it was copied to
        const backupCounts = analysis.files.map((f) => {
          const preExisting = f.existingLocations.length;
          // Files already on a target weren't copied there again
          const newCopies = selectedTargets.filter(
            (tid) => !f.existingLocations.some((l) => l.deviceId === tid)
          ).length;
          return { fileName: f.fileName, total: preExisting + newCopies, locations: [
            ...f.existingLocations.map((l) => deviceNames[l.deviceId] || l.deviceId),
            ...selectedTargets.filter((tid) => !f.existingLocations.some((l) => l.deviceId === tid)).map((tid) => deviceNames[tid] || tid),
          ]};
        });
        const fullyBacked = backupCounts.filter((f) => f.total >= 2).length;
        const singleCopy = backupCounts.filter((f) => f.total === 1).length;
        const noCopy = backupCounts.filter((f) => f.total === 0).length;

        return (
        <div className="import-section">
          <h3>Import Complete</h3>
          <div className="stats-grid">
            <div className="stat-card">
              <div className="stat-value">{totalFilesCopied}</div>
              <div className="stat-label">Files Copied</div>
            </div>
            <div className="stat-card">
              <div className="stat-value">{formatBytes(totalBytesCopied)}</div>
              <div className="stat-label">Data Copied</div>
            </div>
            <div className="stat-card">
              <div className="stat-value">{copiedToNames.length}</div>
              <div className="stat-label">Target{copiedToNames.length !== 1 ? "s" : ""}</div>
            </div>
            <div className="stat-card">
              <div className="stat-value">{newBefore}</div>
              <div className="stat-label">New Files</div>
            </div>
          </div>

          <div style={{ marginTop: 16 }}>
            <h3>Copied To</h3>
            {totalCopied.map((p) => (
              <div key={p.deviceId} className="import-file-row">
                <span className="import-file-name">{p.deviceLabel}</span>
                <span className="import-file-size">{p.filesCopied} files</span>
                <span className="import-file-size">{formatBytes(p.bytesCopied)}</span>
              </div>
            ))}
          </div>

          <div style={{ marginTop: 16 }}>
            <h3>Backup Status</h3>
            <div className="stats-grid">
              <div className="stat-card">
                <div className="stat-value" style={{ color: "var(--safe)" }}>{fullyBacked}</div>
                <div className="stat-label">2+ Backups</div>
              </div>
              <div className="stat-card">
                <div className="stat-value" style={{ color: "var(--warn)" }}>{singleCopy}</div>
                <div className="stat-label">1 Backup</div>
              </div>
              {noCopy > 0 && (
                <div className="stat-card">
                  <div className="stat-value" style={{ color: "var(--danger)" }}>{noCopy}</div>
                  <div className="stat-label">No Backup</div>
                </div>
              )}
            </div>
          </div>

          <div className="import-files-preview" style={{ marginTop: 16 }}>
            <h3>Files ({backupCounts.length})</h3>
            <div className="import-file-list">
              {backupCounts.slice(0, 100).map((f, i) => (
                <div key={i} className="import-file-row">
                  <span className="import-file-name">{f.fileName}</span>
                  <span className={`badge ${f.total >= 2 ? "badge-safe" : f.total === 1 ? "badge-warn" : "badge-danger"}`}>
                    {f.locations.join(", ")}
                  </span>
                </div>
              ))}
              {backupCounts.length > 100 && (
                <div className="import-file-row text-center text-muted-color">
                  ...and {backupCounts.length - 100} more files
                </div>
              )}
            </div>
          </div>

          <div className="import-actions mt-16">
            {sourceDevice?.isRemovable && sourceDevice.isConnected && (
              <button onClick={() => eject(sourceDevice.mountPoint)}>
                Eject SD Card
              </button>
            )}
            <button
              className="btn-primary"
              onClick={() => {
                setSelectedSource("");
                setSourcePath("");
                setSelectedTargets([]);
                reset();
              }}
            >
              Import Another
            </button>
          </div>
        </div>
        );
      })()}
    </div>
  );
}
