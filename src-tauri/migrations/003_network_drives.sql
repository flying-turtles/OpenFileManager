CREATE TABLE IF NOT EXISTS network_drives (
    id          TEXT PRIMARY KEY,
    label       TEXT NOT NULL,
    protocol    TEXT NOT NULL,
    host        TEXT NOT NULL,
    share_path  TEXT NOT NULL,
    username    TEXT NOT NULL DEFAULT '',
    mount_point TEXT NOT NULL,
    device_type TEXT NOT NULL DEFAULT 'unknown',
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
