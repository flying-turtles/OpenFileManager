import { useState, useEffect, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import { getDevices } from "./api/commands";
import type { StorageDevice } from "./types";
import { Dashboard, type FilesFilter } from "./pages/Dashboard";
import { Devices } from "./pages/Devices";
import { Scanner } from "./pages/Scanner";
import { FileBrowser } from "./pages/FileBrowser";
import { Import } from "./pages/Import";
import { Projects } from "./pages/Projects";
import { Transfer } from "./pages/Transfer";
import { Backup } from "./pages/Backup";
import { Similar } from "./pages/Similar";
import "./App.css";

type Page = "dashboard" | "devices" | "scanner" | "files" | "import" | "transfer" | "projects" | "backup" | "similar";

function App() {
  const [page, setPage] = useState<Page>("dashboard");
  const [scanDevice, setScanDevice] = useState<StorageDevice | undefined>();
  const [transferProject, setTransferProject] = useState<{ id: number; title: string } | null>(null);
  const [filesFilter, setFilesFilter] = useState<FilesFilter>("all");
  const [connectPrompts, setConnectPrompts] = useState<StorageDevice[]>([]);

  // Offer a scan when a drive that is already in the index reconnects
  useEffect(() => {
    const unlisten = listen<{ connected: string[] }>("devices-changed", async (event) => {
      const ids = event.payload?.connected ?? [];
      if (ids.length === 0) return;
      try {
        const known = await getDevices();
        const reconnected = known.filter((d) => ids.includes(d.id));
        if (reconnected.length > 0) {
          setConnectPrompts((prev) => {
            const have = new Set(prev.map((d) => d.id));
            return [...prev, ...reconnected.filter((d) => !have.has(d.id))];
          });
        }
      } catch (e) {
        console.error("Failed to resolve connected devices:", e);
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);
  const [theme, setTheme] = useState(() => localStorage.getItem("theme") || "system");

  useEffect(() => {
    if (theme === "system") {
      document.documentElement.removeAttribute("data-theme");
    } else {
      document.documentElement.setAttribute("data-theme", theme);
    }
    localStorage.setItem("theme", theme);
  }, [theme]);

  const handleScanDevice = (device: StorageDevice) => {
    setScanDevice(device);
    setPage("scanner");
  };

  const handleTransferProject = useCallback((id: number, title: string) => {
    setTransferProject({ id, title });
    setPage("transfer");
  }, []);

  const handleOpenFiles = useCallback((filter: FilesFilter) => {
    setFilesFilter(filter);
    setPage("files");
  }, []);

  return (
    <div className="app">
      <nav className="sidebar">
        <div className="nav-title">FileManager</div>
        <button className={page === "dashboard" ? "active" : ""} onClick={() => setPage("dashboard")}>
          Dashboard
        </button>
        <button className={page === "devices" ? "active" : ""} onClick={() => setPage("devices")}>
          Devices
        </button>
        <button className={page === "scanner" ? "active" : ""} onClick={() => setPage("scanner")}>
          Scanner
        </button>
        <button className={page === "files" ? "active" : ""} onClick={() => { setFilesFilter("all"); setPage("files"); }}>
          Files
        </button>
        <button className={page === "import" ? "active" : ""} onClick={() => setPage("import")}>
          Import
        </button>
        <button className={page === "transfer" ? "active" : ""} onClick={() => setPage("transfer")}>
          Transfer
        </button>
        <button className={page === "projects" ? "active" : ""} onClick={() => setPage("projects")}>
          Projects
        </button>
        <button className={page === "similar" ? "active" : ""} onClick={() => setPage("similar")}>
          Similar
        </button>
        <button className={page === "backup" ? "active" : ""} onClick={() => setPage("backup")}>
          Backup
        </button>
        <button
          className="theme-toggle"
          onClick={() => setTheme(t => t === "dark" ? "light" : t === "light" ? "system" : "dark")}
          aria-label="Toggle theme"
        >
          {theme === "dark" ? "Dark" : theme === "light" ? "Light" : "System"}
        </button>
      </nav>
      <main className="content">
        {connectPrompts.map((d) => (
          <div key={d.id} className="connect-banner">
            <span>
              <strong>{d.label}</strong> connected ({d.mountPoint})
            </span>
            <div className="flex-row gap-8">
              <button
                className="btn-primary"
                onClick={() => {
                  setConnectPrompts((prev) => prev.filter((p) => p.id !== d.id));
                  handleScanDevice({ ...d, isConnected: true });
                }}
              >
                Scan Now
              </button>
              <button onClick={() => setConnectPrompts((prev) => prev.filter((p) => p.id !== d.id))}>
                Dismiss
              </button>
            </div>
          </div>
        ))}
        {page === "dashboard" && (
          <Dashboard onOpenFiles={handleOpenFiles} onOpenDevices={() => setPage("devices")} />
        )}
        <div className={page === "devices" ? "contents-display" : "hidden-display"}>
          <Devices onScanDevice={handleScanDevice} />
        </div>
        <div className={page === "scanner" ? "contents-display" : "hidden-display"}>
          <Scanner initialDevice={scanDevice} />
        </div>
        {page === "files" && <FileBrowser initialFilter={filesFilter} />}
        <div className={page === "import" ? "contents-display" : "hidden-display"}>
          <Import />
        </div>
        <div className={page === "transfer" ? "contents-display" : "hidden-display"}>
          <Transfer project={transferProject} />
        </div>
        {page === "projects" && <Projects onTransferProject={handleTransferProject} />}
        <div className={page === "backup" ? "contents-display" : "hidden-display"}>
          <Backup />
        </div>
        <div className={page === "similar" ? "contents-display" : "hidden-display"}>
          <Similar />
        </div>
      </main>
    </div>
  );
}

export default App;
