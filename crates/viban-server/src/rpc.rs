//! Hand-rolled JSON-RPC 2.0 request handling.
//!
//! Methods are namespaced `<area>.<action>`: `server.health`, `agents.spawn`,
//! `sessions.send_message`, `sessions.list`, `sessions.get`. A running agent
//! streams its output as `events.update` notifications, and every session and
//! message is persisted to SQLite so conversations survive a restart.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex};

use viban_core::agents::{spawn_claude, ClaudeSession};
use viban_core::db::Db;
use viban_core::types::{Message, Session, Task};
use viban_core::{git, new_id, AgentEvent};

/// Running agent sessions for one connection, keyed by viban session id.
pub type SessionRegistry = Arc<Mutex<HashMap<String, ClaudeSession>>>;

/// Shared state for method handlers.
pub struct Context {
    pub workspace: PathBuf,
    pub db: Db,
}

#[derive(Debug, Deserialize)]
struct Request {
    id: Value,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct Response {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
struct RpcError {
    code: i32,
    message: String,
}

impl RpcError {
    fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    fn method_not_found(method: &str) -> Self {
        Self::new(-32601, format!("method not found: {method}"))
    }
    fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(-32602, message)
    }
    fn internal(message: impl Into<String>) -> Self {
        Self::new(-32603, message)
    }
}

/// Parses a raw JSON-RPC request, dispatches it, and returns the serialized
/// response.
pub async fn handle(
    raw: &str,
    ctx: &Context,
    registry: &SessionRegistry,
    outbound: &mpsc::UnboundedSender<String>,
) -> String {
    let request: Request = match serde_json::from_str(raw) {
        Ok(request) => request,
        Err(err) => {
            return serialize(Response {
                jsonrpc: "2.0",
                id: Value::Null,
                result: None,
                error: Some(RpcError::new(-32700, format!("parse error: {err}"))),
            });
        }
    };

    let response = match dispatch(&request.method, request.params, ctx, registry, outbound).await {
        Ok(result) => Response {
            jsonrpc: "2.0",
            id: request.id,
            result: Some(result),
            error: None,
        },
        Err(error) => Response {
            jsonrpc: "2.0",
            id: request.id,
            result: None,
            error: Some(error),
        },
    };
    serialize(response)
}

async fn dispatch(
    method: &str,
    params: Value,
    ctx: &Context,
    registry: &SessionRegistry,
    outbound: &mpsc::UnboundedSender<String>,
) -> Result<Value, RpcError> {
    match method {
        "server.health" => Ok(json!({
            "status": "ok",
            "version": viban_core::VERSION,
            "workspace": ctx.workspace.display().to_string(),
        })),
        "agents.spawn" => agents_spawn(params, ctx, registry, outbound).await,
        "sessions.send_message" => sessions_send_message(params, ctx, registry, outbound).await,
        "sessions.list" => sessions_list(ctx).await,
        "sessions.get" => sessions_get(params, ctx).await,
        "boards.get" => boards_get(ctx).await,
        "tasks.create" => tasks_create(params, ctx).await,
        "tasks.update" => tasks_update(params, ctx).await,
        "tasks.delete" => tasks_delete(params, ctx, registry).await,
        "tasks.reorder" => tasks_reorder(params, ctx).await,
        "tasks.start_session" => tasks_start_session(params, ctx).await,
        "git.diff" => git_diff(params, ctx).await,
        "git.commit" => git_commit(params, ctx).await,
        "git.restore" => git_restore(params, ctx).await,
        other => Err(RpcError::method_not_found(other)),
    }
}

/// Creates and persists a session, spawns a fresh Claude Code agent, and
/// starts streaming + persisting its events.
async fn agents_spawn(
    params: Value,
    ctx: &Context,
    registry: &SessionRegistry,
    outbound: &mpsc::UnboundedSender<String>,
) -> Result<Value, RpcError> {
    let session_id = str_param(&params, "session_id")?.to_string();
    let prompt = str_param(&params, "prompt")?;

    let workdir = agent_workdir(ctx, &session_id).await?;
    ctx.db
        .create_session(Session {
            id: session_id.clone(),
            claude_session_id: None,
            title: make_title(prompt),
            created_at: now_millis(),
            project_path: workdir.display().to_string(),
        })
        .await
        .map_err(|err| RpcError::internal(format!("failed to create session: {err}")))?;
    persist_user_message(&ctx.db, &session_id, prompt).await?;

    let (mut agent, events) = spawn_claude(&workdir, None)
        .map_err(|err| RpcError::internal(format!("failed to spawn agent: {err}")))?;
    agent
        .send_message(prompt)
        .await
        .map_err(|err| RpcError::internal(format!("failed to send prompt: {err}")))?;

    registry.lock().await.insert(session_id.clone(), agent);
    spawn_event_pump(
        events,
        session_id.clone(),
        ctx.db.clone(),
        Arc::clone(registry),
        outbound.clone(),
    );

    Ok(json!({ "session_id": session_id }))
}

/// Sends a follow-up message, transparently resuming the agent from SQLite if
/// it is no longer running.
async fn sessions_send_message(
    params: Value,
    ctx: &Context,
    registry: &SessionRegistry,
    outbound: &mpsc::UnboundedSender<String>,
) -> Result<Value, RpcError> {
    let session_id = str_param(&params, "session_id")?.to_string();
    let prompt = str_param(&params, "prompt")?;

    // Live session: send straight to the running agent.
    let delivered = {
        let mut sessions = registry.lock().await;
        match sessions.get_mut(&session_id) {
            Some(agent) => {
                agent
                    .send_message(prompt)
                    .await
                    .map_err(|err| RpcError::internal(format!("failed to send message: {err}")))?;
                true
            }
            None => false,
        }
    };

    // Dead session: resume it from its stored Claude Code session id.
    if !delivered {
        let stored = ctx
            .db
            .get_session(session_id.clone())
            .await
            .map_err(|err| RpcError::internal(format!("db error: {err}")))?
            .ok_or_else(|| RpcError::invalid_params(format!("unknown session: {session_id}")))?;
        let claude_session_id = stored
            .claude_session_id
            .ok_or_else(|| RpcError::internal("session cannot be resumed: no Claude Code id"))?;

        let (mut agent, events) =
            spawn_claude(Path::new(&stored.project_path), Some(&claude_session_id))
                .map_err(|err| RpcError::internal(format!("failed to resume agent: {err}")))?;
        agent
            .send_message(prompt)
            .await
            .map_err(|err| RpcError::internal(format!("failed to send message: {err}")))?;

        registry.lock().await.insert(session_id.clone(), agent);
        spawn_event_pump(
            events,
            session_id.clone(),
            ctx.db.clone(),
            Arc::clone(registry),
            outbound.clone(),
        );
    }

    persist_user_message(&ctx.db, &session_id, prompt).await?;
    Ok(json!({ "ok": true }))
}

/// Lists every persisted session, newest first.
async fn sessions_list(ctx: &Context) -> Result<Value, RpcError> {
    let sessions = ctx
        .db
        .list_sessions()
        .await
        .map_err(|err| RpcError::internal(format!("db error: {err}")))?;
    Ok(json!({ "sessions": sessions }))
}

/// Returns a session and its full message history.
async fn sessions_get(params: Value, ctx: &Context) -> Result<Value, RpcError> {
    let session_id = str_param(&params, "session_id")?.to_string();
    let session = ctx
        .db
        .get_session(session_id.clone())
        .await
        .map_err(|err| RpcError::internal(format!("db error: {err}")))?
        .ok_or_else(|| RpcError::invalid_params(format!("unknown session: {session_id}")))?;
    let messages = ctx
        .db
        .get_messages(session_id)
        .await
        .map_err(|err| RpcError::internal(format!("db error: {err}")))?;
    Ok(json!({ "session": session, "messages": messages }))
}

/// Returns the workspace's board with its columns and tasks.
async fn boards_get(ctx: &Context) -> Result<Value, RpcError> {
    let board = ctx
        .db
        .get_board()
        .await
        .map_err(|err| RpcError::internal(format!("db error: {err}")))?
        .ok_or_else(|| RpcError::internal("no board exists"))?;
    let columns = ctx
        .db
        .list_columns(board.id.clone())
        .await
        .map_err(|err| RpcError::internal(format!("db error: {err}")))?;
    let tasks = ctx
        .db
        .list_tasks(board.id.clone())
        .await
        .map_err(|err| RpcError::internal(format!("db error: {err}")))?;
    Ok(json!({ "board": board, "columns": columns, "tasks": tasks }))
}

/// Creates a task at the end of a column.
async fn tasks_create(params: Value, ctx: &Context) -> Result<Value, RpcError> {
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
        .await
        .map_err(|err| RpcError::internal(format!("db error: {err}")))?
        .ok_or_else(|| RpcError::internal("no board exists"))?;
    let position = ctx
        .db
        .list_tasks(board.id)
        .await
        .map_err(|err| RpcError::internal(format!("db error: {err}")))?
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
    ctx.db
        .create_task(task.clone())
        .await
        .map_err(|err| RpcError::internal(format!("db error: {err}")))?;
    Ok(json!({ "task": task }))
}

/// Updates a task's title, description, and/or linked session.
async fn tasks_update(params: Value, ctx: &Context) -> Result<Value, RpcError> {
    let task_id = str_param(&params, "task_id")?.to_string();
    let mut task = ctx
        .db
        .get_task(task_id.clone())
        .await
        .map_err(|err| RpcError::internal(format!("db error: {err}")))?
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

    ctx.db
        .update_task(task.clone())
        .await
        .map_err(|err| RpcError::internal(format!("db error: {err}")))?;
    Ok(json!({ "task": task }))
}

/// Deletes a task, tearing down its worktree, branch, and any live agent.
async fn tasks_delete(
    params: Value,
    ctx: &Context,
    registry: &SessionRegistry,
) -> Result<Value, RpcError> {
    let task_id = str_param(&params, "task_id")?.to_string();

    let task = ctx
        .db
        .get_task(task_id.clone())
        .await
        .map_err(|err| RpcError::internal(format!("db error: {err}")))?;

    if let Some(task) = task {
        if let Some(session_id) = &task.session_id {
            registry.lock().await.remove(session_id);
        }
        if let Some(worktree_path) = &task.worktree_path {
            if let Err(err) =
                git::worktree_remove(&ctx.workspace, Path::new(worktree_path), true).await
            {
                tracing::warn!(%err, "failed to remove worktree");
            }
        }
        if let Some(branch) = &task.branch {
            if let Err(err) = git::branch_delete(&ctx.workspace, branch).await {
                tracing::warn!(%err, "failed to delete branch");
            }
        }
    }

    ctx.db
        .delete_task(task_id)
        .await
        .map_err(|err| RpcError::internal(format!("db error: {err}")))?;
    Ok(json!({ "ok": true }))
}

/// Creates a git worktree + branch for a task and links a fresh session to it.
/// Idempotent: a task that already has a session returns its existing id.
async fn tasks_start_session(params: Value, ctx: &Context) -> Result<Value, RpcError> {
    let task_id = str_param(&params, "task_id")?.to_string();
    let mut task = ctx
        .db
        .get_task(task_id.clone())
        .await
        .map_err(|err| RpcError::internal(format!("db error: {err}")))?
        .ok_or_else(|| RpcError::invalid_params(format!("unknown task: {task_id}")))?;

    if let Some(session_id) = &task.session_id {
        return Ok(json!({ "session_id": session_id }));
    }

    let id_fragment: String = task_id.chars().take(8).collect();
    let branch = format!("viban/{}-{}", git::slugify(&task.title), id_fragment);
    let worktree_path = ctx
        .workspace
        .join(".viban")
        .join("worktrees")
        .join(&task_id);

    if let Some(parent) = worktree_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|err| RpcError::internal(format!("failed to create worktree dir: {err}")))?;
    }
    git::worktree_add(&ctx.workspace, &worktree_path, &branch)
        .await
        .map_err(|err| RpcError::internal(format!("failed to create worktree: {err}")))?;

    let session_id = new_id();
    task.session_id = Some(session_id.clone());
    task.worktree_path = Some(worktree_path.display().to_string());
    task.branch = Some(branch);
    ctx.db
        .update_task(task)
        .await
        .map_err(|err| RpcError::internal(format!("db error: {err}")))?;

    Ok(json!({ "session_id": session_id }))
}

