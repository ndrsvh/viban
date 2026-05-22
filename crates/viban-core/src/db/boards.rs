//! Board, column, and task persistence — the `impl Db` block for the Kanban
//! data. Lives alongside the session store in the `db` module.

use std::path::Path;

use anyhow::{Context, Result};
use tokio_rusqlite::rusqlite::{self, params, OptionalExtension};

use super::Db;
use crate::types::{Attempt, Board, Checkpoint, Column, Task};

/// The default columns created with a fresh board.
const DEFAULT_COLUMNS: [&str; 4] = ["Backlog", "In Progress", "Review", "Done"];

impl Db {
    /// Returns the workspace's board, creating it (with the default columns)
    /// on first call.
    pub async fn ensure_default_board(&self, project_path: &str) -> Result<Board> {
        let project_path = project_path.to_string();
        self.conn
            .call(move |conn| -> rusqlite::Result<Board> {
                if let Some(board) = conn
                    .query_row(
                        "SELECT id, name, project_path, created_at FROM boards LIMIT 1",
                        [],
                        row_to_board,
                    )
                    .optional()?
                {
                    return Ok(board);
                }

                let board = Board {
                    id: crate::new_id(),
                    name: default_board_name(&project_path),
                    project_path,
                    created_at: crate::now_millis(),
                };
                let tx = conn.transaction()?;
                tx.execute(
                    "INSERT INTO boards (id, name, project_path, created_at) \
                     VALUES (?1, ?2, ?3, ?4)",
                    params![board.id, board.name, board.project_path, board.created_at],
                )?;
                for (position, name) in DEFAULT_COLUMNS.iter().enumerate() {
                    tx.execute(
                        "INSERT INTO columns (id, board_id, name, position) \
                         VALUES (?1, ?2, ?3, ?4)",
                        params![crate::new_id(), board.id, name, position as i64],
                    )?;
                }
                tx.commit()?;
                Ok(board)
            })
            .await
            .context("failed to ensure the default board")
    }

    /// Fetches the workspace's board, if one exists.
    pub async fn get_board(&self) -> Result<Option<Board>> {
        self.conn
            .call(|conn| -> rusqlite::Result<Option<Board>> {
                conn.query_row(
                    "SELECT id, name, project_path, created_at FROM boards LIMIT 1",
                    [],
                    row_to_board,
                )
                .optional()
            })
            .await
            .context("failed to get the board")
    }

