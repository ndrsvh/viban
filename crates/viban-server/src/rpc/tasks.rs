//! `boards.get` and the `tasks.*` CRUD methods.

use std::path::Path;

use serde_json::{json, Value};

use viban_core::types::Task;
use viban_core::{git, new_id};

use super::{now_millis, str_param, Context, RpcError};

/// Returns the workspace's board with its columns, tasks, and the live agent
/// status of each task. Serves `boards.get`.
pub(super) async fn get_board(ctx: &Context) -> Result<Value, RpcError> {
    let board = ctx
        .db
        .get_board()
        .await?
        .ok_or_else(|| RpcError::internal("no board exists"))?;
    let columns = ctx.db.list_columns(board.id.clone()).await?;
    let tasks = ctx.db.list_tasks(board.id.clone()).await?;
    let statuses = ctx.statuses.lock().await.clone();
    Ok(json!({
        "board": board,
        "columns": columns,
        "tasks": tasks,
        "statuses": statuses,
    }))
}

/// Creates a task at the end of a column.
pub(super) async fn create(params: Value, ctx: &Context) -> Result<Value, RpcError> {
    let column_id = str_param(&params, "column_id")?.to_string();
    let title = str_param(&params, "title")?.to_string();
    let description = params
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let board = ctx
        .db
        .get_board()
        .await?
        .ok_or_else(|| RpcError::internal("no board exists"))?;
    let position = ctx
        .db
        .list_tasks(board.id)
        .await?
        .iter()
        .filter(|task| task.column_id == column_id)
        .count() as i64;

    let task = Task {
        id: new_id(),
        column_id,
        title,
        description,
        position,
        session_id: None,
        worktree_path: None,
        branch: None,
        created_at: now_millis(),
    };
    ctx.db.create_task(task.clone()).await?;
    Ok(json!({ "task": task }))
}

/// Updates a task's title, description, and/or linked session.
pub(super) async fn update(params: Value, ctx: &Context) -> Result<Value, RpcError> {
    let task_id = str_param(&params, "task_id")?.to_string();
    let mut task = ctx
        .db
        .get_task(task_id.clone())
        .await?
        .ok_or_else(|| RpcError::invalid_params(format!("unknown task: {task_id}")))?;

    if let Some(title) = params.get("title").and_then(Value::as_str) {
        task.title = title.to_string();
    }
    if let Some(description) = params.get("description").and_then(Value::as_str) {
        task.description = description.to_string();
    }
    if let Some(session_id) = params.get("session_id").and_then(Value::as_str) {
        task.session_id = Some(session_id.to_string());
    }

    ctx.db.update_task(task.clone()).await?;
    Ok(json!({ "task": task }))
}

/// Deletes a task, tearing down every attempt's worktree, branch, and any
/// live agent.
pub(super) async fn delete(params: Value, ctx: &Context) -> Result<Value, RpcError> {
    let task_id = str_param(&params, "task_id")?.to_string();

    let attempts = ctx.db.list_attempts(task_id.clone()).await?;
    for attempt in &attempts {
        if let Some(session_id) = &attempt.session_id {
            ctx.registry.lock().await.remove(session_id);
        }
        if let Some(worktree_path) = &attempt.worktree_path {
            if let Err(err) =
                git::worktree_remove(&ctx.workspace, Path::new(worktree_path), true).await
            {
                tracing::warn!(%err, "failed to remove worktree");
            }
        }
        if let Some(branch) = &attempt.branch {
            if let Err(err) = git::branch_delete(&ctx.workspace, branch).await {
                tracing::warn!(%err, "failed to delete branch");
            }
        }
    }

    ctx.db.delete_task(task_id).await?;
    Ok(json!({ "ok": true }))
}

