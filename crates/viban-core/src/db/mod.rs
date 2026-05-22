//! SQLite-backed persistence for sessions and messages.
//!
//! Uses `tokio-rusqlite` so database work never blocks the async runtime.

mod boards;
mod migrations;
mod schema;

use std::path::Path;

use anyhow::{Context, Result};
use tokio_rusqlite::rusqlite::{self, params, OptionalExtension};
use tokio_rusqlite::Connection;

use crate::now_millis;
use crate::types::{Message, Session, TokenUsage};

/// The viban session/message store. Cheap to clone — the inner connection
/// handle is shared, so background tasks can hold their own copy.
#[derive(Clone)]
pub struct Db {
    conn: Connection,
}

impl Db {
    /// Opens (creating if needed) the database at `path` and runs migrations.
    pub async fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .await
            .with_context(|| format!("failed to open database at {}", path.display()))?;
        Self::migrate(conn).await
    }

    /// Opens an in-memory database — used by tests.
    pub async fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()
            .await
            .context("failed to open in-memory database")?;
        Self::migrate(conn).await
    }

    async fn migrate(conn: Connection) -> Result<Self> {
        conn.call(|conn| -> rusqlite::Result<()> {
            migrations::run(conn)?;
            Ok(())
        })
        .await
        .context("failed to run migrations")?;
        Ok(Self { conn })
    }

    /// Inserts a new session row.
    pub async fn create_session(&self, session: Session) -> Result<()> {
        self.conn
            .call(move |conn| -> rusqlite::Result<()> {
                conn.execute(
                    "INSERT INTO sessions \
                     (id, claude_session_id, title, created_at, project_path) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        session.id,
                        session.claude_session_id,
                        session.title,
                        session.created_at,
                        session.project_path,
                    ],
                )?;
                Ok(())
            })
            .await
            .context("failed to insert session")?;
        Ok(())
    }

    /// Records the Claude Code session id once the agent reports it.
    pub async fn set_claude_session_id(
        &self,
        session_id: String,
        claude_session_id: String,
    ) -> Result<()> {
        self.conn
            .call(move |conn| -> rusqlite::Result<()> {
                conn.execute(
                    "UPDATE sessions SET claude_session_id = ?1 WHERE id = ?2",
                    params![claude_session_id, session_id],
                )?;
                Ok(())
            })
            .await
            .context("failed to update claude_session_id")?;
        Ok(())
    }

    /// Lists all sessions, newest first.
    pub async fn list_sessions(&self) -> Result<Vec<Session>> {
        let sessions = self
            .conn
            .call(|conn| -> rusqlite::Result<Vec<Session>> {
                let mut stmt = conn.prepare(
                    "SELECT id, claude_session_id, title, created_at, project_path \
                     FROM sessions ORDER BY created_at DESC",
                )?;
                let rows = stmt
                    .query_map([], row_to_session)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await
            .context("failed to list sessions")?;
        Ok(sessions)
    }

    /// Fetches a single session by id.
    pub async fn get_session(&self, session_id: String) -> Result<Option<Session>> {
        let session = self
            .conn
            .call(move |conn| -> rusqlite::Result<Option<Session>> {
                conn.query_row(
                    "SELECT id, claude_session_id, title, created_at, project_path \
                     FROM sessions WHERE id = ?1",
                    params![session_id],
                    row_to_session,
                )
                .optional()
            })
            .await
            .context("failed to get session")?;
        Ok(session)
    }

    /// Fetches a session's messages in chronological order.
    pub async fn get_messages(&self, session_id: String) -> Result<Vec<Message>> {
        let messages = self
            .conn
            .call(move |conn| -> rusqlite::Result<Vec<Message>> {
                let mut stmt = conn.prepare(
                    "SELECT id, session_id, role, content, created_at, raw_json \
                     FROM messages WHERE session_id = ?1 ORDER BY created_at, id",
                )?;
                let rows = stmt
                    .query_map(params![session_id], row_to_message)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await
            .context("failed to get messages")?;
        Ok(messages)
    }

    /// Appends a message to a session.
    pub async fn insert_message(&self, message: Message) -> Result<()> {
        self.conn
            .call(move |conn| -> rusqlite::Result<()> {
                conn.execute(
                    "INSERT INTO messages \
                     (id, session_id, role, content, created_at, raw_json) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        message.id,
                        message.session_id,
                        message.role,
                        message.content,
                        message.created_at,
                        message.raw_json,
                    ],
                )?;
                Ok(())
            })
            .await
            .context("failed to insert message")?;
        Ok(())
    }

    /// Records that a session edited `path`. Idempotent — re-touching a file
    /// keeps its first-seen timestamp.
    pub async fn record_session_file(&self, session_id: String, path: String) -> Result<()> {
        let created_at = now_millis();
        self.conn
            .call(move |conn| -> rusqlite::Result<()> {
                conn.execute(
                    "INSERT OR IGNORE INTO session_files (session_id, path, created_at) \
                     VALUES (?1, ?2, ?3)",
                    params![session_id, path, created_at],
                )?;
                Ok(())
            })
            .await
            .context("failed to record session file")?;
        Ok(())
    }

    /// The files a session has edited, in first-touched order.
    pub async fn list_session_files(&self, session_id: String) -> Result<Vec<String>> {
        let files = self
            .conn
            .call(move |conn| -> rusqlite::Result<Vec<String>> {
                let mut stmt = conn.prepare(
                    "SELECT path FROM session_files \
                     WHERE session_id = ?1 ORDER BY created_at, path",
                )?;
                let rows = stmt
                    .query_map(params![session_id], |row| row.get(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await
            .context("failed to list session files")?;
        Ok(files)
    }

    /// Adds one turn's token counts to a session's running total.
    pub async fn add_session_usage(
        &self,
        session_id: String,
        input_tokens: i64,
        output_tokens: i64,
    ) -> Result<()> {
        self.conn
            .call(move |conn| -> rusqlite::Result<()> {
                conn.execute(
                    "INSERT INTO session_usage (session_id, input_tokens, output_tokens) \
                     VALUES (?1, ?2, ?3) \
                     ON CONFLICT(session_id) DO UPDATE SET \
                       input_tokens = input_tokens + ?2, \
                       output_tokens = output_tokens + ?3",
                    params![session_id, input_tokens, output_tokens],
                )?;
                Ok(())
            })
            .await
            .context("failed to record session usage")?;
        Ok(())
    }

    /// A session's accumulated token usage — zero if it has reported none.
    pub async fn get_session_usage(&self, session_id: String) -> Result<TokenUsage> {
        let usage = self
            .conn
            .call(move |conn| -> rusqlite::Result<TokenUsage> {
                conn.query_row(
                    "SELECT input_tokens, output_tokens FROM session_usage \
                     WHERE session_id = ?1",
                    params![session_id],
                    |row| {
                        Ok(TokenUsage {
                            input_tokens: row.get(0)?,
                            output_tokens: row.get(1)?,
                        })
                    },
                )
                .optional()
                .map(Option::unwrap_or_default)
            })
            .await
            .context("failed to get session usage")?;
        Ok(usage)
    }
}

fn row_to_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<Session> {
    Ok(Session {
        id: row.get(0)?,
        claude_session_id: row.get(1)?,
        title: row.get(2)?,
        created_at: row.get(3)?,
        project_path: row.get(4)?,
    })
}

fn row_to_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<Message> {
    Ok(Message {
        id: row.get(0)?,
        session_id: row.get(1)?,
        role: row.get(2)?,
        content: row.get(3)?,
        created_at: row.get(4)?,
        raw_json: row.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str) -> Session {
        Session {
            id: id.to_string(),
            claude_session_id: None,
            title: "test".to_string(),
            created_at: 1,
            project_path: "/tmp".to_string(),
        }
    }

    #[tokio::test]
    async fn migrations_are_idempotent() {
        // open_in_memory runs migrations; a second pass must be a no-op.
        let db = Db::open_in_memory().await.expect("open");
        db.conn
            .call(|conn| -> rusqlite::Result<()> {
                migrations::run(conn)?;
                Ok(())
            })
            .await
            .expect("second migrate");
        assert!(db.list_sessions().await.expect("list").is_empty());
    }

    #[tokio::test]
    async fn session_and_messages_round_trip() {
        let db = Db::open_in_memory().await.expect("open");
        db.create_session(session("s1")).await.expect("create");
        db.set_claude_session_id("s1".into(), "claude-1".into())
            .await
            .expect("set claude id");
        db.insert_message(Message {
            id: "m1".into(),
            session_id: "s1".into(),
            role: "user".into(),
            content: "hello".into(),
            created_at: 2,
            raw_json: None,
        })
        .await
        .expect("insert message");

        let fetched = db
            .get_session("s1".into())
            .await
            .expect("get")
            .expect("some");
        assert_eq!(fetched.claude_session_id.as_deref(), Some("claude-1"));

        let messages = db.get_messages("s1".into()).await.expect("messages");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "hello");

        assert!(db
            .get_session("missing".into())
            .await
            .expect("get")
            .is_none());
    }

    #[tokio::test]
    async fn list_sessions_returns_newest_first() {
        let db = Db::open_in_memory().await.expect("open");
        let mut older = session("s-old");
        older.created_at = 100;
        let mut newer = session("s-new");
        newer.created_at = 200;
        db.create_session(older).await.expect("create older");
        db.create_session(newer).await.expect("create newer");

        let sessions = db.list_sessions().await.expect("list");
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].id, "s-new", "newest session comes first");
        assert_eq!(sessions[1].id, "s-old");
    }

    #[tokio::test]
    async fn messages_are_returned_in_chronological_order() {
        let db = Db::open_in_memory().await.expect("open");
        db.create_session(session("s1")).await.expect("create");
        for (id, created_at) in [("m-late", 20), ("m-early", 10)] {
            db.insert_message(Message {
                id: id.into(),
                session_id: "s1".into(),
                role: "user".into(),
                content: id.into(),
                created_at,
                raw_json: None,
            })
            .await
            .expect("insert");
        }
        let messages = db.get_messages("s1".into()).await.expect("messages");
        assert_eq!(messages[0].id, "m-early");
        assert_eq!(messages[1].id, "m-late");
    }

    #[tokio::test]
    async fn every_migration_is_recorded_once() {
        let db = Db::open_in_memory().await.expect("open");
        let versions = db
            .conn
            .call(|conn| -> rusqlite::Result<Vec<i64>> {
                let mut stmt = conn.prepare("SELECT version FROM migrations ORDER BY version")?;
                let rows = stmt
                    .query_map([], |row| row.get(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await
            .expect("query migration versions");
        assert_eq!(
            versions,
            vec![1, 2, 3, 4, 5, 6, 7],
            "all migrations are recorded"
        );
    }

    #[tokio::test]
    async fn session_usage_accumulates_across_turns() {
        let db = Db::open_in_memory().await.expect("open");
        db.create_session(session("s1")).await.expect("create");

        // No usage yet — zero.
        let zero = db.get_session_usage("s1".into()).await.expect("usage");
        assert_eq!(zero.input_tokens, 0);
        assert_eq!(zero.output_tokens, 0);

        db.add_session_usage("s1".into(), 100, 40)
            .await
            .expect("turn 1");
        db.add_session_usage("s1".into(), 250, 90)
            .await
            .expect("turn 2");

        let total = db.get_session_usage("s1".into()).await.expect("usage");
        assert_eq!(total.input_tokens, 350);
        assert_eq!(total.output_tokens, 130);
    }

    #[tokio::test]
    async fn session_files_are_recorded_deduped_and_scoped() {
        let db = Db::open_in_memory().await.expect("open");
        db.create_session(session("s1")).await.expect("create s1");
        db.create_session(session("s2")).await.expect("create s2");

        db.record_session_file("s1".into(), "src/a.rs".into())
            .await
            .expect("record a");
        db.record_session_file("s1".into(), "src/b.rs".into())
            .await
            .expect("record b");
        // Re-touching a file must not duplicate it.
        db.record_session_file("s1".into(), "src/a.rs".into())
            .await
            .expect("re-record a");
        db.record_session_file("s2".into(), "other.rs".into())
            .await
            .expect("record for s2");

        let files = db.list_session_files("s1".into()).await.expect("list s1");
        assert_eq!(files, vec!["src/a.rs", "src/b.rs"]);

        let other = db.list_session_files("s2".into()).await.expect("list s2");
        assert_eq!(other, vec!["other.rs"], "files are scoped per session");
    }
}