    /// Lists a board's columns in display order.
    pub async fn list_columns(&self, board_id: String) -> Result<Vec<Column>> {
        self.conn
            .call(move |conn| -> rusqlite::Result<Vec<Column>> {
                let mut stmt = conn.prepare(
                    "SELECT id, board_id, name, position FROM columns \
                     WHERE board_id = ?1 ORDER BY position",
                )?;
                let rows = stmt
                    .query_map(params![board_id], row_to_column)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await
            .context("failed to list columns")
    }

    /// Lists every task on a board, ordered by column then position.
    pub async fn list_tasks(&self, board_id: String) -> Result<Vec<Task>> {
        self.conn
            .call(move |conn| -> rusqlite::Result<Vec<Task>> {
                let mut stmt = conn.prepare(
                    "SELECT t.id, t.column_id, t.title, t.description, t.position, \
                            t.session_id, t.worktree_path, t.branch, t.created_at \
                     FROM tasks t JOIN columns c ON t.column_id = c.id \
                     WHERE c.board_id = ?1 ORDER BY t.column_id, t.position",
                )?;
                let rows = stmt
                    .query_map(params![board_id], row_to_task)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await
            .context("failed to list tasks")
    }

    /// Fetches a single task by id.
    pub async fn get_task(&self, task_id: String) -> Result<Option<Task>> {
        self.conn
            .call(move |conn| -> rusqlite::Result<Option<Task>> {
                conn.query_row(
                    "SELECT id, column_id, title, description, position, session_id, \
                            worktree_path, branch, created_at \
                     FROM tasks WHERE id = ?1",
                    params![task_id],
                    row_to_task,
                )
                .optional()
            })
            .await
            .context("failed to get task")
    }

    /// Fetches the task linked to a given session, if any.
    pub async fn get_task_by_session(&self, session_id: String) -> Result<Option<Task>> {
        self.conn
            .call(move |conn| -> rusqlite::Result<Option<Task>> {
                conn.query_row(
                    "SELECT id, column_id, title, description, position, session_id, \
                            worktree_path, branch, created_at \
                     FROM tasks WHERE session_id = ?1",
                    params![session_id],
                    row_to_task,
                )
                .optional()
            })
            .await
            .context("failed to get task by session")
    }

    /// Inserts a new task.
    pub async fn create_task(&self, task: Task) -> Result<()> {
        self.conn
            .call(move |conn| -> rusqlite::Result<()> {
                conn.execute(
                    "INSERT INTO tasks \
                     (id, column_id, title, description, position, session_id, \
                      worktree_path, branch, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        task.id,
                        task.column_id,
                        task.title,
                        task.description,
                        task.position,
                        task.session_id,
                        task.worktree_path,
                        task.branch,
                        task.created_at,
                    ],
                )?;
                Ok(())
            })
            .await
            .context("failed to create task")?;
        Ok(())
    }

    /// Overwrites a task's mutable fields (everything but id and created_at).
    pub async fn update_task(&self, task: Task) -> Result<()> {
        self.conn
            .call(move |conn| -> rusqlite::Result<()> {
                conn.execute(
                    "UPDATE tasks SET column_id = ?2, title = ?3, description = ?4, \
                     position = ?5, session_id = ?6, worktree_path = ?7, branch = ?8 \
                     WHERE id = ?1",
                    params![
                        task.id,
                        task.column_id,
                        task.title,
                        task.description,
                        task.position,
                        task.session_id,
                        task.worktree_path,
                        task.branch,
                    ],
                )?;
                Ok(())
            })
            .await
            .context("failed to update task")?;
        Ok(())
    }

    /// Deletes a task and all of its attempts.
    pub async fn delete_task(&self, task_id: String) -> Result<()> {
        self.conn
            .call(move |conn| -> rusqlite::Result<()> {
                let tx = conn.transaction()?;
                tx.execute("DELETE FROM attempts WHERE task_id = ?1", params![task_id])?;
                tx.execute("DELETE FROM tasks WHERE id = ?1", params![task_id])?;
                tx.commit()?;
                Ok(())
            })
            .await
            .context("failed to delete task")?;
        Ok(())
    }

    /// Applies a column's full task ordering: each task is moved into
    /// `column_id` at its index in `task_ids`.
    pub async fn reorder_column(&self, column_id: String, task_ids: Vec<String>) -> Result<()> {
        self.conn
            .call(move |conn| -> rusqlite::Result<()> {
                let tx = conn.transaction()?;
                for (position, task_id) in task_ids.iter().enumerate() {
                    tx.execute(
                        "UPDATE tasks SET column_id = ?1, position = ?2 WHERE id = ?3",
                        params![column_id, position as i64, task_id],
                    )?;
                }
                tx.commit()?;
                Ok(())
            })
            .await
            .context("failed to reorder column")?;
        Ok(())
    }

    /// Inserts a new attempt for a task.
    pub async fn create_attempt(&self, attempt: Attempt) -> Result<()> {
        self.conn
            .call(move |conn| -> rusqlite::Result<()> {
                conn.execute(
                    "INSERT INTO attempts \
                     (id, task_id, session_id, worktree_path, branch, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        attempt.id,
                        attempt.task_id,
                        attempt.session_id,
                        attempt.worktree_path,
                        attempt.branch,
                        attempt.created_at,
                    ],
                )?;
                Ok(())
            })
            .await
            .context("failed to create attempt")?;
        Ok(())
    }

    /// Lists a task's attempts, newest first.
    pub async fn list_attempts(&self, task_id: String) -> Result<Vec<Attempt>> {
        self.conn
            .call(move |conn| -> rusqlite::Result<Vec<Attempt>> {
                let mut stmt = conn.prepare(
                    "SELECT id, task_id, session_id, worktree_path, branch, created_at \
                     FROM attempts WHERE task_id = ?1 ORDER BY created_at DESC, id",
                )?;
                let rows = stmt
                    .query_map(params![task_id], row_to_attempt)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await
            .context("failed to list attempts")
    }

    /// Fetches a single attempt by id.
    pub async fn get_attempt(&self, attempt_id: String) -> Result<Option<Attempt>> {
        self.conn
            .call(move |conn| -> rusqlite::Result<Option<Attempt>> {
                conn.query_row(
                    "SELECT id, task_id, session_id, worktree_path, branch, created_at \
                     FROM attempts WHERE id = ?1",
                    params![attempt_id],
                    row_to_attempt,
                )
                .optional()
            })
            .await
            .context("failed to get attempt")
    }

    /// Fetches the attempt a given session belongs to, if any.
    pub async fn get_attempt_by_session(&self, session_id: String) -> Result<Option<Attempt>> {
        self.conn
            .call(move |conn| -> rusqlite::Result<Option<Attempt>> {
                conn.query_row(
                    "SELECT id, task_id, session_id, worktree_path, branch, created_at \
                     FROM attempts WHERE session_id = ?1",
                    params![session_id],
                    row_to_attempt,
                )
                .optional()
            })
            .await
            .context("failed to get attempt by session")
    }

    /// Records a saved worktree checkpoint for a task.
    pub async fn create_checkpoint(&self, checkpoint: Checkpoint) -> Result<()> {
        self.conn
            .call(move |conn| -> rusqlite::Result<()> {
                conn.execute(
                    "INSERT INTO checkpoints \
                     (id, task_id, commit_sha, label, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        checkpoint.id,
                        checkpoint.task_id,
                        checkpoint.commit_sha,
                        checkpoint.label,
                        checkpoint.created_at,
                    ],
                )?;
                Ok(())
            })
            .await
            .context("failed to create checkpoint")?;
        Ok(())
    }

    /// Lists a task's checkpoints, oldest first.
    pub async fn list_checkpoints(&self, task_id: String) -> Result<Vec<Checkpoint>> {
        self.conn
            .call(move |conn| -> rusqlite::Result<Vec<Checkpoint>> {
                let mut stmt = conn.prepare(
                    "SELECT id, task_id, commit_sha, label, created_at \
                     FROM checkpoints WHERE task_id = ?1 ORDER BY created_at, id",
                )?;
                let rows = stmt
                    .query_map(params![task_id], row_to_checkpoint)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await
            .context("failed to list checkpoints")
    }

    /// Fetches a single checkpoint by id.
    pub async fn get_checkpoint(&self, checkpoint_id: String) -> Result<Option<Checkpoint>> {
        self.conn
            .call(move |conn| -> rusqlite::Result<Option<Checkpoint>> {
                conn.query_row(
                    "SELECT id, task_id, commit_sha, label, created_at \
                     FROM checkpoints WHERE id = ?1",
                    params![checkpoint_id],
                    row_to_checkpoint,
                )
                .optional()
            })
            .await
            .context("failed to get checkpoint")
    }
}

fn default_board_name(project_path: &str) -> String {
    Path::new(project_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Board")
        .to_string()
}

fn row_to_board(row: &rusqlite::Row<'_>) -> rusqlite::Result<Board> {
    Ok(Board {
        id: row.get(0)?,
        name: row.get(1)?,
        project_path: row.get(2)?,
        created_at: row.get(3)?,
    })
}

fn row_to_column(row: &rusqlite::Row<'_>) -> rusqlite::Result<Column> {
    Ok(Column {
        id: row.get(0)?,
        board_id: row.get(1)?,
        name: row.get(2)?,
        position: row.get(3)?,
    })
}

fn row_to_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get(0)?,
        column_id: row.get(1)?,
        title: row.get(2)?,
        description: row.get(3)?,
        position: row.get(4)?,
        session_id: row.get(5)?,
        worktree_path: row.get(6)?,
        branch: row.get(7)?,
        created_at: row.get(8)?,
    })
}

fn row_to_attempt(row: &rusqlite::Row<'_>) -> rusqlite::Result<Attempt> {
    Ok(Attempt {
        id: row.get(0)?,
        task_id: row.get(1)?,
        session_id: row.get(2)?,
        worktree_path: row.get(3)?,
        branch: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn row_to_checkpoint(row: &rusqlite::Row<'_>) -> rusqlite::Result<Checkpoint> {
    Ok(Checkpoint {
        id: row.get(0)?,
        task_id: row.get(1)?,
        commit_sha: row.get(2)?,
        label: row.get(3)?,
        created_at: row.get(4)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn default_board_is_created_once() {
        let db = Db::open_in_memory().await.expect("open");
        let first = db.ensure_default_board("/tmp/proj").await.expect("ensure");
        let second = db.ensure_default_board("/tmp/proj").await.expect("ensure");
        assert_eq!(first.id, second.id);
        assert_eq!(first.name, "proj");

        let columns = db.list_columns(first.id).await.expect("columns");
        assert_eq!(columns.len(), 4);
        assert_eq!(columns[0].name, "Backlog");
        assert_eq!(columns[3].name, "Done");
    }

    #[tokio::test]
    async fn checkpoints_round_trip_and_order_by_creation() {
        let db = Db::open_in_memory().await.expect("open");
        let board = db.ensure_default_board("/tmp/proj").await.expect("board");
        let columns = db.list_columns(board.id).await.expect("columns");
        db.create_task(Task {
            id: "t1".into(),
            column_id: columns[0].id.clone(),
            title: "Task".into(),
            description: String::new(),
            position: 0,
            session_id: None,
            worktree_path: None,
            branch: None,
            created_at: 0,
        })
        .await
        .expect("create task");

        for (id, sha, label, created) in [
            ("c1", "sha-one", "before refactor", 10),
            ("c2", "sha-two", "after refactor", 20),
        ] {
            db.create_checkpoint(Checkpoint {
                id: id.into(),
                task_id: "t1".into(),
                commit_sha: sha.into(),
                label: label.into(),
                created_at: created,
            })
            .await
            .expect("create checkpoint");
        }

        let checkpoints = db.list_checkpoints("t1".into()).await.expect("list");
        assert_eq!(checkpoints.len(), 2);
        assert_eq!(checkpoints[0].id, "c1", "oldest checkpoint first");
        assert_eq!(checkpoints[1].label, "after refactor");

        let fetched = db
            .get_checkpoint("c2".into())
            .await
            .expect("get_checkpoint")
            .expect("checkpoint exists");
        assert_eq!(fetched.commit_sha, "sha-two");

        assert!(db
            .list_checkpoints("other".into())
            .await
            .expect("list")
            .is_empty());
    }

    #[tokio::test]
    async fn tasks_create_list_and_reorder() {
        let db = Db::open_in_memory().await.expect("open");
        let board = db.ensure_default_board("/tmp/proj").await.expect("ensure");
        let columns = db.list_columns(board.id.clone()).await.expect("columns");
        let column = &columns[0];

        for (index, title) in ["first", "second"].iter().enumerate() {
            db.create_task(Task {
                id: format!("t{index}"),
                column_id: column.id.clone(),
                title: (*title).to_string(),
                description: String::new(),
                position: index as i64,
                session_id: None,
                worktree_path: None,
                branch: None,
                created_at: 0,
            })
            .await
            .expect("create");
        }

        let tasks = db.list_tasks(board.id.clone()).await.expect("list");
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].id, "t0");

        db.reorder_column(column.id.clone(), vec!["t1".into(), "t0".into()])
            .await
            .expect("reorder");
        let reordered = db.list_tasks(board.id).await.expect("list");
        assert_eq!(reordered[0].id, "t1");
        assert_eq!(reordered[1].id, "t0");
    }

    #[tokio::test]
    async fn get_board_is_none_before_any_board_exists() {
        let db = Db::open_in_memory().await.expect("open");
        assert!(db.get_board().await.expect("get_board").is_none());
    }

    #[tokio::test]
    async fn task_update_covers_worktree_fields_and_delete_removes_it() {
        let db = Db::open_in_memory().await.expect("open");
        let board = db.ensure_default_board("/tmp/proj").await.expect("board");
        let columns = db.list_columns(board.id.clone()).await.expect("columns");

        let task = Task {
            id: "t1".into(),
            column_id: columns[0].id.clone(),
            title: "Original".into(),
            description: "desc".into(),
            position: 0,
            session_id: None,
            worktree_path: None,
            branch: None,
            created_at: 10,
        };
        db.create_task(task.clone()).await.expect("create");

        let updated = Task {
            title: "Renamed".into(),
            session_id: Some("sess-1".into()),
            worktree_path: Some("/tmp/wt".into()),
            branch: Some("viban/renamed".into()),
            ..task
        };
        db.update_task(updated).await.expect("update");

        let fetched = db
            .get_task("t1".into())
            .await
            .expect("get_task")
            .expect("task exists");
        assert_eq!(fetched.title, "Renamed");
        assert_eq!(fetched.session_id.as_deref(), Some("sess-1"));
        assert_eq!(fetched.worktree_path.as_deref(), Some("/tmp/wt"));
        assert_eq!(fetched.branch.as_deref(), Some("viban/renamed"));

        db.delete_task("t1".into()).await.expect("delete");
        assert!(db.get_task("t1".into()).await.expect("get_task").is_none());
    }

    #[tokio::test]
    async fn get_task_by_session_finds_the_linked_task() {
        let db = Db::open_in_memory().await.expect("open");
        let board = db.ensure_default_board("/tmp/proj").await.expect("board");
        let columns = db.list_columns(board.id.clone()).await.expect("columns");

        db.create_task(Task {
            id: "t1".into(),
            column_id: columns[0].id.clone(),
            title: "Linked".into(),
            description: String::new(),
            position: 0,
            session_id: Some("sess-42".into()),
            worktree_path: None,
            branch: None,
            created_at: 0,
        })
        .await
        .expect("create");

        let found = db
            .get_task_by_session("sess-42".into())
            .await
            .expect("query")
            .expect("a linked task");
        assert_eq!(found.id, "t1");
        assert!(db
            .get_task_by_session("missing".into())
            .await
            .expect("query")
            .is_none());
    }

    #[tokio::test]
    async fn reorder_moves_a_task_into_another_column() {
        let db = Db::open_in_memory().await.expect("open");
        let board = db.ensure_default_board("/tmp/proj").await.expect("board");
        let columns = db.list_columns(board.id.clone()).await.expect("columns");
        let (backlog, in_progress) = (&columns[0], &columns[1]);

        for (index, title) in ["a", "b"].iter().enumerate() {
            db.create_task(Task {
                id: format!("t{index}"),
                column_id: backlog.id.clone(),
                title: (*title).to_string(),
                description: String::new(),
                position: index as i64,
                session_id: None,
                worktree_path: None,
                branch: None,
                created_at: 0,
            })
            .await
            .expect("create");
        }

        // Move t0 into the In Progress column; leave t1 in Backlog.
        db.reorder_column(in_progress.id.clone(), vec!["t0".into()])
            .await
            .expect("reorder in_progress");
        db.reorder_column(backlog.id.clone(), vec!["t1".into()])
            .await
            .expect("reorder backlog");

        let tasks = db.list_tasks(board.id).await.expect("list");
        let t0 = tasks.iter().find(|task| task.id == "t0").expect("t0");
        let t1 = tasks.iter().find(|task| task.id == "t1").expect("t1");
        assert_eq!(t0.column_id, in_progress.id);
        assert_eq!(t0.position, 0);
        assert_eq!(t1.column_id, backlog.id);
    }

    #[tokio::test]
    async fn get_task_is_none_for_an_unknown_id() {
        let db = Db::open_in_memory().await.expect("open");
        assert!(db
            .get_task("nope".into())
            .await
            .expect("get_task")
            .is_none());
    }

    fn attempt(id: &str, task_id: &str, created_at: i64) -> Attempt {
        Attempt {
            id: id.to_string(),
            task_id: task_id.to_string(),
            session_id: Some(format!("session-{id}")),
            worktree_path: Some(format!("/wt/{id}")),
            branch: Some(format!("viban/x-{id}")),
            created_at,
        }
    }

    #[tokio::test]
    async fn attempts_create_list_and_get() {
        let db = Db::open_in_memory().await.expect("open");
        let board = db.ensure_default_board("/tmp/proj").await.expect("board");
        let columns = db.list_columns(board.id.clone()).await.expect("columns");
        db.create_task(Task {
            id: "t1".into(),
            column_id: columns[0].id.clone(),
            title: "Task".into(),
            description: String::new(),
            position: 0,
            session_id: None,
            worktree_path: None,
            branch: None,
            created_at: 0,
        })
        .await
        .expect("create task");

        db.create_attempt(attempt("a1", "t1", 10))
            .await
            .expect("create a1");
        db.create_attempt(attempt("a2", "t1", 20))
            .await
            .expect("create a2");

        let attempts = db.list_attempts("t1".into()).await.expect("list");
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[0].id, "a2", "newest attempt first");
        assert_eq!(attempts[1].id, "a1");

        let fetched = db
            .get_attempt("a1".into())
            .await
            .expect("get_attempt")
            .expect("attempt exists");
        assert_eq!(fetched.task_id, "t1");
        assert_eq!(fetched.session_id.as_deref(), Some("session-a1"));
        assert_eq!(fetched.branch.as_deref(), Some("viban/x-a1"));

        assert!(db
            .get_attempt("missing".into())
            .await
            .expect("get_attempt")
            .is_none());
        assert!(db
            .list_attempts("other".into())
            .await
            .expect("list")
            .is_empty());
    }
}
