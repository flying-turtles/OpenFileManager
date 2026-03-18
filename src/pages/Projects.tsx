import { useEffect, useState, useMemo } from "react";
import { useProjects } from "../hooks/useProjects";
import { useDevices } from "../hooks/useDevices";
import { FileTable } from "../components/FileTable";
import { BulkDeleteModal } from "../components/BulkDeleteModal";
import { TransferModal } from "../components/TransferModal";
import { getFileSafety, deleteFileCopy, bulkDeleteFileCopies } from "../api/commands";
import type { Project, FileLocation, FileSafety, BulkDeleteResult, BulkDeleteEvent } from "../types";
import "./Projects.css";

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
}

type View = "list" | "detail" | "create" | "edit";
type SafetyFilter = "all" | "safe" | "unsafe" | "duplicates";
type SortKey = "name" | "size" | "modified";
type SortDir = "asc" | "desc";

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

export function Projects() {
  const { projects, selected, loading, refresh, select, create, update, remove, setSelected } =
    useProjects();
  const { devices, refresh: refreshDevices } = useDevices();
  const [view, setView] = useState<View>("list");

  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [startDate, setStartDate] = useState("");
  const [endDate, setEndDate] = useState("");

  // Detail view filters
  const [safetyFilter, setSafetyFilter] = useState<SafetyFilter>("all");
  const [extFilter, setExtFilter] = useState("");
  const [sortKey, setSortKey] = useState<SortKey>("name");
  const [sortDir, setSortDir] = useState<SortDir>("asc");
  const [projectFiles, setProjectFiles] = useState<FileSafety[]>([]);
  const [dupDeviceFilter, setDupDeviceFilter] = useState("");
  const [showBulkDelete, setShowBulkDelete] = useState(false);
  const [showTransfer, setShowTransfer] = useState(false);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const deviceNames = useMemo(() => {
    const map: Record<string, string> = {};
    for (const d of devices) map[d.id] = d.label;
    return map;
  }, [devices]);

  const connectedDeviceIds = useMemo(
    () => new Set(devices.filter((d) => d.isConnected).map((d) => d.id)),
    [devices],
  );

  // Sync projectFiles with selected project data
  useEffect(() => {
    if (selected) {
      setProjectFiles(selected.files);
    }
  }, [selected]);

  // Reset filters when opening a new project
  const openDetail = async (id: number) => {
    setSafetyFilter("all");
    setExtFilter("");
    setDupDeviceFilter("");
    setSortKey("name");
    setSortDir("asc");
    await select(id);
    setView("detail");
  };

  const openCreate = () => {
    setTitle("");
    setDescription("");
    setStartDate("");
    setEndDate("");
    setView("create");
  };

  const openEdit = (p: Project) => {
    setTitle(p.title);
    setDescription(p.description);
    setStartDate(p.startDate);
    setEndDate(p.endDate);
    setView("edit");
  };

  const handleSubmit = async () => {
    if (!title || !startDate || !endDate) return;
    if (view === "create") {
      await create(title, description, startDate, endDate);
    } else if (view === "edit" && selected) {
      await update(selected.project.id, title, description, startDate, endDate);
      await select(selected.project.id);
    }
    setView("list");
  };

  const handleDeleteProject = async (id: number) => {
    if (!confirm("Delete this project?")) return;
    await remove(id);
    setView("list");
  };

  const handleDeleteFileCopy = async (locationId: number) => {
    await deleteFileCopy(locationId);
    setProjectFiles((prev) =>
      prev
        .map((sf) => ({
          ...sf,
          totalCopies: sf.totalCopies - (sf.locations.some((l) => l.id === locationId) ? 1 : 0),
          locations: sf.locations.filter((l) => l.id !== locationId),
        }))
        .filter((sf) => sf.locations.length > 0)
    );
  };

  const handleBulkDelete = async (locationIds: number[], onEvent: (event: BulkDeleteEvent) => void): Promise<BulkDeleteResult> => {
    const result = await bulkDeleteFileCopies(locationIds, onEvent);
    const succeededSet = new Set(result.succeeded);
    setProjectFiles((prev) =>
      prev
        .map((sf) => ({
          ...sf,
          totalCopies: sf.totalCopies - sf.locations.filter((l) => succeededSet.has(l.id)).length,
          locations: sf.locations.filter((l) => !succeededSet.has(l.id)),
        }))
        .filter((sf) => sf.locations.length > 0)
    );
    return result;
  };

  const bulkDeleteTargets: FileLocation[] = useMemo(() => {
    if (safetyFilter !== "duplicates" || !dupDeviceFilter) return [];
    return projectFiles
      .filter((sf) => sf.totalCopies > 2)
      .flatMap((sf) =>
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
  }, [projectFiles, dupDeviceFilter, extFilter, safetyFilter]);

  const bulkDeleteDeviceName = useMemo(() => {
    const d = devices.find((d) => d.id === dupDeviceFilter);
    return d?.label ?? "";
  }, [devices, dupDeviceFilter]);

  const bulkDeleteDeviceConnected = useMemo(
    () => connectedDeviceIds.has(dupDeviceFilter),
    [connectedDeviceIds, dupDeviceFilter],
  );

  // Filter project files by safety status
  const filteredSafetyFiles: FileSafety[] = useMemo(() => {
    if (safetyFilter === "safe") return projectFiles.filter((sf) => sf.isSafe);
    if (safetyFilter === "unsafe") return projectFiles.filter((sf) => !sf.isSafe);
    if (safetyFilter === "duplicates") {
      let dups = projectFiles.filter((sf) => sf.totalCopies > 2);
      if (dupDeviceFilter) {
        dups = dups.filter((sf) => sf.locations.some((l) => l.deviceId === dupDeviceFilter));
      }
      return dups;
    }
    return projectFiles;
  }, [projectFiles, safetyFilter, dupDeviceFilter]);

  // One row per unique file (first location), not one per location
  const rawFiles: FileLocation[] = useMemo(
    () =>
      filteredSafetyFiles
        .filter((sf) => sf.locations.length > 0)
        .map((sf) => ({ ...sf.locations[0], _safety: sf })),
    [filteredSafetyFiles]
  );

  // Extension counts
  const extensions = useMemo(() => {
    const counts = new Map<string, number>();
    for (const f of rawFiles) {
      const ext = getExtension(f);
      if (ext) counts.set(ext, (counts.get(ext) ?? 0) + 1);
    }
    return [...counts.entries()].sort((a, b) => b[1] - a[1]);
  }, [rawFiles]);

  // Apply ext filter + sort
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

  if (view === "create" || view === "edit") {
    return (
      <div className="page">
        <h1>{view === "create" ? "New Project" : "Edit Project"}</h1>
        <div className="project-form">
          <div className="form-group">
            <label>Title</label>
            <input type="text" value={title} onChange={(e) => setTitle(e.target.value)} />
          </div>
          <div className="form-group">
            <label>Description</label>
            <textarea
              className="project-textarea"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={3}
            />
          </div>
          <div className="form-row">
            <div className="form-group">
              <label>Start Date</label>
              <input
                type="date"
                className="date-input"
                value={startDate}
                onChange={(e) => setStartDate(e.target.value)}
              />
            </div>
            <div className="form-group">
              <label>End Date</label>
              <input
                type="date"
                className="date-input"
                value={endDate}
                onChange={(e) => setEndDate(e.target.value)}
              />
            </div>
          </div>
          <div className="form-actions">
            <button onClick={() => setView(selected ? "detail" : "list")}>Cancel</button>
            <button className="btn-primary" onClick={handleSubmit} disabled={!title || !startDate || !endDate}>
              {view === "create" ? "Create" : "Save"}
            </button>
          </div>
        </div>
      </div>
    );
  }

  if (view === "detail" && selected) {
    const { project, stats } = selected;
    return (
      <div className="page">
        <div className="page-header">
          <div>
            <h1>{project.title}</h1>
            {project.description && <p className="text-muted-color mt-4">{project.description}</p>}
            <p className="text-muted-color text-xs mt-4">
              {project.startDate} — {project.endDate}
            </p>
          </div>
          <div className="flex-row gap-8">
            <button onClick={() => { setSelected(null); setView("list"); }}>Back</button>
            <button onClick={() => openEdit(project)}>Edit</button>
            <button onClick={() => setShowTransfer(true)}>Transfer to...</button>
            <button className="btn-danger" onClick={() => handleDeleteProject(project.id)}>Delete</button>
          </div>
        </div>

        <div className="stats-grid mb-20">
          <div className="stat-card">
            <div className="stat-value">{stats.totalFiles}</div>
            <div className="stat-label">Files</div>
          </div>
          <div className="stat-card">
            <div className="stat-value">{formatBytes(stats.totalSizeBytes)}</div>
            <div className="stat-label">Total Size</div>
          </div>
          <div className="stat-card">
            <div className="stat-value">{stats.backedUpPct.toFixed(0)}%</div>
            <div className="stat-label">Backed Up</div>
          </div>
        </div>

        <div className="browser-controls">
          <div className="filter-toggle">
            <button className={safetyFilter === "all" ? "active" : ""} onClick={() => { setSafetyFilter("all"); setExtFilter(""); setDupDeviceFilter(""); }}>
              All
            </button>
            <button className={safetyFilter === "safe" ? "active" : ""} onClick={() => { setSafetyFilter("safe"); setExtFilter(""); setDupDeviceFilter(""); }}>
              Safe Only
            </button>
            <button className={safetyFilter === "unsafe" ? "active" : ""} onClick={() => { setSafetyFilter("unsafe"); setExtFilter(""); setDupDeviceFilter(""); }}>
              Unsafe Only
            </button>
            <button className={safetyFilter === "duplicates" ? "active" : ""} onClick={() => { setSafetyFilter("duplicates"); setExtFilter(""); }}>
              Duplicates
            </button>
          </div>
          {safetyFilter === "duplicates" && (
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
              <button className={sortKey === "name" ? "active" : ""} onClick={() => toggleSort("name")}>
                Name {sortKey === "name" ? (sortDir === "asc" ? "\u2191" : "\u2193") : ""}
              </button>
              <button className={sortKey === "size" ? "active" : ""} onClick={() => toggleSort("size")}>
                Size {sortKey === "size" ? (sortDir === "asc" ? "\u2191" : "\u2193") : ""}
              </button>
              <button className={sortKey === "modified" ? "active" : ""} onClick={() => toggleSort("modified")}>
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
            deviceNames={deviceNames}
            connectedDeviceIds={connectedDeviceIds}
            onGetSafety={(hash) => getFileSafety(hash)}
            onDeleteLocation={safetyFilter === "duplicates" ? handleDeleteFileCopy : undefined}
          />
        )}
        {!loading && displayFiles.length === 0 && (
          <p className="empty">
            {safetyFilter === "all"
              ? "No files match this date range"
              : safetyFilter === "safe"
                ? "No safe files"
                : safetyFilter === "unsafe"
                  ? "No unsafe files"
                  : "No files with more than 2 copies"}
          </p>
        )}

        {showBulkDelete && (
          <BulkDeleteModal
            deviceName={bulkDeleteDeviceName}
            files={bulkDeleteTargets}
            onConfirm={handleBulkDelete}
            onClose={() => setShowBulkDelete(false)}
          />
        )}

        {showTransfer && (
          <TransferModal
            projectId={project.id}
            projectTitle={project.title}
            devices={devices}
            onClose={() => { setShowTransfer(false); refreshDevices(); }}
          />
        )}
      </div>
    );
  }

  // List view
  return (
    <div className="page">
      <div className="page-header">
        <h1>Projects</h1>
        <button className="btn-primary" onClick={openCreate}>New Project</button>
      </div>

      {loading ? (
        <div className="project-grid">
          {[...Array(3)].map((_, i) => <div key={i} className="skeleton skeleton-card" />)}
        </div>
      ) : projects.length === 0 ? (
        <p className="empty">No projects yet</p>
      ) : (
      <div className="project-grid">
        {projects.map((p) => (
          <div key={p.id} className="project-card" onClick={() => openDetail(p.id)}>
            <h3>{p.title}</h3>
            {p.description && (
              <p className="text-muted-color text-sm mt-4">
                {p.description.length > 80 ? p.description.slice(0, 80) + "…" : p.description}
              </p>
            )}
            <p className="text-muted-color text-xs mt-8">
              {p.startDate} — {p.endDate}
            </p>
          </div>
        ))}
      </div>
      )}
    </div>
  );
}
