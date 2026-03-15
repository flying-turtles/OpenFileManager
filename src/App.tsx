import { useState, useEffect } from "react";
import type { StorageDevice } from "./types";
import { Dashboard } from "./pages/Dashboard";
import { Devices } from "./pages/Devices";
import { Scanner } from "./pages/Scanner";
import { FileBrowser } from "./pages/FileBrowser";
import { Import } from "./pages/Import";
import { Projects } from "./pages/Projects";
import "./App.css";

type Page = "dashboard" | "devices" | "scanner" | "files" | "import" | "projects";

function App() {
  const [page, setPage] = useState<Page>("dashboard");
  const [scanDevice, setScanDevice] = useState<StorageDevice | undefined>();
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
        {page === "projects" && <Projects />}
      </main>
    </div>
  );
}

export default App;
