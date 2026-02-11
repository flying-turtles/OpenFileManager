import { useEffect, useState } from "react";
import { useDevices } from "../hooks/useDevices";
import { useFiles } from "../hooks/useFiles";
import { FileTable } from "../components/FileTable";

export function FileBrowser() {
  const { devices } = useDevices();
  const { files, unsafeFiles, loading, loadDeviceFiles, loadUnsafeFiles, loadFileSafety } =
    useFiles();
  const [selectedDevice, setSelectedDevice] = useState("");
  const [filter, setFilter] = useState<"all" | "unsafe">("all");

  useEffect(() => {
    if (filter === "unsafe") {
      loadUnsafeFiles();
    } else if (selectedDevice) {
      loadDeviceFiles(selectedDevice);
    }
  }, [selectedDevice, filter, loadDeviceFiles, loadUnsafeFiles]);

  // Build a flat file list from unsafe files for display
  const displayFiles =
    filter === "unsafe"
      ? unsafeFiles.flatMap((sf) =>
          sf.locations.map((loc) => ({
            ...loc,
            _safety: sf,
          }))
        )
      : files;

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
            className={filter === "unsafe" ? "active" : ""}
            onClick={() => setFilter("unsafe")}
          >
            Unsafe Only
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
                {d.label} ({d.mount_point})
              </option>
            ))}
          </select>
        )}
      </div>

      {loading ? (
        <div>Loading...</div>
      ) : (
        <FileTable files={displayFiles} onGetSafety={loadFileSafety} />
      )}

      {!loading && displayFiles.length === 0 && (
        <p className="empty">
          {filter === "unsafe" ? "No unsafe files found" : "Select a device to view files"}
        </p>
      )}
    </div>
  );
}
