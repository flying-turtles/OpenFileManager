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
import { DiffResolveModal, type DeletePreviewItem } from "../components/DiffResolveModal";
import { formatBytes } from "../utils/format";
import type { DiffDeviceResult, DiffFileEntry, FileLocation } from "../types";
import "./ProjectDiff.css";

type Section = "delete" | "copy";

// Above this many files in a device+section, folders start collapsed on
// load. Below it, everything expands — the common case is a handful of
// Lightroom rejects and should stay fully visible. This must be checked
// once per device section, not per folder: `buildTree` only attaches files
// to their leaf directory node, so a folder of subfolders always reports
// zero direct files and a per-folder check never bounds anything. Import
// lays files out as `<mount>/YYYY-MM-DD/<filename>`, so a multi-day shoot
// is exactly this shape - many depth-1 folders, each under any reasonable
// per-folder cap, all auto-opening at once.
const AUTO_EXPAND_FILE_THRESHOLD = 500;

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
  sectionExpanded,
}: {
  deviceId: string;
  node: TreeNode;
  section: Section;
  selected: Set<string>;
  toggleKeys: (section: Section, keys: string[], next: boolean) => void;
  previewDeviceId: string;
  depth: number;
  sectionExpanded: boolean;
}) {
  // depth 0 is the tree root: it has no header (node.path === "" below
  // skips it) and therefore no toggle, so it must always let its children
  // through or nothing - not even the day-folder headers - could ever be
  // reached. Only depth 1 (the day folders, which do have a header and
  // toggle) is actually gated by the section-level size decision; depth 2+
  // always starts closed.
  const [open, setOpen] = useState(depth === 0 || (sectionExpanded && depth < 2));
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
              sectionExpanded={sectionExpanded}
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
  // Hoisted above the early returns below: hooks can't be called
  // conditionally, and this component returns early for skipped or
  // empty devices.
  const keys = useMemo(() => collectKeys(device.deviceId, tree), [device.deviceId, tree]);

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
  const bytes = section === "delete" ? device.deleteBytes : device.copyBytes;
  const sectionExpanded = entries.length <= AUTO_EXPAND_FILE_THRESHOLD;

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
        sectionExpanded={sectionExpanded}
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
    resolve,
    resolveProgress,
    resolveResult,
    reload,
  } = useProjectDiff(projectId, sotDeviceId);

  const [section, setSection] = useState<Section>("delete");
  const [resolving, setResolving] = useState(false);
  const selected = section === "delete" ? selectedDelete : selectedCopy;

  const deletePreviewItems = useMemo<DeletePreviewItem[]>(() => {
    if (!diff) return [];
    const items: DeletePreviewItem[] = [];
    for (const device of diff.devices) {
      for (const e of device.toDelete) {
        const key = entryKey(device.deviceId, e.relativePath);
        if (!selectedDelete.has(key)) continue;
        items.push({
          key,
          fileName: e.fileName,
          fileSize: e.fileSize,
          deviceLabel: device.deviceLabel,
          location: previewLocation(device.deviceId, e),
        });
      }
    }
    return items;
  }, [diff, selectedDelete]);

  const selectedCopyBytes = useMemo(
    () => copyItems.reduce((sum, i) => sum + i.fileSize, 0),
    [copyItems]
  );

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
          <button
            className="btn-danger"
            disabled={
              phase !== "ready" ||
              (deleteLocationIds.length === 0 && copyItems.length === 0)
            }
            onClick={() => setResolving(true)}
          >
            Resolve diff
          </button>
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

      {resolving && diff && (
        <DiffResolveModal
          sotLabel={diff.sotLabel}
          deleteItems={deletePreviewItems}
          copyCount={copyItems.length}
          copyBytes={selectedCopyBytes}
          purgeCount={diff.purgeLocationIds.length}
          busy={phase === "resolving"}
          progressText={resolveProgress}
          result={resolveResult}
          onConfirm={resolve}
          onClose={() => {
            const didResolve = resolveResult !== null;
            setResolving(false);
            // The index and the disks both moved; recompute rather than show
            // a diff that no longer describes reality.
            if (didResolve) reload();
          }}
        />
      )}
    </div>
  );
}
