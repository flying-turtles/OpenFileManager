import { useState, useEffect, useCallback } from "react";
import type { StorageDevice } from "./types";
import { Dashboard } from "./pages/Dashboard";
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
        <button className={page === "files" ? "active" : ""} onClick={() => setPage("files")}>
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
        {page === "dashboard" && <Dashboard />}
        {page === "devices" && <Devices onScanDevice={handleScanDevice} />}
        <div className={page === "scanner" ? "contents-display" : "hidden-display"}>
          <Scanner initialDevice={scanDevice} />
        </div>
        {page === "files" && <FileBrowser />}
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
