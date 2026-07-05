CREATE TABLE IF NOT EXISTS backup_settings (
    id             INTEGER PRIMARY KEY CHECK (id = 1),
    host           TEXT NOT NULL,
    port           INTEGER NOT NULL DEFAULT 5432,
    db_name        TEXT NOT NULL,
    username       TEXT NOT NULL,
    last_backup_at TEXT
);
