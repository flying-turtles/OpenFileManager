CREATE TABLE IF NOT EXISTS storage_devices (
    id              TEXT PRIMARY KEY,  -- volume UUID
    label           TEXT NOT NULL,
    mount_point     TEXT NOT NULL,
    device_type     TEXT NOT NULL DEFAULT 'unknown',  -- hot/cold/unknown
    total_bytes     INTEGER NOT NULL DEFAULT 0,
    available_bytes INTEGER NOT NULL DEFAULT 0,
    is_removable    INTEGER NOT NULL DEFAULT 0,
    first_seen      TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen       TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS files (
    blake3_hash        TEXT PRIMARY KEY,
    file_size          INTEGER NOT NULL,
    representative_name TEXT NOT NULL,
    extension          TEXT NOT NULL DEFAULT '',
    created_at         TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS file_locations (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    blake3_hash   TEXT NOT NULL REFERENCES files(blake3_hash),
    device_id     TEXT NOT NULL REFERENCES storage_devices(id),
    file_path     TEXT NOT NULL,  -- relative to mount point
    file_name     TEXT NOT NULL,
    file_size     INTEGER NOT NULL,
    modified_at   TEXT,
    last_verified TEXT NOT NULL DEFAULT (datetime('now')),
    scan_mode     TEXT NOT NULL DEFAULT 'full',  -- quick/full/deferred
    UNIQUE(device_id, file_path)
);

CREATE INDEX IF NOT EXISTS idx_locations_hash ON file_locations(blake3_hash);
CREATE INDEX IF NOT EXISTS idx_locations_device ON file_locations(device_id);
CREATE INDEX IF NOT EXISTS idx_files_extension ON files(extension);
CREATE INDEX IF NOT EXISTS idx_files_size ON files(file_size);
