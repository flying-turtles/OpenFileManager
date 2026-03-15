import { useState, useEffect, useCallback } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { open } from "@tauri-apps/plugin-dialog";
import type { StorageDevice, PendingScan } from "../types";
import { useScanProgress } from "../hooks/useScanProgress";
import { ProgressBar } from "../components/ProgressBar";
import { getPendingScans, dismissPendingScan } from "../api/commands";
import "./Scanner.css";

interface Props {
  initialDevice?: StorageDevice;
}

export function Scanner({ initialDevice }: Props) {
  const [target, setTarget] = useState(initialDevice?.mountPoint || "");
  const [dragOver, setDragOver] = useState(false);
  const [pendingScans, setPendingScans] = useState<PendingScan[]>([]);
  const progress = useScanProgress();

  useEffect(() => {
    if (initialDevice?.mountPoint) {
      setTarget(initialDevice.mountPoint);
    }
  }, [initialDevice]);

  useEffect(() => {
    getPendingScans().then(setPendingScans);
  }, [progress.paused, progress.finished]);

  const handleStart = () => {
    if (!target) return;
    progress.scan(target);
  };

  const handleResume = (ps: PendingScan) => {
    setTarget(ps.target);
    progress.scan(ps.target);
  };

  const handleDismiss = async (ps: PendingScan) => {
    await dismissPendingScan(ps.id);
    setPendingScans((prev) => prev.filter((s) => s.id !== ps.id));
  };

  const handleBrowseFolder = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (selected) {
      setTarget(selected);
    }
  };

  const handleBrowseFiles = async () => {
    const selected = await open({ directory: false, multiple: true });
    if (selected) {
      // For multiple files, use the parent directory; for single file, use the file path
      const paths = Array.isArray(selected) ? selected : [selected];
      if (paths.length === 1) {
        setTarget(paths[0]);
      } else if (paths.length > 1) {
        // Use common parent directory
        const parts = paths[0].split("/");
        parts.pop();
        setTarget(parts.join("/"));
      }
    }
  };

  const handleDrop = useCallback(
    (paths: string[]) => {
      if (paths.length > 0 && !progress.scanning) {
        setTarget(paths[0]);
      }
    },
    [progress.scanning]
  );

  useEffect(() => {
    const webview = getCurrentWebviewWindow();
    const unlisten = webview.onDragDropEvent((event) => {
      if (event.payload.type === "over") {
        setDragOver(true);
      } else if (event.payload.type === "drop") {
        setDragOver(false);
        handleDrop(event.payload.paths);
      } else {
        setDragOver(false);
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [handleDrop]);

  const isBusy = progress.scanning;

  return (
    <div className="page">
      <h1>Scanner</h1>

      {pendingScans.length > 0 && !isBusy && !progress.finished && !progress.paused && (
        <div className="pending-scans">
          <h3>On Hold</h3>
          {pendingScans.map((ps) => (
            <div key={ps.id} className="pending-scan-row">
              <div className="pending-scan-info">
                <span className="pending-scan-target">{ps.target}</span>
                <span className="pending-scan-stats">
                  {ps.processed} / {ps.totalFiles} scanned · {ps.hashed} hashed
                </span>
                <span className="pending-scan-date">Paused {ps.pausedAt}</span>
              </div>
              <div className="pending-scan-actions">
                <button className="btn-primary" onClick={() => handleResume(ps)}>
                  Resume
                </button>
                <button onClick={() => handleDismiss(ps)}>Dismiss</button>
              </div>
            </div>
          ))}
        </div>
      )}

      <div className="scan-config">
        <div className="form-group">
          <label>Target Path</label>
          <div className={`path-input-row ${dragOver ? "drag-over" : ""}`}>
            <input
              type="text"
              value={target}
              onChange={(e) => setTarget(e.target.value)}
              placeholder="Drop a folder here, browse, or type a path..."
              disabled={isBusy}
            />
            <button onClick={handleBrowseFolder} disabled={isBusy}>
              Folder
            </button>
            <button onClick={handleBrowseFiles} disabled={isBusy}>
              File
            </button>
          </div>
          {dragOver && <div className="drop-hint">Drop to set path</div>}
        </div>
        <div className="scan-actions">
          {!isBusy && !progress.paused ? (
            <button className="btn-primary" onClick={handleStart} disabled={!target}>
              Start Scan
            </button>
          ) : isBusy ? (
            <>
              <button className="btn-warning" onClick={progress.pause}>
                Pause
              </button>
              <button className="btn-danger" onClick={progress.cancel}>
                Cancel
              </button>
            </>
          ) : null}
        </div>
      </div>

      {(isBusy || progress.finished || progress.paused) && (
        <ProgressBar
          scanned={progress.scanned}
          total={progress.total}
          toHash={progress.toHash}
          skipped={progress.skipped}
          hashed={progress.hashed}
          lastFile={progress.lastFile}
        />
      )}

      {progress.error && <div className="error-msg">{progress.error}</div>}

      {progress.paused && (
        <div className="scan-result scan-paused">
          <span>
            Scan paused: {progress.scanned} scanned, {progress.hashed} hashed, {progress.added} added
          </span>
          <div className="scan-paused-actions">
            <button className="btn-primary" onClick={() => progress.scan(target)}>
              Resume
            </button>
            <button onClick={progress.reset}>Dismiss</button>
          </div>
        </div>
      )}

      {progress.finished && (
        <div className="scan-result">
          <span>
            Scan complete: {progress.scanned} scanned, {progress.hashed} hashed,{" "}
            {progress.added} added, {progress.removed} removed
          </span>
          <button onClick={progress.reset}>New Scan</button>
        </div>
      )}
    </div>
  );
}
