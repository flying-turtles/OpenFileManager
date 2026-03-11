import { useState } from "react";

interface Props {
  onAdd: (
    protocol: string,
    host: string,
    sharePath: string,
    username: string,
    password: string,
    label: string,
    deviceType: string
  ) => Promise<unknown>;
  onClose: () => void;
}

export function AddNetworkDriveModal({ onAdd, onClose }: Props) {
  const [protocol, setProtocol] = useState("smb");
  const [host, setHost] = useState("");
  const [sharePath, setSharePath] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [label, setLabel] = useState("");
  const [deviceType, setDeviceType] = useState("unknown");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");

  const canSubmit = host.trim() && sharePath.trim() && label.trim();

  const handleSubmit = async () => {
    if (!canSubmit) return;
    setSubmitting(true);
    setError("");
    try {
      await onAdd(protocol, host.trim(), sharePath.trim(), username.trim(), password, label.trim(), deviceType);
      onClose();
    } catch (e: any) {
      setError(String(e));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-content" onClick={(e) => e.stopPropagation()}>
        <h2>Add Network Drive</h2>
        {error && <div className="error-msg">{error}</div>}
        <div className="form-group">
          <label>Protocol</label>
          <select value={protocol} onChange={(e) => setProtocol(e.target.value)}>
            <option value="smb">SMB</option>
            <option value="nfs">NFS</option>
          </select>
        </div>
        <div className="form-group">
          <label>Host</label>
          <input type="text" value={host} onChange={(e) => setHost(e.target.value)} placeholder="192.168.1.100 or nas.local" />
        </div>
        <div className="form-group">
          <label>{protocol === "smb" ? "Share Name" : "Export Path"}</label>
          <input type="text" value={sharePath} onChange={(e) => setSharePath(e.target.value)} placeholder={protocol === "smb" ? "Photos" : "/volume1/photos"} />
        </div>
        {protocol === "smb" && (
          <>
            <div className="form-group">
              <label>Username</label>
              <input type="text" value={username} onChange={(e) => setUsername(e.target.value)} placeholder="optional" />
            </div>
            <div className="form-group">
              <label>Password</label>
              <input type="password" value={password} onChange={(e) => setPassword(e.target.value)} placeholder="stored in Keychain" />
            </div>
          </>
        )}
        <div className="form-group">
          <label>Label</label>
          <input type="text" value={label} onChange={(e) => setLabel(e.target.value)} placeholder="NAS Photos" />
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
            {submitting ? "Adding..." : "Add Drive"}
          </button>
        </div>
      </div>
    </div>
  );
}
