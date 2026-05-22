//! Board, column, and task persistence — the `impl Db` block for the Kanban
//! data. Lives alongside the session store in the `db` module.

use std::path::Path;

use anyhow::{Context, Result};
use tokio_rusqlite::rusqlite::{self, params, OptionalExtension};

use super::Db;
use crate::types::{Board, Column, Task};

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
                            t.session_id, t.created_at \
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
                    "SELECT id, column_id, title, description, position, session_id, created_at \
                     FROM tasks WHERE id = ?1",
                    params![task_id],
                    row_to_task,
                )
                .optional()
            })
            .await
            .context("failed to get task")
    }

    /// Inserts a new task.
    pub async fn create_task(&self, task: Task) -> Result<()> {
        self.conn
            .call(move |conn| -> rusqlite::Result<()> {
                conn.execute(
                    "INSERT INTO tasks \
                     (id, column_id, title, description, position, session_id, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        task.id,
                        task.column_id,
                        task.title,
                        task.description,
                        task.position,
                        task.session_id,
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
                     position = ?5, session_id = ?6 WHERE id = ?1",
                    params![
                        task.id,
                        task.column_id,
                        task.title,
                        task.description,
                        task.position,
                        task.session_id,
                    ],
                )?;
                Ok(())
            })
            .await
            .context("failed to update task")?;
        Ok(())
    }

    /// Deletes a task.
    pub async fn delete_task(&self, task_id: String) -> Result<()> {
        self.conn
            .call(move |conn| -> rusqlite::Result<()> {
                conn.execute("DELETE FROM tasks WHERE id = ?1", params![task_id])?;
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
        created_at: row.get(6)?,
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
}
