import { useEffect, useState, useMemo } from "react";
import { useDevices } from "../hooks/useDevices";
import { useFiles } from "../hooks/useFiles";
import { FileTable } from "../components/FileTable";
import { BulkDeleteModal } from "../components/BulkDeleteModal";
import type { FileLocation } from "../types";

type SortKey = "name" | "size" | "modified";
type SortDir = "asc" | "desc";
type FilterMode = "all" | "unsafe" | "safe" | "duplicates";

function getExtension(f: FileLocation): string {
  const dot = f.fileName.lastIndexOf(".");
  return dot > 0 ? f.fileName.slice(dot + 1).toLowerCase() : "";
}

function sortFiles(files: FileLocation[], key: SortKey, dir: SortDir): FileLocation[] {
  const sorted = [...files].sort((a, b) => {
    if (key === "size") return a.fileSize - b.fileSize;
    if (key === "modified") return (a.modifiedAt ?? "").localeCompare(b.modifiedAt ?? "");
    return a.fileName.localeCompare(b.fileName);
  });
  return dir === "desc" ? sorted.reverse() : sorted;
}

export function FileBrowser() {
  const { devices } = useDevices();
  const {
    files, unsafeFiles, safeFiles, duplicateFiles, loading, totalCount,
    loadDeviceFiles, loadUnsafeFiles, loadSafeFiles, loadDuplicateFiles,
    deleteFileCopy, bulkDeleteCopies, loadFileSafety,
  } = useFiles();
  const [selectedDevice, setSelectedDevice] = useState("");
  const [filter, setFilter] = useState<FilterMode>("all");
  const [extFilter, setExtFilter] = useState("");
  const [sortKey, setSortKey] = useState<SortKey>("name");
  const [sortDir, setSortDir] = useState<SortDir>("asc");
  const [dupDeviceFilter, setDupDeviceFilter] = useState("");
  const [sameDriveOnly, setSameDriveOnly] = useState(false);
  const [showBulkDelete, setShowBulkDelete] = useState(false);

  const deviceNames = useMemo(() => {
    const map: Record<string, string> = {};
    for (const d of devices) map[d.id] = d.label;
    return map;
  }, [devices]);

  const connectedDeviceIds = useMemo(
    () => new Set(devices.filter((d) => d.isConnected).map((d) => d.id)),
    [devices],
  );

  const bulkDeleteTargets: FileLocation[] = useMemo(() => {
    if (filter !== "duplicates" || !dupDeviceFilter) return [];
    if (sameDriveOnly) {
      return duplicateFiles.flatMap((sf) => {
        const onDevice = sf.locations.filter((l) => {
          if (l.deviceId !== dupDeviceFilter) return false;
          if (extFilter) {
            const dot = l.fileName.lastIndexOf(".");
            const ext = dot > 0 ? l.fileName.slice(dot + 1).toLowerCase() : "";
            if (ext !== extFilter) return false;
          }
          return true;
        });
        if (onDevice.length <= 1) return [];
        const sorted = [...onDevice].sort((a, b) => a.filePath.length - b.filePath.length);
        return sorted.slice(1);
      });
    }
    return duplicateFiles.flatMap((sf) =>
      sf.locations.filter((l) => {
        if (l.deviceId !== dupDeviceFilter) return false;
        if (extFilter) {
          const dot = l.fileName.lastIndexOf(".");
          const ext = dot > 0 ? l.fileName.slice(dot + 1).toLowerCase() : "";
          if (ext !== extFilter) return false;
        }
        return true;
      })
    );
  }, [duplicateFiles, dupDeviceFilter, extFilter, filter, sameDriveOnly]);

  const bulkDeleteDeviceName = useMemo(() => {
    const d = devices.find((d) => d.id === dupDeviceFilter);
    return d?.label ?? "";
  }, [devices, dupDeviceFilter]);

  const bulkDeleteDeviceConnected = useMemo(
    () => connectedDeviceIds.has(dupDeviceFilter),
    [connectedDeviceIds, dupDeviceFilter],
  );

  useEffect(() => {
    if (filter === "unsafe") {
      loadUnsafeFiles();
    } else if (filter === "safe") {
      loadSafeFiles();
    } else if (filter === "duplicates") {
      loadDuplicateFiles(dupDeviceFilter || undefined, sameDriveOnly || undefined);
    } else if (selectedDevice) {
      loadDeviceFiles(selectedDevice);
    }
  }, [selectedDevice, filter, dupDeviceFilter, sameDriveOnly, loadDeviceFiles, loadUnsafeFiles, loadSafeFiles, loadDuplicateFiles]);

  useEffect(() => {
    setExtFilter("");
    setDupDeviceFilter("");
    setSameDriveOnly(false);
  }, [filter, selectedDevice]);

  const rawFiles: FileLocation[] =
    filter === "all"
      ? files
      : (filter === "unsafe" ? unsafeFiles : filter === "safe" ? safeFiles : duplicateFiles)
          .filter((sf) => sf.locations.length > 0)
          .map((sf) => ({ ...sf.locations[0], _safety: sf }));

  const extensions = useMemo(() => {
    const counts = new Map<string, number>();
    for (const f of rawFiles) {
      const ext = getExtension(f);
      if (ext) counts.set(ext, (counts.get(ext) ?? 0) + 1);
    }
    return [...counts.entries()].sort((a, b) => b[1] - a[1]);
  }, [rawFiles]);

  const displayFiles = useMemo(() => {
    let filtered = rawFiles;
    if (extFilter) {
      filtered = filtered.filter((f) => getExtension(f) === extFilter);
    }
    return sortFiles(filtered, sortKey, sortDir);
  }, [rawFiles, extFilter, sortKey, sortDir]);

  const toggleSort = (key: SortKey) => {
    if (sortKey === key) {
      setSortDir((d) => (d === "asc" ? "desc" : "asc"));
    } else {
      setSortKey(key);
      setSortDir(key === "name" ? "asc" : "desc");
    }
  };

  const emptyMsg =
    filter === "unsafe"
      ? "No unsafe files found"
      : filter === "safe"
        ? "No safe files found"
        : filter === "duplicates"
          ? "No files with more than 2 copies"
          : "Select a device to view files";

  return (
    <div className="page">
      <h1>Files</h1>
      <div className="browser-controls">
        <div className="filter-toggle">
          <button
            className={filter === "all" ? "active" : ""}
            onClick={() => setFilter("all")}
          >
            By Device
          </button>
          <button
            className={filter === "safe" ? "active" : ""}
            onClick={() => setFilter("safe")}
          >
            Safe Only
          </button>
          <button
            className={filter === "unsafe" ? "active" : ""}
            onClick={() => setFilter("unsafe")}
          >
            Unsafe Only
          </button>
          <button
            className={filter === "duplicates" ? "active" : ""}
            onClick={() => setFilter("duplicates")}
          >
            Duplicates
          </button>
        </div>
        {filter === "all" && (
          <select
            value={selectedDevice}
            onChange={(e) => setSelectedDevice(e.target.value)}
          >
            <option value="">Select device...</option>
            {devices.map((d) => (
              <option key={d.id} value={d.id}>
                {d.label} ({d.mountPoint})
              </option>
            ))}
          </select>
        )}
        {filter === "duplicates" && (
          <>
            <select
              value={dupDeviceFilter}
              onChange={(e) => setDupDeviceFilter(e.target.value)}
            >
              <option value="">All devices</option>
              {devices.filter((d) => d.isConnected).map((d) => (
                <option key={d.id} value={d.id}>
                  {d.label} ({d.mountPoint})
                </option>
              ))}
            </select>
            <label style={{ display: "flex", alignItems: "center", gap: 6 }}>
              <input
                type="checkbox"
                checked={sameDriveOnly}
                onChange={(e) => setSameDriveOnly(e.target.checked)}
                disabled={!dupDeviceFilter}
              />
              Same-drive only
            </label>
            {bulkDeleteTargets.length > 0 && bulkDeleteDeviceConnected && (
              <button className="btn-danger" onClick={() => setShowBulkDelete(true)}>
                Bulk Delete ({bulkDeleteTargets.length})
              </button>
            )}
          </>
        )}
      </div>

      {rawFiles.length > 0 && (
        <div className="browser-controls mt-0">
          <select
            value={extFilter}
            onChange={(e) => setExtFilter(e.target.value)}
            style={{ minWidth: 160 }}
          >
            <option value="">All types</option>
            {extensions.map(([ext, count]) => (
              <option key={ext} value={ext}>
                .{ext} ({count.toLocaleString()})
              </option>
            ))}
          </select>
          <div className="filter-toggle">
            <button
              className={sortKey === "name" ? "active" : ""}
              onClick={() => toggleSort("name")}
            >
              Name {sortKey === "name" ? (sortDir === "asc" ? "\u2191" : "\u2193") : ""}
            </button>
            <button
              className={sortKey === "size" ? "active" : ""}
              onClick={() => toggleSort("size")}
            >
              Size {sortKey === "size" ? (sortDir === "asc" ? "\u2191" : "\u2193") : ""}
            </button>
            <button
              className={sortKey === "modified" ? "active" : ""}
              onClick={() => toggleSort("modified")}
            >
              Date {sortKey === "modified" ? (sortDir === "asc" ? "\u2191" : "\u2193") : ""}
            </button>
          </div>
          {extFilter && (
            <span className="file-table-status">
              {displayFiles.length.toLocaleString()} .{extFilter} files
            </span>
          )}
        </div>
      )}

      {loading ? (
        <div>
          {[...Array(8)].map((_, i) => <div key={i} className="skeleton skeleton-row" />)}
        </div>
      ) : (
        <FileTable
          files={displayFiles}
          totalCount={extFilter ? undefined : totalCount}
          deviceNames={deviceNames}
          connectedDeviceIds={connectedDeviceIds}
          selectedDeviceId={filter === "all" ? selectedDevice : undefined}
          onGetSafety={loadFileSafety}
          onDeleteLocation={filter === "duplicates" ? deleteFileCopy : undefined}
        />
      )}

      {!loading && displayFiles.length === 0 && (
        <p className="empty">{emptyMsg}</p>
      )}

      {showBulkDelete && (
        <BulkDeleteModal
          deviceName={bulkDeleteDeviceName}
          files={bulkDeleteTargets}
          onConfirm={bulkDeleteCopies}
          onClose={() => setShowBulkDelete(false)}
        />
      )}
    </div>
  );
}
