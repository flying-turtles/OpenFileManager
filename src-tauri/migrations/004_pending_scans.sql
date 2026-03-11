CREATE TABLE IF NOT EXISTS pending_scans (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    scan_type   TEXT NOT NULL,  -- 'scan' or 'import'
    target      TEXT NOT NULL,
    device_id   TEXT NOT NULL,
    mode        TEXT NOT NULL DEFAULT 'quick',
    total_files INTEGER NOT NULL DEFAULT 0,
    processed   INTEGER NOT NULL DEFAULT 0,
    hashed      INTEGER NOT NULL DEFAULT 0,
    added       INTEGER NOT NULL DEFAULT 0,
    paused_at   TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(scan_type, target, device_id)
);
