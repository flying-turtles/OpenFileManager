import { useState } from "react";
import type { StorageDevice } from "./types";
import { Dashboard } from "./pages/Dashboard";
import { Devices } from "./pages/Devices";
import { Scanner } from "./pages/Scanner";
import { FileBrowser } from "./pages/FileBrowser";
import "./App.css";

type Page = "dashboard" | "devices" | "scanner" | "files";

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
      </nav>
      <main className="content">
        {page === "dashboard" && <Dashboard />}
        {page === "devices" && <Devices onScanDevice={handleScanDevice} />}
        {page === "scanner" && <Scanner initialDevice={scanDevice} />}
        {page === "files" && <FileBrowser />}
      </main>
    </div>
  );
}

export default App;