/// Applies a column's full task ordering (also re-parents moved tasks).
pub(super) async fn reorder(params: Value, ctx: &Context) -> Result<Value, RpcError> {
    let column_id = str_param(&params, "column_id")?.to_string();
    let task_ids = params
        .get("task_ids")
        .and_then(Value::as_array)
        .ok_or_else(|| RpcError::invalid_params("missing or non-array 'task_ids'"))?
        .iter()
        .filter_map(Value::as_str)
        .map(String::from)
        .collect::<Vec<_>>();
    ctx.db.reorder_column(column_id, task_ids).await?;
    Ok(json!({ "ok": true }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::rpc::test_support::{context, task};

    #[tokio::test]
    async fn get_board_returns_the_default_columns() {
        let (ctx, _ws, _data) = context().await;
        let board = super::get_board(&ctx).await.expect("get_board");
        assert!(board["board"]["id"].is_string());
        assert!(
            !board["columns"].as_array().expect("columns").is_empty(),
            "the default board has columns"
        );
    }

    #[tokio::test]
    async fn get_board_reports_live_task_statuses() {
        use viban_core::types::AgentStatus;
        let (ctx, _ws, _data) = context().await;
        let task_id = task(&ctx, "Running task").await;
        ctx.statuses
            .lock()
            .await
            .insert(task_id.clone(), AgentStatus::Running);

        let board = super::get_board(&ctx).await.expect("get_board");
        assert_eq!(board["statuses"][&task_id], "running");
    }

    #[tokio::test]
    async fn create_then_get_board_shows_the_task() {
        let (ctx, _ws, _data) = context().await;
        let task_id = task(&ctx, "Write docs").await;
        let board = super::get_board(&ctx).await.expect("get_board");
        assert!(board["tasks"]
            .as_array()
            .expect("tasks")
            .iter()
            .any(|t| t["id"].as_str() == Some(&task_id)));
    }

    #[tokio::test]
    async fn update_changes_the_title() {
        let (ctx, _ws, _data) = context().await;
        let task_id = task(&ctx, "Old title").await;
        let updated = super::update(json!({ "task_id": task_id, "title": "New title" }), &ctx)
            .await
            .expect("update");
        assert_eq!(updated["task"]["title"], "New title");
    }

    #[tokio::test]
    async fn update_rejects_an_unknown_task() {
        let (ctx, _ws, _data) = context().await;
        let err = super::update(json!({ "task_id": "nope" }), &ctx)
            .await
            .expect_err("unknown task errors");
        assert_eq!(err.code, -32602);
    }

    #[tokio::test]
    async fn delete_removes_a_task() {
        let (ctx, _ws, _data) = context().await;
        let task_id = task(&ctx, "Temporary").await;
        super::delete(json!({ "task_id": task_id.clone() }), &ctx)
            .await
            .expect("delete");
        let board = super::get_board(&ctx).await.expect("get_board");
        assert!(!board["tasks"]
            .as_array()
            .expect("tasks")
            .iter()
            .any(|t| t["id"].as_str() == Some(&task_id)));
    }

    #[tokio::test]
    async fn reorder_changes_the_relative_order() {
        let (ctx, _ws, _data) = context().await;
        let first = task(&ctx, "First").await;
        let second = task(&ctx, "Second").await;
        let board = super::get_board(&ctx).await.expect("board");
        let column_id = board["columns"][0]["id"]
            .as_str()
            .expect("column")
            .to_string();

        super::reorder(
            json!({ "column_id": column_id, "task_ids": [&second, &first] }),
            &ctx,
        )
        .await
        .expect("reorder");

        let board = super::get_board(&ctx).await.expect("board after reorder");
        let position_of = |id: &str| -> i64 {
            board["tasks"]
                .as_array()
                .expect("tasks")
                .iter()
                .find(|t| t["id"].as_str() == Some(id))
                .and_then(|t| t["position"].as_i64())
                .expect("a position")
        };
        assert!(
            position_of(&second) < position_of(&first),
            "the reordered task moved ahead"
        );
    }
}
