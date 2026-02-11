import { useState } from "react";
import type { StorageDevice } from "../types";
import { useScanProgress } from "../hooks/useScanProgress";
import { ProgressBar } from "../components/ProgressBar";

interface Props {
  initialDevice?: StorageDevice;
}

export function Scanner({ initialDevice }: Props) {
  const [target, setTarget] = useState(initialDevice?.mount_point || "");
  const [mode, setMode] = useState<"quick" | "full">("quick");
  const progress = useScanProgress();

  const handleStart = () => {
    if (!target) return;
    progress.scan(target, mode);
  };

  return (
    <div className="page">
      <h1>Scanner</h1>
      <div className="scan-config">
        <div className="form-group">
          <label>Target Path</label>
          <input
            type="text"
            value={target}
            onChange={(e) => setTarget(e.target.value)}
            placeholder="/Volumes/MyDrive or /Users/..."
            disabled={progress.scanning}
          />
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
