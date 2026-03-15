import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useFocusTrap } from "../hooks/useFocusTrap";

interface Props {
  onAdd: (path: string, label: string, deviceType: string) => Promise<unknown>;
  onClose: () => void;
}

export function AddLocationModal({ onAdd, onClose }: Props) {
  const trapRef = useFocusTrap<HTMLDivElement>();
  const [path, setPath] = useState("");
  const [label, setLabel] = useState("");
  const [deviceType, setDeviceType] = useState("unknown");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");

  const canSubmit = path.trim() && label.trim();

  const handleBrowse = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (selected) {
      setPath(selected);
      if (!label) {
        const name = selected.split("/").filter(Boolean).pop() ?? "";
        setLabel(name);
      }
    }
  };

  const handleSubmit = async () => {
    if (!canSubmit) return;
    setSubmitting(true);
    setError("");
    try {
      await onAdd(path.trim(), label.trim(), deviceType);
      onClose();
    } catch (e: any) {
      setError(String(e));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="modal-overlay" onClick={onClose} role="dialog" aria-label="Add location" aria-modal="true" onKeyDown={(e) => { if (e.key === "Escape") onClose(); }}>
      <div className="modal-content" ref={trapRef} onClick={(e) => e.stopPropagation()}>
        <h2>Add Location</h2>
        {error && <div className="error-msg">{error}</div>}
        <div className="form-group">
          <label>Folder</label>
          <div className="flex-row gap-8">
            <input
              type="text"
              value={path}
              onChange={(e) => setPath(e.target.value)}
              placeholder="/Volumes/MyDrive"
              className="flex-1"
            />
            <button onClick={handleBrowse}>Browse</button>
          </div>
        </div>
        <div className="form-group">
          <label>Label</label>
          <input type="text" value={label} onChange={(e) => setLabel(e.target.value)} placeholder="NAS Media" />
        </div>
        <div className="form-group">
          <label>Device Type</label>
          <select value={deviceType} onChange={(e) => setDeviceType(e.target.value)}>
            <option value="unknown">Unknown</option>
            <option value="hot">Hot</option>
            <option value="cold">Cold</option>
            <option value="production">Production</option>
          </select>
        </div>
        <div className="form-actions">
          <button onClick={onClose}>Cancel</button>
          <button className="btn-primary" onClick={handleSubmit} disabled={!canSubmit || submitting}>
            {submitting ? "Adding..." : "Add Location"}
          </button>
        </div>
      </div>
    </div>
  );
}
