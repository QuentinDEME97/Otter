CREATE TABLE IF NOT EXISTS clients (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    public_key BLOB NOT NULL,
    api_key_fingerprint TEXT UNIQUE NOT NULL,
    api_key_hash TEXT NOT NULL,
    registered_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS files (
    id TEXT PRIMARY KEY,
    owner_client_id TEXT NOT NULL,
    path TEXT NOT NULL UNIQUE,
    size INTEGER NOT NULL,
    checksum TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    last_modified TEXT NOT NULL,
    FOREIGN KEY (owner_client_id) REFERENCES clients(id)
);

CREATE TABLE IF NOT EXISTS file_logs (
    id TEXT PRIMARY KEY,
    file_id TEXT,
    client_id TEXT NOT NULL,
    action TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    metadata TEXT,
    FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE SET NULL,
    FOREIGN KEY (client_id) REFERENCES clients(id)
);

CREATE INDEX IF NOT EXISTS idx_files_path ON files(path);
CREATE INDEX IF NOT EXISTS idx_file_logs_client_id ON file_logs(client_id);
CREATE INDEX IF NOT EXISTS idx_file_logs_timestamp ON file_logs(timestamp DESC);