/// Resolves the working directory for a session's agent: the linked task's
/// worktree path if it has one, otherwise the shared workspace.
async fn agent_workdir(ctx: &Context, session_id: &str) -> Result<PathBuf, RpcError> {
    let task = ctx
        .db
        .get_task_by_session(session_id.to_string())
        .await
        .map_err(|err| RpcError::internal(format!("db error: {err}")))?;
    Ok(task
        .and_then(|task| task.worktree_path)
        .map(PathBuf::from)
        .unwrap_or_else(|| ctx.workspace.clone()))
}

/// Returns a task's pending worktree changes for review.
async fn git_diff(params: Value, ctx: &Context) -> Result<Value, RpcError> {
    let task_id = str_param(&params, "task_id")?;
    let (_task, worktree) = task_worktree(ctx, task_id).await?;
    let files = git::worktree_diff(&worktree)
        .await
        .map_err(|err| RpcError::internal(format!("failed to diff worktree: {err}")))?;
    Ok(json!({ "files": files }))
}

/// Commits a task's worktree changes and moves the task to the Review column.
async fn git_commit(params: Value, ctx: &Context) -> Result<Value, RpcError> {
    let task_id = str_param(&params, "task_id")?;
    let (mut task, worktree) = task_worktree(ctx, task_id).await?;
    git::commit_all(&worktree, &task.title)
        .await
        .map_err(|err| RpcError::internal(format!("failed to commit worktree: {err}")))?;
    move_task_to_column(ctx, &mut task, "Review").await?;
    Ok(json!({ "ok": true }))
}

