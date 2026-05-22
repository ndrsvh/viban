//! The `git.*` methods — reviewing and finalizing a task's worktree:
//! `git.diff` (review), `git.commit` (accept), `git.restore` (reject), and
//! `git.merge` (finalize into the project).

use std::path::PathBuf;

use serde_json::{json, Value};

use viban_core::agents::generate_commit_message;
use viban_core::git;
use viban_core::types::Task;

use super::{str_param, Context, RpcError};

/// Returns a task's pending worktree changes for review.
pub(super) async fn diff(params: Value, ctx: &Context) -> Result<Value, RpcError> {
    let task_id = str_param(&params, "task_id")?;
    let (_task, worktree) = task_worktree(ctx, task_id).await?;
    let files = git::worktree_diff(&worktree).await?;
    Ok(json!({ "files": files }))
}

/// Commits a task's worktree changes and moves the task to the Review column.
/// The commit message is generated from the diff by the agent, falling back to
/// the task title.
pub(super) async fn commit(params: Value, ctx: &Context) -> Result<Value, RpcError> {
    let task_id = str_param(&params, "task_id")?;
    let (mut task, worktree) = task_worktree(ctx, task_id).await?;
    let files = git::worktree_diff(&worktree).await.unwrap_or_default();
    let message = generate_commit_message(&worktree, &files, &task.title).await;
    git::commit_all(&worktree, &message).await?;
    move_task_to_column(ctx, &mut task, "Review").await?;
    Ok(json!({ "ok": true }))
}

/// Discards a task's worktree changes and moves the task back to In Progress.
pub(super) async fn restore(params: Value, ctx: &Context) -> Result<Value, RpcError> {
    let task_id = str_param(&params, "task_id")?;
    let (mut task, worktree) = task_worktree(ctx, task_id).await?;
    git::discard_all(&worktree).await?;
    move_task_to_column(ctx, &mut task, "In Progress").await?;
    Ok(json!({ "ok": true }))
}

/// Merges a task's branch into the project, tears down its worktree and
/// branch, and moves the task to the Done column.
pub(super) async fn merge(params: Value, ctx: &Context) -> Result<Value, RpcError> {
    let task_id = str_param(&params, "task_id")?;
    let (mut task, worktree) = task_worktree(ctx, task_id).await?;
    let branch = task
        .branch
        .clone()
        .ok_or_else(|| RpcError::invalid_params(format!("task {task_id} has no branch")))?;

    git::merge_branch(&ctx.workspace, &branch).await?;

    // The merge landed — tear down the task's worktree and branch.
    if let Err(err) = git::worktree_remove(&ctx.workspace, &worktree, true).await {
        tracing::warn!(%err, "failed to remove worktree after merge");
    }
    if let Err(err) = git::branch_delete(&ctx.workspace, &branch).await {
        tracing::warn!(%err, "failed to delete branch after merge");
    }

    task.worktree_path = None;
    task.branch = None;
    move_task_to_column(ctx, &mut task, "Done").await?;
    Ok(json!({ "ok": true }))
}

/// Loads a task and the path of its git worktree, erroring if it has none.
pub(super) async fn task_worktree(
    ctx: &Context,
    task_id: &str,
) -> Result<(Task, PathBuf), RpcError> {
    let task = ctx
        .db
        .get_task(task_id.to_string())
        .await?
        .ok_or_else(|| RpcError::invalid_params(format!("unknown task: {task_id}")))?;
    let worktree = task
        .worktree_path
        .clone()
        .ok_or_else(|| RpcError::invalid_params(format!("task {task_id} has no worktree")))?;
    Ok((task, PathBuf::from(worktree)))
}

/// Moves `task` to the end of the board column named `column_name`.
async fn move_task_to_column(
    ctx: &Context,
    task: &mut Task,
    column_name: &str,
) -> Result<(), RpcError> {
    let board = ctx
        .db
        .get_board()
        .await?
        .ok_or_else(|| RpcError::internal("no board exists"))?;
    let columns = ctx.db.list_columns(board.id.clone()).await?;
    let column = columns
        .iter()
        .find(|column| column.name == column_name)
        .ok_or_else(|| RpcError::internal(format!("no column named {column_name}")))?;
    let position = ctx
        .db
        .list_tasks(board.id)
        .await?
        .iter()
        .filter(|other| other.column_id == column.id && other.id != task.id)
        .count() as i64;

    task.column_id = column.id.clone();
    task.position = position;
    ctx.db.update_task(task.clone()).await?;
    Ok(())
}
