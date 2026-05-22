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

/// Migration 2 — the Kanban board: boards, columns, and tasks.
pub const MIGRATION_2: &str = "
CREATE TABLE boards (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    project_path TEXT NOT NULL,
    created_at   INTEGER NOT NULL
);

CREATE TABLE columns (
    id       TEXT PRIMARY KEY,
    board_id TEXT NOT NULL REFERENCES boards(id),
    name     TEXT NOT NULL,
    position INTEGER NOT NULL
);

CREATE TABLE tasks (
    id          TEXT PRIMARY KEY,
    column_id   TEXT NOT NULL REFERENCES columns(id),
    title       TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    position    INTEGER NOT NULL,
    session_id  TEXT,
    created_at  INTEGER NOT NULL
);

CREATE INDEX idx_columns_board ON columns (board_id, position);
CREATE INDEX idx_tasks_column ON tasks (column_id, position);
";

/// Every migration, in application order. A migration's version is its
/// 1-based index in this list.
pub const MIGRATIONS: &[&str] = &[MIGRATION_1, MIGRATION_2];
