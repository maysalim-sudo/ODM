use rusqlite::Connection;

const V1: &str = r#"
CREATE TABLE downloads (
    id              TEXT PRIMARY KEY,
    url             TEXT NOT NULL,
    final_url       TEXT,
    filename        TEXT NOT NULL,
    save_dir        TEXT NOT NULL,
    size            INTEGER,
    mime            TEXT,
    category        TEXT NOT NULL DEFAULT 'general',
    state           TEXT NOT NULL,
    error_kind      TEXT,
    error_msg       TEXT,
    priority        INTEGER NOT NULL DEFAULT 0,
    max_connections INTEGER NOT NULL DEFAULT 8,
    speed_limit     INTEGER,
    etag            TEXT,
    last_modified   TEXT,
    checksum        TEXT,
    kind            TEXT NOT NULL DEFAULT 'file',
    page_url        TEXT,
    page_title      TEXT,
    filename_locked INTEGER NOT NULL DEFAULT 0,
    created_at      INTEGER NOT NULL,
    started_at      INTEGER,
    finished_at     INTEGER
);
CREATE INDEX idx_downloads_state ON downloads(state);

CREATE TABLE download_headers (
    download_id TEXT NOT NULL REFERENCES downloads(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    value       TEXT NOT NULL,
    PRIMARY KEY (download_id, name)
);

CREATE TABLE chunks (
    download_id TEXT NOT NULL REFERENCES downloads(id) ON DELETE CASCADE,
    idx         INTEGER NOT NULL,
    start       INTEGER NOT NULL,
    end_incl    INTEGER,
    written     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (download_id, idx)
);

CREATE TABLE hls_segments (
    download_id TEXT NOT NULL REFERENCES downloads(id) ON DELETE CASCADE,
    idx         INTEGER NOT NULL,
    url         TEXT NOT NULL,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    state       TEXT NOT NULL DEFAULT 'queued',
    bytes       INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (download_id, idx)
);

CREATE TABLE hls_meta (
    download_id TEXT PRIMARY KEY REFERENCES downloads(id) ON DELETE CASCADE,
    variant_url TEXT NOT NULL,
    key_url     TEXT,
    iv          TEXT,
    map_url     TEXT
);
"#;

/// Ordered; index i migrates to user_version i+1. Append only — never edit a shipped entry.
const MIGRATIONS: &[&str] = &[V1];

pub fn migrate(conn: &mut Connection) -> rusqlite::Result<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}
