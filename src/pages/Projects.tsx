import { useEffect, useState } from "react";
import { useProjects } from "../hooks/useProjects";
import { FileTable } from "../components/FileTable";
import { getFileSafety } from "../api/commands";
import type { Project } from "../types";

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
}

type View = "list" | "detail" | "create" | "edit";

export function Projects() {
  const { projects, selected, loading, refresh, select, create, update, remove, setSelected } =
    useProjects();
  const [view, setView] = useState<View>("list");

  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [startDate, setStartDate] = useState("");
  const [endDate, setEndDate] = useState("");

  useEffect(() => {
    refresh();
  }, [refresh]);

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

  const openDetail = async (id: number) => {
    await select(id);
    setView("detail");
  };

  const handleDelete = async (id: number) => {
    if (!confirm("Delete this project?")) return;
    await remove(id);
    setView("list");
  };

  // Flatten FileSafety[] to FileLocation[] for FileTable
  const detailFiles = selected
    ? selected.files.flatMap((sf) =>
        sf.locations.map((loc) => ({ ...loc, _safety: sf }))
      )
    : [];

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
            <button className="btn-danger" onClick={() => handleDelete(project.id)}>Delete</button>
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

        {stats.extensions.length > 0 && (
          <div style={{ marginBottom: 20, display: "flex", flexWrap: "wrap", gap: 6 }}>
            {stats.extensions.map((e) => (
              <span key={e.extension} className="ext-chip">
                {e.extension || "no ext"} ({e.count})
              </span>
            ))}
          </div>
        )}

        {loading ? (
          <div>Loading...</div>
        ) : (
          <FileTable files={detailFiles} onGetSafety={(hash) => getFileSafety(hash)} />
        )}
        {!loading && detailFiles.length === 0 && (
          <p className="empty">No files match this date range</p>
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