/// Discards a task's worktree changes and moves the task back to In Progress.
async fn git_restore(params: Value, ctx: &Context) -> Result<Value, RpcError> {
    let task_id = str_param(&params, "task_id")?;
    let (mut task, worktree) = task_worktree(ctx, task_id).await?;
    git::discard_all(&worktree)
        .await
        .map_err(|err| RpcError::internal(format!("failed to discard worktree: {err}")))?;
    move_task_to_column(ctx, &mut task, "In Progress").await?;
    Ok(json!({ "ok": true }))
}

/// Loads a task and the path of its git worktree, erroring if it has none.
async fn task_worktree(ctx: &Context, task_id: &str) -> Result<(Task, PathBuf), RpcError> {
    let task = ctx
        .db
        .get_task(task_id.to_string())
        .await
        .map_err(|err| RpcError::internal(format!("db error: {err}")))?
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
        .await
        .map_err(|err| RpcError::internal(format!("db error: {err}")))?
        .ok_or_else(|| RpcError::internal("no board exists"))?;
    let columns = ctx
        .db
        .list_columns(board.id.clone())
        .await
        .map_err(|err| RpcError::internal(format!("db error: {err}")))?;
    let column = columns
        .iter()
        .find(|column| column.name == column_name)
        .ok_or_else(|| RpcError::internal(format!("no column named {column_name}")))?;
    let position = ctx
        .db
        .list_tasks(board.id)
        .await
        .map_err(|err| RpcError::internal(format!("db error: {err}")))?
        .iter()
        .filter(|other| other.column_id == column.id && other.id != task.id)
        .count() as i64;

    task.column_id = column.id.clone();
    task.position = position;
    ctx.db
        .update_task(task.clone())
        .await
        .map_err(|err| RpcError::internal(format!("db error: {err}")))?;
    Ok(())
}

