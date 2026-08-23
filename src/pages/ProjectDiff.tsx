import { useMemo, useState } from "react";
import {
  useProjectDiff,
  buildTree,
  collectKeys,
  folderState,
  entryKey,
  type TreeNode,
} from "../hooks/useProjectDiff";
import { LazyThumb } from "../components/LazyThumb";
import { formatBytes } from "../utils/format";
import type { DiffDeviceResult, DiffFileEntry, FileLocation } from "../types";
import "./ProjectDiff.css";

type Section = "delete" | "copy";

interface Props {
  projectId: number;
  projectTitle: string;
  sotDeviceId: string;
  onBack: () => void;
}

/** A stand-in FileLocation so <FilePreview> can resolve a path. */
export function previewLocation(deviceId: string, entry: DiffFileEntry): FileLocation {
  return {
    id: entry.locationId ?? 0,
    blake3Hash: entry.blake3Hash,
    deviceId,
    filePath: entry.relativePath,
    fileName: entry.fileName,
    fileSize: entry.fileSize,
    modifiedAt: null,
    lastVerified: "",
    scanMode: "diff",
  };
}

function FolderCheckbox({
  state,
  onChange,
}: {
  state: "all" | "none" | "some";
  onChange: (next: boolean) => void;
}) {
  return (
    <input
      type="checkbox"
      checked={state === "all"}
      ref={(el) => {
        if (el) el.indeterminate = state === "some";
      }}
      onChange={() => onChange(state !== "all")}
    />
  );
}

function Folder({
  deviceId,
  node,
  section,
  selected,
  toggleKeys,
  previewDeviceId,
  depth,
}: {
  deviceId: string;
  node: TreeNode;
  section: Section;
  selected: Set<string>;
  toggleKeys: (section: Section, keys: string[], next: boolean) => void;
  previewDeviceId: string;
  depth: number;
}) {
  // Deep folders and very large ones start closed: a day folder with
  // thousands of files should not render thousands of rows unasked.
  const [open, setOpen] = useState(depth < 2 && node.files.length <= 500);
  const keys = useMemo(() => collectKeys(deviceId, node), [deviceId, node]);
  const state = folderState(keys, selected);

  return (
    <div className="diff-folder" style={{ marginLeft: depth === 0 ? 0 : 16 }}>
      {node.path !== "" && (
        <div className="diff-folder-header">
          <FolderCheckbox state={state} onChange={(next) => toggleKeys(section, keys, next)} />
          <button className="diff-folder-name" onClick={() => setOpen((o) => !o)}>
            {open ? "-" : "+"} {node.name}
          </button>
          <span className="diff-folder-count">{keys.length}</span>
        </div>
      )}

      {open && (
        <>
          {node.children.map((child) => (
            <Folder
              key={child.path}
              deviceId={deviceId}
              node={child}
              section={section}
              selected={selected}
              toggleKeys={toggleKeys}
              previewDeviceId={previewDeviceId}
              depth={depth + 1}
            />
          ))}
          {node.files.map((file) => {
            const key = entryKey(deviceId, file.relativePath);
            return (
              <label key={key} className="diff-file-row">
                <input
                  type="checkbox"
                  checked={selected.has(key)}
                  onChange={(e) => toggleKeys(section, [key], e.target.checked)}
                />
                <LazyThumb
                  locations={[previewLocation(previewDeviceId, file)]}
                  fileName={file.fileName}
                  preferredDeviceId={previewDeviceId}
                />
                <span className="diff-file-name">{file.fileName}</span>
                <span className="diff-file-size">{formatBytes(file.fileSize)}</span>
              </label>
            );
          })}
        </>
      )}
    </div>
  );
}

