CREATE TABLE IF NOT EXISTS image_hashes (
    blake3_hash TEXT PRIMARY KEY,
    dhash       INTEGER,  -- NULL = decode failed / unsupported format
    hashed_at   TEXT NOT NULL DEFAULT (datetime('now'))
);
