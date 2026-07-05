import { useState, useEffect } from "react";
import { useBackup } from "../hooks/useBackup";

export function Backup() {
  const { settings, loading, phase, progress, error, totalRows, save, test, backup, resetStatus } =
    useBackup();

  const [host, setHost] = useState("");
  const [port, setPort] = useState("5432");
  const [database, setDatabase] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [status, setStatus] = useState<{ kind: "ok" | "err"; text: string } | null>(null);

  // Populate form from saved settings
  useEffect(() => {
    if (settings) {
      setHost(settings.host);
      setPort(String(settings.port));
      setDatabase(settings.database);
      setUsername(settings.username);
    }
  }, [settings]);

  const portNum = parseInt(port, 10);
  const portValid = !isNaN(portNum) && portNum >= 1 && portNum <= 65535;
  const passwordRequired = !settings?.hasPassword || username.trim() !== settings?.username;
  const canSave =
    host.trim() && database.trim() && username.trim() && portValid &&
    (!passwordRequired || password);
  const configured = settings !== null && settings.hasPassword;

  const handleSave = async () => {
    if (!canSave) return;
    setSaving(true);
    setStatus(null);
    try {
      await save(host.trim(), portNum, database.trim(), username.trim(), password);
      setPassword("");
      setStatus({ kind: "ok", text: "Connection details saved" });
    } catch (e) {
      setStatus({ kind: "err", text: String(e) });
    } finally {
      setSaving(false);
    }
  };

  const handleTest = async () => {
    setTesting(true);
    setStatus(null);
    try {
      await test();
      setStatus({ kind: "ok", text: "Connection successful" });
    } catch (e) {
      setStatus({ kind: "err", text: String(e) });
    } finally {
      setTesting(false);
    }
  };

  const pct =
    progress && progress.totalRows > 0
      ? Math.round((progress.rowsCopied / progress.totalRows) * 100)
      : 0;

  return (
    <div className="page">
      <h1>Backup</h1>
      <p className="text-muted-color text-sm" style={{ marginBottom: 16 }}>
        Mirror the local database to a PostgreSQL server. The local database stays the source of
        truth — each backup replaces the server copy.
      </p>

      {loading ? (
        <div className="skeleton skeleton-card" />
      ) : (
        <>
          <div className="import-section">
            <h3>PostgreSQL Connection</h3>
            <div className="form-row">
              <div className="form-group" style={{ flex: 3 }}>
                <label>Host</label>
                <input
                  type="text"
                  value={host}
                  onChange={(e) => setHost(e.target.value)}
                  placeholder="db.example.com or 192.168.1.10"
                />
              </div>
              <div className="form-group" style={{ flex: 1 }}>
                <label>Port</label>
                <input
                  type="text"
                  value={port}
                  onChange={(e) => setPort(e.target.value)}
                  placeholder="5432"
                />
              </div>
            </div>
            <div className="form-group">
              <label>Database</label>
              <input
                type="text"
                value={database}
                onChange={(e) => setDatabase(e.target.value)}
                placeholder="filemanager_backup"
              />
            </div>
            <div className="form-group">
              <label>User</label>
              <input
                type="text"
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                placeholder="postgres"
              />
            </div>
            <div className="form-group">
              <label>Password</label>
              <input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder={
                  settings?.hasPassword && !passwordRequired
                    ? "Saved in Keychain — leave empty to keep"
                    : "Stored in Keychain"
                }
              />
            </div>
            {status && (
              <div
                className={status.kind === "err" ? "error-msg" : "success-msg"}
                style={{ marginBottom: 12 }}
              >
                {status.text}
              </div>
            )}
            <div className="form-actions">
              <button onClick={handleTest} disabled={testing || !configured}>
                {testing ? "Testing..." : "Test Connection"}
              </button>
              <button className="btn-primary" onClick={handleSave} disabled={!canSave || saving}>
                {saving ? "Saving..." : "Save"}
              </button>
            </div>
          </div>

          <div className="import-section" style={{ marginTop: 16 }}>
            <h3>Backup</h3>
            {settings?.lastBackupAt ? (
              <p className="text-muted-color text-sm">Last backup: {settings.lastBackupAt}</p>
            ) : (
              <p className="text-muted-color text-sm">No backup yet</p>
            )}

            {phase === "running" && progress && (
              <div className="progress-container">
                <div className="progress-bar">
                  <div className="progress-fill" style={{ width: `${pct}%` }} />
                </div>
                <div className="progress-stats">
                  <span>
                    {progress.table
                      ? `${progress.table}: ${progress.rowsCopied.toLocaleString()} / ${progress.totalRows.toLocaleString()} rows`
                      : "Connecting..."}
                  </span>
                  <span>
                    Table {Math.min(progress.tablesDone + 1, progress.totalTables)} / {progress.totalTables}
                  </span>
                </div>
              </div>
            )}

            {phase === "done" && (
              <div className="success-msg" style={{ marginBottom: 12 }}>
                Backup complete — {totalRows.toLocaleString()} rows copied to {host}
              </div>
            )}
            {phase === "error" && error && (
              <div className="error-msg" style={{ marginBottom: 12 }}>{error}</div>
            )}

            <div className="form-actions">
              {(phase === "done" || phase === "error") && (
                <button onClick={resetStatus}>Dismiss</button>
              )}
              <button
                className="btn-primary"
                onClick={backup}
                disabled={phase === "running" || !configured}
              >
                {phase === "running" ? "Backing up..." : "Backup Now"}
              </button>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
