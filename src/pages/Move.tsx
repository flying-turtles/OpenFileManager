import { useState, useEffect, useCallback } from "react";
import { useMove } from "../hooks/useMove";
import { useDevices } from "../hooks/useDevices";
import { browseDirectory } from "../api/commands";
import { formatBytes } from "../utils/format";
import type { DirEntry } from "../types";
import "./Move.css";

const joinPath = (dir: string, name: string) =>
  dir.endsWith("/") ? `${dir}${name}` : `${dir}/${name}`;

const parentPath = (path: string) => {
  const trimmed = path.replace(/\/+$/, "");
  const idx = trimmed.lastIndexOf("/");
  return idx > 0 ? trimmed.slice(0, idx) : "/";
};

interface PaneProps {
  title: string;
  path: string;
  onPathChange: (path: string) => void;
  selected?: Set<string>;
  onToggle?: (fullPath: string) => void;
  refreshKey: number;
}

function Pane({ title, path, onPathChange, selected, onToggle, refreshKey }: PaneProps) {
  const { devices } = useDevices();
  const [entries, setEntries] = useState<DirEntry[]>([]);
  const [error, setError] = useState("");
  const connected = devices.filter((d) => d.isConnected);

  useEffect(() => {
    if (!path) {
      setEntries([]);
      return;
    }
    let cancelled = false;
    browseDirectory(path)
      .then((e) => {
        if (!cancelled) {
          setEntries(e);
          setError("");
        }
      })
      .catch((e) => {
        if (!cancelled) {
          setEntries([]);
          setError(String(e));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [path, refreshKey]);

  return (
    <div className="move-pane">
      <h3>{title}</h3>
      <select
        value={connected.find((d) => path.startsWith(d.mountPoint))?.mountPoint ?? ""}
        onChange={(e) => onPathChange(e.target.value)}
      >
        <option value="">Select device...</option>
        {connected.map((d) => (
          <option key={d.id} value={d.mountPoint}>
            {d.label}
          </option>
        ))}
      </select>

      {path && (
        <div className="move-crumb">
          <button onClick={() => onPathChange(parentPath(path))} disabled={path === "/"}>
            Up
          </button>
          <span className="move-crumb-path" title={path}>
            {path}
          </span>
        </div>
      )}

      {error && <div className="error-msg">{error}</div>}

      <div className="move-listing">
        {entries.map((entry) => {
          const full = joinPath(path, entry.name);
          return (
            <div key={full} className="move-row">
              {onToggle && (
                <input
                  type="checkbox"
                  checked={selected?.has(full) ?? false}
                  onChange={() => onToggle(full)}
                />
              )}
              <button
                className="move-row-name"
                disabled={!entry.isDir}
                onClick={() => entry.isDir && onPathChange(full)}
              >
                {entry.isDir ? "📁" : "📄"} {entry.name}
              </button>
              {!entry.isDir && (
                <span className="move-row-size">{formatBytes(entry.size)}</span>
              )}
            </div>
          );
        })}
        {path && entries.length === 0 && !error && (
          <div className="move-empty">Empty</div>
        )}
      </div>
    </div>
  );
}

export function Move() {
  const { phase, plan, progress, result, errors, createPlan, start, cancel, reset } =
    useMove();
  const [sourcePath, setSourcePath] = useState("");
  const [destPath, setDestPath] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [permanent, setPermanent] = useState(false);
  const [refreshKey, setRefreshKey] = useState(0);

  const toggle = useCallback((full: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(full)) next.delete(full);
      else next.add(full);
      return next;
    });
  }, []);

  // Changing the source folder invalidates selections made in the old one.
  const changeSourcePath = useCallback((p: string) => {
    setSourcePath(p);
    setSelected(new Set());
  }, []);

  const finish = useCallback(() => {
    setSelected(new Set());
    setRefreshKey((k) => k + 1);
    reset();
  }, [reset]);

  const busy = phase === "planning" || phase === "moving";

  return (
    <div className="page">
      <h1>Move</h1>

      {errors.length > 0 && (
        <div className="error-msg">{errors[errors.length - 1]}</div>
      )}

      <div className="move-layout">
        <Pane
          title="Source"
          path={sourcePath}
          onPathChange={changeSourcePath}
          selected={selected}
          onToggle={toggle}
          refreshKey={refreshKey}
        />

        <div className="move-controls">
          <div className="move-selected-count">
            {selected.size} selected
          </div>
          <label className="move-permanent">
            <input
              type="checkbox"
              checked={permanent}
              onChange={(e) => setPermanent(e.target.checked)}
            />
            Delete source permanently (skip Trash)
          </label>
          <button
            className="btn-primary"
            disabled={busy || selected.size === 0 || !destPath}
            onClick={() => createPlan([...selected], destPath)}
          >
            Move →
          </button>

          {phase === "planned" && plan && (
            <div className="move-plan">
              <div>
                {plan.totalFiles} file(s), {formatBytes(plan.totalBytes)}
              </div>
              <div className="text-muted-color">to {plan.destLabel}</div>
              {plan.sameVolumeCount === plan.totalFiles && plan.totalFiles > 0 && (
                <div className="text-muted-color">Same volume — instant move</div>
              )}
              <div className="move-plan-actions">
                <button className="btn-primary" onClick={() => start(permanent)}>
                  Confirm
                </button>
                <button onClick={reset}>Cancel</button>
              </div>
            </div>
          )}

          {phase === "moving" && progress && (
            <div className="move-progress">
              <div className="progress-bar">
                <div
                  className="progress-fill"
                  style={{
                    width: `${progress.totalBytes ? (progress.bytesMoved / progress.totalBytes) * 100 : 0}%`,
                  }}
                />
              </div>
              <div className="progress-stats">
                <span>
                  {progress.phase} {progress.currentFile}
                </span>
                <span>
                  {progress.processed} / {progress.total}
                </span>
              </div>
              <button className="btn-danger" onClick={cancel}>
                Cancel
              </button>
            </div>
          )}

          {phase === "cancelled" && (
            <div className="move-result">
              <div>Move cancelled.</div>
              <button className="btn-primary" onClick={finish}>
                Done
              </button>
            </div>
          )}

          {phase === "complete" && result && (
            <div className="move-result">
              <div>
                {result.moved} moved ({formatBytes(result.bytesMoved)})
              </div>
              {result.failed.length > 0 && (
                <div className="move-failures">
                  <strong>{result.failed.length} failed</strong>
                  {result.failed.map((f, i) => (
                    <div key={i} className="move-failure">
                      {f.fileName}: {f.error}
                    </div>
                  ))}
                </div>
              )}
              <button className="btn-primary" onClick={finish}>
                Done
              </button>
            </div>
          )}
        </div>

        <Pane
          title="Destination"
          path={destPath}
          onPathChange={setDestPath}
          refreshKey={refreshKey}
        />
      </div>
    </div>
  );
}
