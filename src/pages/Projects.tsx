import { useEffect, useState, useMemo } from "react";
import { useProjects } from "../hooks/useProjects";
import { useDevices } from "../hooks/useDevices";
import { FileTable } from "../components/FileTable";
import { getFileSafety, deleteFileCopy } from "../api/commands";
import type { Project, FileLocation, FileSafety } from "../types";

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
  const dot = f.file_name.lastIndexOf(".");
  return dot > 0 ? f.file_name.slice(dot + 1).toLowerCase() : "";
}

function sortFiles(files: FileLocation[], key: SortKey, dir: SortDir): FileLocation[] {
  const sorted = [...files].sort((a, b) => {
    if (key === "size") return a.file_size - b.file_size;
    if (key === "modified") return (a.modified_at ?? "").localeCompare(b.modified_at ?? "");
    return a.file_name.localeCompare(b.file_name);
  });
  return dir === "desc" ? sorted.reverse() : sorted;
}

export function Projects() {
  const { projects, selected, loading, refresh, select, create, update, remove, setSelected } =
    useProjects();
  const { devices } = useDevices();
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

  useEffect(() => {
    refresh();
  }, [refresh]);

  const deviceNames = useMemo(() => {
    const map: Record<string, string> = {};
    for (const d of devices) map[d.id] = d.label;
    return map;
  }, [devices]);

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
    setStartDate(p.start_date);
    setEndDate(p.end_date);
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
          total_copies: sf.total_copies - (sf.locations.some((l) => l.id === locationId) ? 1 : 0),
          locations: sf.locations.filter((l) => l.id !== locationId),
        }))
        .filter((sf) => sf.locations.length > 0)
    );
  };

  // Filter project files by safety status
  const filteredSafetyFiles: FileSafety[] = useMemo(() => {
    if (safetyFilter === "safe") return projectFiles.filter((sf) => sf.is_safe);
    if (safetyFilter === "unsafe") return projectFiles.filter((sf) => !sf.is_safe);
    if (safetyFilter === "duplicates") return projectFiles.filter((sf) => sf.total_copies > 2);
    return projectFiles;
  }, [projectFiles, safetyFilter]);

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
            {project.description && <p style={{ color: "var(--text-muted)", marginTop: 4 }}>{project.description}</p>}
            <p style={{ color: "var(--text-muted)", fontSize: 12, marginTop: 4 }}>
              {project.start_date} — {project.end_date}
            </p>
          </div>
          <div style={{ display: "flex", gap: 8 }}>
            <button onClick={() => { setSelected(null); setView("list"); }}>Back</button>
            <button onClick={() => openEdit(project)}>Edit</button>
            <button className="btn-danger" onClick={() => handleDeleteProject(project.id)}>Delete</button>
          </div>
        </div>

        <div className="stats-grid" style={{ marginBottom: 20 }}>
          <div className="stat-card">
            <div className="stat-value">{stats.total_files}</div>
            <div className="stat-label">Files</div>
          </div>
          <div className="stat-card">
            <div className="stat-value">{formatBytes(stats.total_size_bytes)}</div>
            <div className="stat-label">Total Size</div>
          </div>
          <div className="stat-card">
            <div className="stat-value">{stats.backed_up_pct.toFixed(0)}%</div>
            <div className="stat-label">Backed Up</div>
          </div>
        </div>

        <div className="browser-controls">
          <div className="filter-toggle">
            <button className={safetyFilter === "all" ? "active" : ""} onClick={() => { setSafetyFilter("all"); setExtFilter(""); }}>
              All
            </button>
            <button className={safetyFilter === "safe" ? "active" : ""} onClick={() => { setSafetyFilter("safe"); setExtFilter(""); }}>
              Safe Only
            </button>
            <button className={safetyFilter === "unsafe" ? "active" : ""} onClick={() => { setSafetyFilter("unsafe"); setExtFilter(""); }}>
              Unsafe Only
            </button>
            <button className={safetyFilter === "duplicates" ? "active" : ""} onClick={() => { setSafetyFilter("duplicates"); setExtFilter(""); }}>
              Duplicates
            </button>
          </div>
        </div>

        {rawFiles.length > 0 && (
          <div className="browser-controls" style={{ marginTop: 0 }}>
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
          <div>Loading...</div>
        ) : (
          <FileTable
            files={displayFiles}
            deviceNames={deviceNames}
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

      {loading && <div>Loading...</div>}

      {!loading && projects.length === 0 && (
        <p className="empty">No projects yet</p>
      )}

      <div className="project-grid">
        {projects.map((p) => (
          <div key={p.id} className="project-card" onClick={() => openDetail(p.id)}>
            <h3>{p.title}</h3>
            {p.description && (
              <p style={{ color: "var(--text-muted)", fontSize: 13, marginTop: 4 }}>
                {p.description.length > 80 ? p.description.slice(0, 80) + "…" : p.description}
              </p>
            )}
            <p style={{ color: "var(--text-muted)", fontSize: 12, marginTop: 8 }}>
              {p.start_date} — {p.end_date}
            </p>
          </div>
        ))}
      </div>
    </div>
  );
}