function DeviceSection({
  device,
  section,
  selected,
  toggleKeys,
  sotDeviceId,
}: {
  device: DiffDeviceResult;
  section: Section;
  selected: Set<string>;
  toggleKeys: (section: Section, keys: string[], next: boolean) => void;
  sotDeviceId: string;
}) {
  const entries = section === "delete" ? device.toDelete : device.toCopy;
  const tree = useMemo(() => buildTree(entries), [entries]);

  if (device.skipReason) {
    return (
      <div className="diff-device diff-device-skipped">
        <strong>{device.deviceLabel}</strong>
        <span className="text-muted-color"> - {device.skipReason}</span>
      </div>
    );
  }
  if (entries.length === 0) return null;

  // Deletions preview from the backup that still holds the file; copies
  // preview from the source of truth, the only place they exist.
  const previewDeviceId = section === "delete" ? device.deviceId : sotDeviceId;
  const keys = collectKeys(device.deviceId, tree);
  const bytes = section === "delete" ? device.deleteBytes : device.copyBytes;

  return (
    <div className="diff-device">
      <div className="diff-device-header">
        <FolderCheckbox
          state={folderState(keys, selected)}
          onChange={(next) => toggleKeys(section, keys, next)}
        />
        <strong>{device.deviceLabel}</strong>
        <span className="text-muted-color">
          {entries.length} file{entries.length !== 1 ? "s" : ""}, {formatBytes(bytes)}
        </span>
      </div>
      <Folder
        deviceId={device.deviceId}
        node={tree}
        section={section}
        selected={selected}
        toggleKeys={toggleKeys}
        previewDeviceId={previewDeviceId}
        depth={0}
      />
    </div>
  );
}

export function ProjectDiff({ projectId, projectTitle, sotDeviceId, onBack }: Props) {
  const {
    phase,
    diff,
    progress,
    error,
    selectedDelete,
    selectedCopy,
    deleteLocationIds,
    copyItems,
    toggleKeys,
    setAll,
    cancel,
  } = useProjectDiff(projectId, sotDeviceId);

  const [section, setSection] = useState<Section>("delete");
  const selected = section === "delete" ? selectedDelete : selectedCopy;

  return (
    <div className="page">
      <div className="page-header">
        <div>
          <h1>Diff - {projectTitle}</h1>
          {diff && (
            <p className="text-muted-color text-xs mt-4">
              Source of truth: <strong>{diff.sotLabel}</strong>
            </p>
          )}
        </div>
        <div className="flex-row gap-8">
          <button onClick={onBack}>Back</button>
        </div>
      </div>

      {phase === "computing" && (
        <div className="diff-progress">
          <div className="progress-bar">
            <div
              className="progress-fill"
              style={{
                width: `${progress && progress.total ? (progress.checked / progress.total) * 100 : 0}%`,
              }}
            />
          </div>
          <div className="progress-stats">
            <span>Checking files on disk...</span>
            <span>
              {progress?.checked ?? 0} / {progress?.total ?? 0}
            </span>
          </div>
          <button className="btn-danger" onClick={cancel}>
            Cancel
          </button>
        </div>
      )}

      {error && <div className="error-msg">{error}</div>}

      {diff && (
        <>
          <div className="stats-grid mb-20">
            <div className="stat-card">
              <div className="stat-value">{diff.totalDeleteFiles}</div>
              <div className="stat-label">
                To delete, {formatBytes(diff.totalDeleteBytes)}
              </div>
            </div>
            <div className="stat-card">
              <div className="stat-value">{diff.totalCopyFiles}</div>
              <div className="stat-label">To copy, {formatBytes(diff.totalCopyBytes)}</div>
            </div>
            <div className="stat-card">
              <div className="stat-value">{diff.unreadable.length}</div>
              <div className="stat-label">Unreadable</div>
            </div>
          </div>

          {diff.unreadable.length > 0 && (
            <div className="diff-warning">
              {diff.unreadable.length} file{diff.unreadable.length !== 1 ? "s" : ""} could
              not be read and were left out of the diff entirely. Nothing will happen to
              them.
              <details>
                <summary>Show</summary>
                {diff.unreadable.slice(0, 100).map((u, i) => (
                  <div key={i} className="text-xs text-muted-color">
                    {u.deviceId}: {u.relativePath} - {u.error}
                  </div>
                ))}
              </details>
            </div>
          )}

          <div className="browser-controls">
            <div className="filter-toggle">
              <button
                className={section === "delete" ? "active" : ""}
                onClick={() => setSection("delete")}
              >
                Delete from backups ({deleteLocationIds.length})
              </button>
              <button
                className={section === "copy" ? "active" : ""}
                onClick={() => setSection("copy")}
              >
                Copy to backups ({copyItems.length})
              </button>
            </div>
            <div className="flex-row gap-8">
              <button onClick={() => setAll(section, true)}>Select all</button>
              <button onClick={() => setAll(section, false)}>Select none</button>
            </div>
          </div>

          <div className="diff-listing">
            {diff.devices.map((device) => (
              <DeviceSection
                key={`${section}-${device.deviceId}`}
                device={device}
                section={section}
                selected={selected}
                toggleKeys={toggleKeys}
                sotDeviceId={diff.sotDeviceId}
              />
            ))}
          </div>
        </>
      )}
    </div>
  );
}
