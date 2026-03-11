import { useState } from "react";
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
      </nav>
      <main className="content">
        {page === "dashboard" && <Dashboard />}
        {page === "devices" && <Devices onScanDevice={handleScanDevice} />}
        <div style={{ display: page === "scanner" ? "contents" : "none" }}>
          <Scanner initialDevice={scanDevice} />
        </div>
        {page === "files" && <FileBrowser />}
        <div style={{ display: page === "import" ? "contents" : "none" }}>
          <Import />
        </div>
        {page === "projects" && <Projects />}
      </main>
    </div>
  );
}

export default App;
