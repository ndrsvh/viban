//! Domain types shared across viban — persisted rows and wire payloads.

use serde::{Deserialize, Serialize};

/// A persisted Claude Code session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// viban's own session id (the primary key and wire id).
    pub id: String,
    /// The Claude Code session id, learned from the init event; used to
    /// `--resume`. `None` until the agent has reported it.
    pub claude_session_id: Option<String>,
    /// Display title — the first user prompt, truncated.
    pub title: String,
    /// Creation time, Unix epoch milliseconds.
    pub created_at: i64,
    /// Filesystem path of the workspace the session runs in.
    pub project_path: String,
}

/// One persisted message within a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    /// `user`, `assistant`, `tool`, `system`, or `error`.
    pub role: String,
    /// Human-readable text content.
    pub content: String,
    /// Creation time, Unix epoch milliseconds.
    pub created_at: i64,
    /// The raw agent event JSON, when the message came from one.
    pub raw_json: Option<String>,
}

/// A Kanban board — one per workspace in the current MVP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Board {
    pub id: String,
    pub name: String,
    pub project_path: String,
    pub created_at: i64,
}

/// A column on a board, ordered by `position`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    pub id: String,
    pub board_id: String,
    pub name: String,
    pub position: i64,
}

/// A task card within a column, ordered by `position`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub column_id: String,
    pub title: String,
    pub description: String,
    pub position: i64,
    /// The viban session started from this task, if any.
    pub session_id: Option<String>,
    pub created_at: i64,
}
