//! `tasks.start_session` and the `attempts.*` methods — the attempt
//! lifecycle: each attempt is a session plus, optionally, a git worktree.

use anyhow::Context as _;
use serde_json::{json, Value};

use viban_core::git;
use viban_core::new_id;
use viban_core::types::{Attempt, Task};

use super::{now_millis, str_param, Context, RpcError};

/// Starts a task's first session and links it to the task. Serves
/// `tasks.start_session`.
///
/// Idempotent: a task that already has a session returns its existing id.
///
/// With `without_git: true` the agent runs directly in the project folder and
/// no worktree is created. Otherwise the task gets an isolated git worktree;
/// when the project folder is not yet its own git repository this returns
/// `{ "needs_git_init": true }` unless the caller passes `init_git: true`, in
/// which case a repository is initialized first.
pub(super) async fn start_session(params: Value, ctx: &Context) -> Result<Value, RpcError> {
    let task_id = str_param(&params, "task_id")?.to_string();
    let init_git = params
        .get("init_git")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let without_git = params
        .get("without_git")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut task = ctx
        .db
        .get_task(task_id.clone())
        .await?
        .ok_or_else(|| RpcError::invalid_params(format!("unknown task: {task_id}")))?;

    if let Some(session_id) = &task.session_id {
        return Ok(json!({ "session_id": session_id }));
    }

    // No-git mode: run the agent straight in the project folder.
    if without_git {
        let session_id = create_task_attempt(ctx, &mut task, false).await?;
        return Ok(json!({ "session_id": session_id }));
    }

    // A worktree needs the project folder to be its own git repository with a
    // commit. If it is not, ask the caller how to proceed.
    if !repo_ready(ctx).await {
        if !init_git {
            return Ok(json!({ "needs_git_init": true }));
        }
        git::prepare_repo(&ctx.workspace).await?;
    }

    let session_id = create_task_attempt(ctx, &mut task, true).await?;
    Ok(json!({ "session_id": session_id }))
}

/// Starts an additional attempt for a task, leaving earlier attempts intact.
/// The attempt gets its own git worktree when the project is a ready
/// repository, and otherwise runs directly in the project folder — matching
/// how the task's first session was started.
pub(super) async fn create(params: Value, ctx: &Context) -> Result<Value, RpcError> {
    let task_id = str_param(&params, "task_id")?.to_string();
    let mut task = ctx
        .db
        .get_task(task_id.clone())
        .await?
        .ok_or_else(|| RpcError::invalid_params(format!("unknown task: {task_id}")))?;

    let session_id = create_task_attempt(ctx, &mut task, repo_ready(ctx).await).await?;
    Ok(json!({ "session_id": session_id }))
}

/// Lists a task's attempts, newest first.
pub(super) async fn list(params: Value, ctx: &Context) -> Result<Value, RpcError> {
    let task_id = str_param(&params, "task_id")?.to_string();
    let attempts = ctx.db.list_attempts(task_id).await?;
    Ok(json!({ "attempts": attempts }))
}

/// Makes an existing attempt the task's active one, repointing the task's
/// session, worktree, and branch at it.
pub(super) async fn activate(params: Value, ctx: &Context) -> Result<Value, RpcError> {
    let attempt_id = str_param(&params, "attempt_id")?.to_string();
    let attempt = ctx
        .db
        .get_attempt(attempt_id.clone())
        .await?
        .ok_or_else(|| RpcError::invalid_params(format!("unknown attempt: {attempt_id}")))?;
    let mut task = ctx
        .db
        .get_task(attempt.task_id.clone())
        .await?
        .ok_or_else(|| RpcError::internal("attempt references an unknown task"))?;

    task.session_id = attempt.session_id;
    task.worktree_path = attempt.worktree_path;
    task.branch = attempt.branch;
    ctx.db.update_task(task).await?;
    Ok(json!({ "ok": true }))
}

/// Whether the project folder is its own git repository with at least one
/// commit — the precondition for creating a worktree.
async fn repo_ready(ctx: &Context) -> bool {
    git::is_repo_root(&ctx.workspace).await && git::has_head(&ctx.workspace).await
}

