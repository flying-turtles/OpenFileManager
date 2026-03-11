CREATE TABLE IF NOT EXISTS dir_cache (
    device_id TEXT NOT NULL,
    dir_path TEXT NOT NULL,
    dir_mtime TEXT NOT NULL,
    file_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (device_id, dir_path),
    FOREIGN KEY (device_id) REFERENCES storage_devices(id) ON DELETE CASCADE
);
