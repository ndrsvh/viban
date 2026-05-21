//! SQL schema. Each migration's statements live here as a constant; the
//! migration runner applies them in order.

/// Migration 1 — the initial sessions + messages schema.
pub const MIGRATION_1: &str = "
CREATE TABLE sessions (
    id                TEXT PRIMARY KEY,
    claude_session_id TEXT,
    title             TEXT NOT NULL,
    created_at        INTEGER NOT NULL,
    project_path      TEXT NOT NULL
);

CREATE TABLE messages (
    id         TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    role       TEXT NOT NULL,
    content    TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    raw_json   TEXT
);

CREATE INDEX idx_messages_session ON messages (session_id, created_at);
";

/// Every migration, in application order. A migration's version is its
/// 1-based index in this list.
pub const MIGRATIONS: &[&str] = &[MIGRATION_1];
