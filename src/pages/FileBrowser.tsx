import { useEffect, useState } from "react";
import { useDevices } from "../hooks/useDevices";
import { useFiles } from "../hooks/useFiles";
import { FileTable } from "../components/FileTable";

export function FileBrowser() {
  const { devices } = useDevices();
  const {
    files, unsafeFiles, safeFiles, loading, totalCount,
    loadDeviceFiles, loadUnsafeFiles, loadSafeFiles, loadFileSafety,
  } = useFiles();
  const [selectedDevice, setSelectedDevice] = useState("");
  const [filter, setFilter] = useState<"all" | "unsafe" | "safe">("all");

  useEffect(() => {
    if (filter === "unsafe") {
      loadUnsafeFiles();
    } else if (filter === "safe") {
      loadSafeFiles();
    } else if (selectedDevice) {
      loadDeviceFiles(selectedDevice);
    }
  }, [selectedDevice, filter, loadDeviceFiles, loadUnsafeFiles, loadSafeFiles]);

  const displayFiles =
    filter === "unsafe"
      ? unsafeFiles.flatMap((sf) =>
          sf.locations.map((loc) => ({ ...loc, _safety: sf }))
        )
      : filter === "safe"
        ? safeFiles.flatMap((sf) =>
            sf.locations.map((loc) => ({ ...loc, _safety: sf }))
          )
        : files;

  const emptyMsg =
    filter === "unsafe"
      ? "No unsafe files found"
      : filter === "safe"
        ? "No safe files found"
        : "Select a device to view files";

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
            className={filter === "safe" ? "active" : ""}
            onClick={() => setFilter("safe")}
          >
            Safe Only
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
        <FileTable files={displayFiles} totalCount={totalCount} onGetSafety={loadFileSafety} />
      )}

      {!loading && displayFiles.length === 0 && (
        <p className="empty">{emptyMsg}</p>
      )}
    </div>
  );
}