/// Applies a column's full task ordering (also re-parents moved tasks).
async fn tasks_reorder(params: Value, ctx: &Context) -> Result<Value, RpcError> {
    let column_id = str_param(&params, "column_id")?.to_string();
    let task_ids = params
        .get("task_ids")
        .and_then(Value::as_array)
        .ok_or_else(|| RpcError::invalid_params("missing or non-array 'task_ids'"))?
        .iter()
        .filter_map(Value::as_str)
        .map(String::from)
        .collect::<Vec<_>>();
    ctx.db
        .reorder_column(column_id, task_ids)
        .await
        .map_err(|err| RpcError::internal(format!("db error: {err}")))?;
    Ok(json!({ "ok": true }))
}

/// Forwards every agent event as an `events.update` notification, persists the
/// conversational ones, records the Claude Code session id, and drops the
/// session when the agent exits.
fn spawn_event_pump(
    mut events: mpsc::UnboundedReceiver<AgentEvent>,
    session_id: String,
    db: Db,
    registry: SessionRegistry,
    outbound: mpsc::UnboundedSender<String>,
) {
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            if let AgentEvent::SessionStarted {
                session_id: claude_id,
            } = &event
            {
                if let Err(err) = db
                    .set_claude_session_id(session_id.clone(), claude_id.clone())
                    .await
                {
                    tracing::warn!(%err, "failed to store claude session id");
                }
            }
            if let Err(err) = persist_event(&db, &session_id, &event).await {
                tracing::warn!(%err, "failed to persist agent event");
            }

            let notification = json!({
                "jsonrpc": "2.0",
                "method": "events.update",
                "params": { "subscription_id": session_id, "event": event },
            });
            if outbound.send(notification.to_string()).is_err() {
                break;
            }
        }
        registry.lock().await.remove(&session_id);
        tracing::debug!(session_id, "agent session ended");
    });
}

