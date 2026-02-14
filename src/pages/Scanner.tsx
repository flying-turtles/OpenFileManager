import { useState, useEffect, useCallback } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { open } from "@tauri-apps/plugin-dialog";
import type { StorageDevice } from "../types";
import { useScanProgress } from "../hooks/useScanProgress";
import { ProgressBar } from "../components/ProgressBar";

interface Props {
  initialDevice?: StorageDevice;
}

export function Scanner({ initialDevice }: Props) {
  const [target, setTarget] = useState(initialDevice?.mount_point || "");
  const [mode, setMode] = useState<"quick" | "full">("quick");
  const [dragOver, setDragOver] = useState(false);
  const progress = useScanProgress();

  const handleStart = () => {
    if (!target) return;
    progress.scan(target, mode);
  };

  const handleBrowse = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (selected) {
      setTarget(selected);
    }
  };

  const handleDrop = useCallback((paths: string[]) => {
    if (paths.length > 0 && !progress.scanning) {
      setTarget(paths[0]);
    }
  }, [progress.scanning]);

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

  return (
    <div className="page">
      <h1>Scanner</h1>
      <div className="scan-config">
        <div className="form-group">
          <label>Target Path</label>
          <div className={`path-input-row ${dragOver ? "drag-over" : ""}`}>
            <input
              type="text"
              value={target}
              onChange={(e) => setTarget(e.target.value)}
              placeholder="Drop a folder here, browse, or type a path..."
              disabled={progress.scanning}
            />
            <button onClick={handleBrowse} disabled={progress.scanning}>
              Browse
            </button>
          </div>
          {dragOver && <div className="drop-hint">Drop to set path</div>}
        </div>
        <div className="form-group">
          <label>Mode</label>
          <div className="mode-toggle">
            <button
              className={mode === "quick" ? "active" : ""}
              onClick={() => setMode("quick")}
              disabled={progress.scanning}
            >
              Quick
            </button>
            <button
              className={mode === "full" ? "active" : ""}
              onClick={() => setMode("full")}
              disabled={progress.scanning}
            >
              Full
            </button>
          </div>
        </div>
        <div className="scan-actions">
          {!progress.scanning ? (
            <button className="btn-primary" onClick={handleStart} disabled={!target}>
              Start Scan
            </button>
          ) : (
            <button className="btn-danger" onClick={progress.cancel}>
              Cancel
            </button>
          )}
        </div>
      </div>

      {(progress.scanning || progress.finished) && (
        <ProgressBar
          scanned={progress.scanned}
          total={progress.total}
          hashed={progress.hashed}
          lastFile={progress.lastFile}
        />
      )}

      {progress.error && <div className="error-msg">{progress.error}</div>}

      {progress.finished && (
        <div className="scan-result">
          <span>
            Scan complete: {progress.scanned} scanned, {progress.hashed} hashed, {progress.added} added, {progress.removed} removed
          </span>
          <button onClick={progress.reset}>New Scan</button>
        </div>
      )}
    </div>
  );
}
