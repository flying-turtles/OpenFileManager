import { useCallback, useEffect, useState } from "react";
import { SourceCleanupModal } from "./SourceCleanupModal";
import {
  getScanSummary,
  getScanCleanupPreview,
  deleteRedundantScannedFiles,
} from "../api/commands";
import { formatBytes } from "../utils/format";
import type { ScanSummary, SourceCleanupEvent } from "../types";

interface Props {
  /** The path that was scanned. */
  target: string;
  deviceNames: Record<string, string>;
  onOpenProject: (id: number) => void;
}

/** `2024-03-07 14:22:01` -> `7 Mar 2024`. Falls back to the raw string. */
function formatDay(value: string | null): string {
  if (!value) return "—";
  const date = new Date(value.replace(" ", "T"));
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleDateString(undefined, {
    day: "numeric",
    month: "short",
    year: "numeric",
  });
}

function dateRange(summary: ScanSummary): string {
  const { oldestModified: from, newestModified: to } = summary;
  if (!from && !to) return "Unknown";
  const start = formatDay(from);
  const end = formatDay(to);
  return start === end ? start : `${start} – ${end}`;
}

/**
 * What the index knows about the scanned location after a scan finishes.
 * Deliberately not built from the scan's own counters: a scan skips
 * unchanged directories, so on a re-scan those counters describe almost
 * nothing while these figures describe the whole location.
 */
export function ScanSummaryPanel({ target, deviceNames, onOpenProject }: Props) {
  const [summary, setSummary] = useState<ScanSummary | null>(null);
  const [error, setError] = useState("");
  const [showCleanup, setShowCleanup] = useState(false);

  const load = useCallback(() => {
    setError("");
    getScanSummary(target)
      .then(setSummary)
      .catch((e) => setError(String(e)));
  }, [target]);

  useEffect(load, [load]);

  const loadPreview = useCallback(() => getScanCleanupPreview(target), [target]);

  const runDelete = useCallback(
    (permanent: boolean, onEvent: (event: SourceCleanupEvent) => void) =>
      deleteRedundantScannedFiles(target, permanent, onEvent),
    [target]
  );

  if (error) return <div className="error-msg">{error}</div>;
  if (!summary) return <div className="scan-summary-loading">Summarising…</div>;

  const location = summary.scanPrefix
    ? `${summary.deviceLabel} / ${summary.scanPrefix}`
    : summary.deviceLabel;

  return (
    <div className="scan-summary">
      <div className="scan-summary-header">
        <h3>{location}</h3>
        <span className="text-muted-color text-xs">{summary.totalFiles.toLocaleString()} files indexed</span>
      </div>

      <div className="scan-summary-stats">
        <div className="scan-summary-stat">
          <span className="scan-summary-label">Total size</span>
          <span className="scan-summary-value">{formatBytes(summary.totalBytes)}</span>
        </div>
        <div className="scan-summary-stat">
          <span className="scan-summary-label">Files</span>
          <span className="scan-summary-value">{summary.totalFiles.toLocaleString()}</span>
        </div>
        <div className="scan-summary-stat">
          <span className="scan-summary-label">Date range</span>
          <span className="scan-summary-value">{dateRange(summary)}</span>
        </div>
        <div className="scan-summary-stat">
          <span className="scan-summary-label">Backed up elsewhere</span>
          <span className="scan-summary-value">
            {summary.redundantFiles.toLocaleString()}
            {summary.redundantFiles > 0 && (
              <span className="text-muted-color text-xs"> · {formatBytes(summary.redundantBytes)}</span>
            )}
          </span>
        </div>
      </div>

      <div className="scan-summary-section">
        <span className="scan-summary-label">Projects</span>
        {summary.projects.length === 0 ? (
          <p className="text-muted-color text-xs">No files here fall inside a project's date range.</p>
        ) : (
          <div className="scan-summary-projects">
            {summary.projects.map((p) => (
              <button
                key={p.id}
                className="scan-summary-project"
                onClick={() => onOpenProject(p.id)}
                title={`Open ${p.title}`}
              >
                <span className="scan-summary-project-title">{p.title}</span>
                <span className="text-muted-color text-xs">
                  {p.fileCount.toLocaleString()} · {formatBytes(p.totalBytes)}
                </span>
              </button>
            ))}
            {summary.unassignedFiles > 0 && (
              <span className="scan-summary-project scan-summary-project-none">
                <span className="scan-summary-project-title">No project</span>
                <span className="text-muted-color text-xs">
                  {summary.unassignedFiles.toLocaleString()}
                </span>
              </span>
            )}
          </div>
        )}
      </div>

      {summary.redundantFiles > 0 && (
        <div className="scan-summary-cleanup">
          <p className="text-xs">
            {summary.redundantFiles.toLocaleString()} file
            {summary.redundantFiles !== 1 ? "s" : ""} ({formatBytes(summary.redundantBytes)}) here
            also exist on at least 2 other devices.
          </p>
          <button className="btn-danger" onClick={() => setShowCleanup(true)}>
            Delete backed-up files
          </button>
        </div>
      )}

      {showCleanup && (
        <SourceCleanupModal
          deviceNames={deviceNames}
          loadPreview={loadPreview}
          runDelete={runDelete}
          onDeleted={load}
          onClose={() => setShowCleanup(false)}
        />
      )}
    </div>
  );
}