/// Persists the conversational events. Control events (init, result, raw)
/// carry no conversation and are skipped.
async fn persist_event(db: &Db, session_id: &str, event: &AgentEvent) -> anyhow::Result<()> {
    let (role, content) = match event {
        AgentEvent::AssistantText { text } => ("assistant", text.clone()),
        AgentEvent::ToolUse { name, .. } => ("tool", format!("using {name}")),
        AgentEvent::Error { message } => ("error", message.clone()),
        AgentEvent::SessionStarted { .. } | AgentEvent::Result { .. } | AgentEvent::Raw { .. } => {
            return Ok(())
        }
    };
    db.insert_message(Message {
        id: new_id(),
        session_id: session_id.to_string(),
        role: role.to_string(),
        content,
        created_at: now_millis(),
        raw_json: serde_json::to_string(event).ok(),
    })
    .await
}

async fn persist_user_message(db: &Db, session_id: &str, prompt: &str) -> Result<(), RpcError> {
    db.insert_message(Message {
        id: new_id(),
        session_id: session_id.to_string(),
        role: "user".to_string(),
        content: prompt.to_string(),
        created_at: now_millis(),
        raw_json: None,
    })
    .await
    .map_err(|err| RpcError::internal(format!("failed to persist message: {err}")))
}

/// The session's first prompt line, trimmed to ~50 characters.
fn make_title(prompt: &str) -> String {
    let first_line = prompt.lines().next().unwrap_or("").trim();
    let title: String = first_line.chars().take(50).collect();
    if first_line.chars().count() > 50 {
        format!("{title}…")
    } else if title.is_empty() {
        "Untitled session".to_string()
    } else {
        title
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

fn str_param<'a>(params: &'a Value, key: &str) -> Result<&'a str, RpcError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::invalid_params(format!("missing or non-string '{key}'")))
}

fn serialize(response: Response) -> String {
    serde_json::to_string(&response).unwrap_or_else(|_| {
        r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"internal error"}}"#
            .to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::make_title;

    #[test]
    fn make_title_keeps_a_short_prompt_verbatim() {
        assert_eq!(make_title("Fix the login bug"), "Fix the login bug");
    }

    #[test]
    fn make_title_uses_only_the_first_line() {
        assert_eq!(make_title("First line\nsecond line\nthird"), "First line");
    }

    #[test]
    fn make_title_truncates_and_ellipsizes_long_prompts() {
        let long = "a".repeat(80);
        let title = make_title(&long);
        assert!(title.ends_with('…'), "long titles end with an ellipsis");
        assert_eq!(title.chars().count(), 51, "50 characters plus the ellipsis");
    }

    #[test]
    fn make_title_does_not_truncate_at_exactly_fifty_characters() {
        let exact = "a".repeat(50);
        assert_eq!(make_title(&exact), exact);
    }

    #[test]
    fn make_title_falls_back_for_blank_input() {
        assert_eq!(make_title(""), "Untitled session");
        assert_eq!(make_title("   \n  "), "Untitled session");
    }
}
