//! Domain types shared across viban — persisted rows and wire payloads.
//!
//! Each type derives `ts_rs::TS`; `cargo test` regenerates the matching
//! TypeScript in `src/types/generated/`, so the frontend types never drift.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A persisted Claude Code session.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/types/generated/")]
pub struct Session {
    /// viban's own session id (the primary key and wire id).
    pub id: String,
    /// The Claude Code session id, learned from the init event; used to
    /// `--resume`. `None` until the agent has reported it.
    pub claude_session_id: Option<String>,
    /// Display title — the first user prompt, truncated.
    pub title: String,
    /// Creation time, Unix epoch milliseconds.
    // Over JSON-RPC this is a plain JSON number; ts-rs would otherwise emit
    // `bigint`, which a JSON number is not at runtime.
    #[ts(type = "number")]
    pub created_at: i64,
    /// Filesystem path of the workspace the session runs in.
    pub project_path: String,
}

/// One persisted message within a session.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/types/generated/")]
pub struct Message {
    pub id: String,
    pub session_id: String,
    /// `user`, `assistant`, `tool`, `system`, or `error`.
    pub role: String,
    /// Human-readable text content.
    pub content: String,
    /// Creation time, Unix epoch milliseconds.
    // Over JSON-RPC this is a plain JSON number; ts-rs would otherwise emit
    // `bigint`, which a JSON number is not at runtime.
    #[ts(type = "number")]
    pub created_at: i64,
    /// The raw agent event JSON, when the message came from one.
    pub raw_json: Option<String>,
}

/// A Kanban board — one per workspace in the current MVP.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/types/generated/")]
pub struct Board {
    pub id: String,
    pub name: String,
    pub project_path: String,
    // Over JSON-RPC this is a plain JSON number; ts-rs would otherwise emit
    // `bigint`, which a JSON number is not at runtime.
    #[ts(type = "number")]
    pub created_at: i64,
}

/// A column on a board, ordered by `position`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/types/generated/")]
pub struct Column {
    pub id: String,
    pub board_id: String,
    pub name: String,
    #[ts(type = "number")]
    pub position: i64,
}

/// A task card within a column, ordered by `position`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/types/generated/")]
pub struct Task {
    pub id: String,
    pub column_id: String,
    pub title: String,
    pub description: String,
    #[ts(type = "number")]
    pub position: i64,
    /// The viban session started from this task, if any.
    pub session_id: Option<String>,
    /// Filesystem path of the task's git worktree, once a session has started.
    pub worktree_path: Option<String>,
    /// The git branch the task's worktree is on.
    pub branch: Option<String>,
    // Over JSON-RPC this is a plain JSON number; ts-rs would otherwise emit
    // `bigint`, which a JSON number is not at runtime.
    #[ts(type = "number")]
    pub created_at: i64,
}

/// One agent run of a task, with its own session, worktree, and branch. A
/// task may have several attempts; the task's own `session_id` /
/// `worktree_path` / `branch` point at the currently active one.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/types/generated/")]
pub struct Attempt {
    pub id: String,
    pub task_id: String,
    pub session_id: Option<String>,
    pub worktree_path: Option<String>,
    pub branch: Option<String>,
    // Over JSON-RPC this is a plain JSON number; ts-rs would otherwise emit
    // `bigint`, which a JSON number is not at runtime.
    #[ts(type = "number")]
    pub created_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_round_trips_through_json() {
        let task = Task {
            id: "t1".into(),
            column_id: "c1".into(),
            title: "Title".into(),
            description: "Desc".into(),
            position: 3,
            session_id: Some("s1".into()),
            worktree_path: Some("/tmp/wt".into()),
            branch: Some("viban/x".into()),
            created_at: 42,
        };
        let json = serde_json::to_string(&task).expect("serialize");
        let back: Task = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.id, task.id);
        assert_eq!(back.position, 3);
        assert_eq!(back.session_id.as_deref(), Some("s1"));
        assert_eq!(back.worktree_path.as_deref(), Some("/tmp/wt"));
        assert_eq!(back.branch.as_deref(), Some("viban/x"));
    }

    #[test]
    fn task_optional_fields_serialize_as_null() {
        let task = Task {
            id: "t".into(),
            column_id: "c".into(),
            title: "x".into(),
            description: String::new(),
            position: 0,
            session_id: None,
            worktree_path: None,
            branch: None,
            created_at: 0,
        };
        let value = serde_json::to_value(&task).expect("to_value");
        assert!(value["session_id"].is_null());
        assert!(value["worktree_path"].is_null());
        assert!(value["branch"].is_null());
    }

    #[test]
    fn session_round_trips_through_json() {
        let session = Session {
            id: "s".into(),
            claude_session_id: Some("claude-1".into()),
            title: "t".into(),
            created_at: 1,
            project_path: "/p".into(),
        };
        let back: Session =
            serde_json::from_str(&serde_json::to_string(&session).expect("serialize"))
                .expect("deserialize");
        assert_eq!(back.id, "s");
        assert_eq!(back.claude_session_id.as_deref(), Some("claude-1"));
    }

    #[test]
    fn board_and_column_round_trip_through_json() {
        let board = Board {
            id: "b".into(),
            name: "n".into(),
            project_path: "/p".into(),
            created_at: 2,
        };
        let back: Board = serde_json::from_str(&serde_json::to_string(&board).expect("serialize"))
            .expect("deserialize");
        assert_eq!(back.name, "n");

        let column = Column {
            id: "c".into(),
            board_id: "b".into(),
            name: "Backlog".into(),
            position: 1,
        };
        let back: Column =
            serde_json::from_str(&serde_json::to_string(&column).expect("serialize"))
                .expect("deserialize");
        assert_eq!(back.position, 1);
        assert_eq!(back.board_id, "b");
    }

    #[test]
    fn message_round_trips_through_json() {
        let message = Message {
            id: "m".into(),
            session_id: "s".into(),
            role: "assistant".into(),
            content: "hello".into(),
            created_at: 3,
            raw_json: Some("{\"k\":1}".into()),
        };
        let back: Message =
            serde_json::from_str(&serde_json::to_string(&message).expect("serialize"))
                .expect("deserialize");
        assert_eq!(back.role, "assistant");
        assert_eq!(back.raw_json.as_deref(), Some("{\"k\":1}"));
    }
}