/// Creates a new attempt for `task`: a session, an `attempts` row, and repoints
/// the task's active fields at it.
///
/// With `with_git` the attempt also gets its own git worktree + branch (the
/// project folder must already be a ready repository). Without it the agent
/// runs directly in the project folder, and the attempt carries no worktree.
async fn create_task_attempt(
    ctx: &Context,
    task: &mut Task,
    with_git: bool,
) -> Result<String, RpcError> {
    let attempt_id = new_id();
    let session_id = new_id();

    let (worktree_path, branch) = if with_git {
        let id_fragment: String = attempt_id.chars().take(8).collect();
        let branch = format!("viban/{}-{}", git::slugify(&task.title), id_fragment);
        let worktree_path = ctx.data_dir.join("worktrees").join(&attempt_id);
        if let Some(parent) = worktree_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .context("failed to create the worktree directory")?;
        }
        git::worktree_add(&ctx.workspace, &worktree_path, &branch).await?;
        (Some(worktree_path.display().to_string()), Some(branch))
    } else {
        (None, None)
    };

    ctx.db
        .create_attempt(Attempt {
            id: attempt_id,
            task_id: task.id.clone(),
            session_id: Some(session_id.clone()),
            worktree_path: worktree_path.clone(),
            branch: branch.clone(),
            created_at: now_millis(),
        })
        .await?;

    task.session_id = Some(session_id.clone());
    task.worktree_path = worktree_path;
    task.branch = branch;
    ctx.db.update_task(task.clone()).await?;

    Ok(session_id)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::rpc::test_support::{context, task};

    #[tokio::test]
    async fn start_session_without_git_creates_a_plain_attempt() {
        let (ctx, _ws, _data) = context().await;
        let task_id = task(&ctx, "Plain task").await;

        let result = super::start_session(
            json!({ "task_id": task_id.clone(), "without_git": true }),
            &ctx,
        )
        .await
        .expect("start_session");
        let session_id = result["session_id"].as_str().expect("a session id");

        let attempts = ctx.db.list_attempts(task_id).await.expect("attempts");
        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts[0].session_id.as_deref(), Some(session_id));
        assert!(
            attempts[0].worktree_path.is_none(),
            "no worktree in no-git mode"
        );
        assert!(attempts[0].branch.is_none(), "no branch in no-git mode");
    }

    #[tokio::test]
    async fn start_session_in_a_non_repo_folder_asks_for_git_init() {
        let (ctx, _ws, _data) = context().await;
        let task_id = task(&ctx, "Needs git").await;
        // The workspace tempdir is not a git repository.
        let result = super::start_session(json!({ "task_id": task_id }), &ctx)
            .await
            .expect("start_session");
        assert_eq!(result["needs_git_init"], true);
    }

    #[tokio::test]
    async fn start_session_is_idempotent() {
        let (ctx, _ws, _data) = context().await;
        let task_id = task(&ctx, "Once only").await;
        let first = super::start_session(
            json!({ "task_id": task_id.clone(), "without_git": true }),
            &ctx,
        )
        .await
        .expect("first start");
        let second = super::start_session(json!({ "task_id": task_id, "without_git": true }), &ctx)
            .await
            .expect("second start");
        assert_eq!(first["session_id"], second["session_id"]);
    }

    #[tokio::test]
    async fn create_adds_a_second_attempt() {
        let (ctx, _ws, _data) = context().await;
        let task_id = task(&ctx, "Two tries").await;
        super::start_session(
            json!({ "task_id": task_id.clone(), "without_git": true }),
            &ctx,
        )
        .await
        .expect("first attempt");
        super::create(json!({ "task_id": task_id.clone() }), &ctx)
            .await
            .expect("second attempt");

        let attempts = ctx.db.list_attempts(task_id).await.expect("attempts");
        assert_eq!(attempts.len(), 2);
    }

    #[tokio::test]
    async fn activate_repoints_the_task_at_an_earlier_attempt() {
        let (ctx, _ws, _data) = context().await;
        let task_id = task(&ctx, "Switch me").await;
        let first = super::start_session(
            json!({ "task_id": task_id.clone(), "without_git": true }),
            &ctx,
        )
        .await
        .expect("attempt one");
        let first_session = first["session_id"].as_str().expect("session").to_string();
        super::create(json!({ "task_id": task_id.clone() }), &ctx)
            .await
            .expect("attempt two");

        // The task now points at the second attempt; activate the first.
        let attempts = ctx
            .db
            .list_attempts(task_id.clone())
            .await
            .expect("attempts");
        let first_attempt = attempts
            .iter()
            .find(|a| a.session_id.as_deref() == Some(&first_session))
            .expect("the first attempt");
        super::activate(json!({ "attempt_id": first_attempt.id }), &ctx)
            .await
            .expect("activate");

        let task = ctx
            .db
            .get_task(task_id)
            .await
            .expect("task")
            .expect("the task exists");
        assert_eq!(task.session_id.as_deref(), Some(first_session.as_str()));
    }

    #[tokio::test]
    async fn list_returns_a_tasks_attempts() {
        let (ctx, _ws, _data) = context().await;
        let task_id = task(&ctx, "List me").await;
        super::start_session(
            json!({ "task_id": task_id.clone(), "without_git": true }),
            &ctx,
        )
        .await
        .expect("attempt");
        let result = super::list(json!({ "task_id": task_id }), &ctx)
            .await
            .expect("list");
        assert_eq!(result["attempts"].as_array().expect("attempts").len(), 1);
    }
}
