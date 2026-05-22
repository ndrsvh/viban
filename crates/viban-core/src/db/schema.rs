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

/// Migration 3 — per-task git worktree: where each task's agent runs.
pub const MIGRATION_3: &str = "
ALTER TABLE tasks ADD COLUMN worktree_path TEXT;
ALTER TABLE tasks ADD COLUMN branch TEXT;
";

/// Migration 4 — attempts: a task may have several agent runs, each with its
/// own session, worktree, and branch. Existing worktree-backed tasks are
/// backfilled with one attempt copying their current fields.
pub const MIGRATION_4: &str = "
CREATE TABLE attempts (
    id            TEXT PRIMARY KEY,
    task_id       TEXT NOT NULL REFERENCES tasks(id),
    session_id    TEXT,
    worktree_path TEXT,
    branch        TEXT,
    created_at    INTEGER NOT NULL
);

CREATE INDEX idx_attempts_task ON attempts (task_id, created_at);

INSERT INTO attempts (id, task_id, session_id, worktree_path, branch, created_at)
SELECT lower(hex(randomblob(16))), id, session_id, worktree_path, branch, created_at
FROM tasks
WHERE session_id IS NOT NULL;
";

/// Migration 5 — session_files: the files an agent session has edited,
/// recorded from its tool calls so a session's footprint survives a restart.
pub const MIGRATION_5: &str = "
CREATE TABLE session_files (
    session_id TEXT NOT NULL REFERENCES sessions(id),
    path       TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (session_id, path)
);

CREATE INDEX idx_session_files_session ON session_files (session_id, created_at);
";

/// Every migration, in application order. A migration's version is its
/// 1-based index in this list.
pub const MIGRATIONS: &[&str] = &[
    MIGRATION_1,
    MIGRATION_2,
    MIGRATION_3,
    MIGRATION_4,
    MIGRATION_5,
];
