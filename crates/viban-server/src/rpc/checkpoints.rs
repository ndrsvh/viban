//! The `checkpoints.*` methods — saving and restoring worktree checkpoints.
//!
//! A checkpoint is a commit on the task's branch; restoring resets the
//! worktree to it.

use serde_json::{json, Value};

use viban_core::git;
use viban_core::new_id;
use viban_core::types::Checkpoint;

use super::review::task_worktree;
use super::{now_millis, str_param, Context, RpcError};

/// Saves a checkpoint of a task's worktree — a commit it can be rolled back
/// to. Serves `checkpoints.create`.
pub(super) async fn create(params: Value, ctx: &Context) -> Result<Value, RpcError> {
    let task_id = str_param(&params, "task_id")?;
    let label = str_param(&params, "label")?;
    let (_task, worktree) = task_worktree(ctx, task_id).await?;

    let commit_sha = git::create_checkpoint(&worktree, label).await?;
    let checkpoint = Checkpoint {
        id: new_id(),
        task_id: task_id.to_string(),
        commit_sha,
        label: label.to_string(),
        created_at: now_millis(),
    };
    ctx.db.create_checkpoint(checkpoint.clone()).await?;
    Ok(json!({ "checkpoint": checkpoint }))
}

/// Lists a task's saved checkpoints, oldest first.
pub(super) async fn list(params: Value, ctx: &Context) -> Result<Value, RpcError> {
    let task_id = str_param(&params, "task_id")?.to_string();
    let checkpoints = ctx.db.list_checkpoints(task_id).await?;
    Ok(json!({ "checkpoints": checkpoints }))
}

/// Resets a task's worktree to a saved checkpoint — discarding everything
/// done since. Serves `checkpoints.restore`.
pub(super) async fn restore(params: Value, ctx: &Context) -> Result<Value, RpcError> {
    let checkpoint_id = str_param(&params, "checkpoint_id")?.to_string();
    let checkpoint = ctx
        .db
        .get_checkpoint(checkpoint_id.clone())
        .await?
        .ok_or_else(|| RpcError::invalid_params(format!("unknown checkpoint: {checkpoint_id}")))?;
    let (_task, worktree) = task_worktree(ctx, &checkpoint.task_id).await?;
    git::restore_checkpoint(&worktree, &checkpoint.commit_sha).await?;
    Ok(json!({ "ok": true }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::rpc::test_support::{context, task};

    #[tokio::test]
    async fn create_rejects_a_task_without_a_worktree() {
        let (ctx, _ws, _data) = context().await;
        let task_id = task(&ctx, "No worktree").await;
        let err = super::create(json!({ "task_id": task_id, "label": "x" }), &ctx)
            .await
            .expect_err("a worktree-less task errors");
        assert_eq!(err.code, -32602);
    }

    #[tokio::test]
    async fn restore_rejects_an_unknown_checkpoint() {
        let (ctx, _ws, _data) = context().await;
        let err = super::restore(json!({ "checkpoint_id": "ghost" }), &ctx)
            .await
            .expect_err("an unknown checkpoint errors");
        assert_eq!(err.code, -32602);
    }

    #[tokio::test]
    async fn list_is_empty_for_a_task_with_no_checkpoints() {
        let (ctx, _ws, _data) = context().await;
        let task_id = task(&ctx, "Fresh").await;
        let result = super::list(json!({ "task_id": task_id }), &ctx)
            .await
            .expect("list");
        assert_eq!(
            result["checkpoints"].as_array().expect("checkpoints").len(),
            0
        );
    }
}
