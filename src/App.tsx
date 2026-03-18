import { useState, useEffect, useCallback } from "react";
import type { StorageDevice } from "./types";
import { Dashboard } from "./pages/Dashboard";
import { Devices } from "./pages/Devices";
import { Scanner } from "./pages/Scanner";
import { FileBrowser } from "./pages/FileBrowser";
import { Import } from "./pages/Import";
import { Projects } from "./pages/Projects";
import { Transfer } from "./pages/Transfer";
import "./App.css";

type Page = "dashboard" | "devices" | "scanner" | "files" | "import" | "transfer" | "projects";

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
          <Transfer projectId={transferProject?.id ?? null} projectTitle={transferProject?.title ?? ""} />
        </div>
        {page === "projects" && <Projects onTransferProject={handleTransferProject} />}
      </main>
    </div>
  );
}

export default App;
