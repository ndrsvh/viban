//! Thin Tauri command proxies. Business logic lives in viban-server; these
//! commands only forward calls to it over JSON-RPC.

use serde_json::{json, Value};
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tokio::sync::oneshot;
use viban_core::exec::CommandOutput;
use viban_core::types::TaskStatusUpdate;
use viban_core::AgentEvent;

use crate::AppState;

/// Returns the path of the currently open project, or `None` if none is set.
#[tauri::command]
pub async fn current_project(state: State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(state.project.lock().await.clone())
}

/// Opens a native folder dialog, persists the chosen folder as the project,
/// and restarts the sidecar against it. Any folder is accepted — git is
/// initialized later, on demand, when a task first needs a worktree. Returns
/// the chosen path, or `None` if the user cancelled.
#[tauri::command]
pub async fn open_project(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let (tx, rx) = oneshot::channel();
    app.dialog().file().pick_folder(move |folder| {
        let _ = tx.send(folder);
    });
    let folder = rx
        .await
        .map_err(|_| "folder dialog closed unexpectedly".to_string())?;
    let Some(folder) = folder else {
        return Ok(None);
    };
    let path = folder
        .into_path()
        .map_err(|err| format!("invalid folder path: {err}"))?;

    let path_str = path.to_string_lossy().into_owned();
    crate::project::save(&app, &path_str).map_err(|err| err.to_string())?;
    *state.project.lock().await = Some(path_str.clone());
    // Wake the supervisor so it tears down the old sidecar and starts a new
    // one against this project.
    state.project_changed.notify_one();
    Ok(Some(path_str))
}

/// Returns viban-server's health report (`server.health`).
#[tauri::command]
pub async fn server_health(state: State<'_, AppState>) -> Result<Value, String> {
    let client = state.client().await.ok_or("server not connected")?;
    client
        .call("server.health", Value::Null)
        .await
        .map_err(|err| err.to_string())
}

/// Subscribes `on_event` to a session's agent events. Call this when a chat
/// view opens, before spawning or sending, so no event is missed.
#[tauri::command]
pub async fn open_session(
    session_id: String,
    on_event: Channel<AgentEvent>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let client = state.client().await.ok_or("server not connected")?;
    let mut events = client.subscribe(&session_id).await;
    tauri::async_runtime::spawn(async move {
        while let Some(value) = events.recv().await {
            match serde_json::from_value::<AgentEvent>(value) {
                Ok(event) => {
                    if on_event.send(event).is_err() {
                        break;
                    }
                }
                Err(err) => tracing::warn!(%err, "dropping malformed agent event"),
            }
        }
    });
    Ok(())
}

/// Stops forwarding a session's events (the agent keeps running server-side).
#[tauri::command]
pub async fn close_session(session_id: String, state: State<'_, AppState>) -> Result<(), String> {
    if let Some(client) = state.client().await {
        client.unsubscribe(&session_id).await;
    }
    Ok(())
}

/// Subscribes `on_event` to the `tasks` topic — live per-task agent status.
/// Call this while the board is shown so its cards reflect agent activity.
#[tauri::command]
pub async fn watch_task_status(
    on_event: Channel<TaskStatusUpdate>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let client = state.client().await.ok_or("server not connected")?;
    let mut events = client.subscribe("tasks").await;
    tauri::async_runtime::spawn(async move {
        while let Some(value) = events.recv().await {
            match serde_json::from_value::<TaskStatusUpdate>(value) {
                Ok(update) => {
                    if on_event.send(update).is_err() {
                        break;
                    }
                }
                Err(err) => tracing::warn!(%err, "dropping malformed task status"),
            }
        }
    });
    Ok(())
}

/// Stops forwarding `tasks` topic notifications.
#[tauri::command]
pub async fn unwatch_task_status(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(client) = state.client().await {
        client.unsubscribe("tasks").await;
    }
    Ok(())
}

/// Runs a shell command in a task's worktree (`tasks.run_command`). Output
/// streams on the `run:<task_id>` topic — subscribe with `watch_run` first.
#[tauri::command]
pub async fn run_command(
    task_id: String,
    command: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let client = state.client().await.ok_or("server not connected")?;
    client
        .call(
            "tasks.run_command",
            json!({ "task_id": task_id, "command": command }),
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

/// Subscribes `on_event` to a task's command-run output (`run:<task_id>`).
#[tauri::command]
pub async fn watch_run(
    task_id: String,
    on_event: Channel<CommandOutput>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let client = state.client().await.ok_or("server not connected")?;
    let mut events = client.subscribe(&format!("run:{task_id}")).await;
    tauri::async_runtime::spawn(async move {
        while let Some(value) = events.recv().await {
            match serde_json::from_value::<CommandOutput>(value) {
                Ok(output) => {
                    if on_event.send(output).is_err() {
                        break;
                    }
                }
                Err(err) => tracing::warn!(%err, "dropping malformed command output"),
            }
        }
    });
    Ok(())
}

/// Stops forwarding a task's command-run output.
#[tauri::command]
pub async fn unwatch_run(task_id: String, state: State<'_, AppState>) -> Result<(), String> {
    if let Some(client) = state.client().await {
        client.unsubscribe(&format!("run:{task_id}")).await;
    }
    Ok(())
}

/// Spawns a brand-new Claude Code session for `prompt`.
#[tauri::command]
pub async fn spawn_session(
    session_id: String,
    prompt: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let client = state.client().await.ok_or("server not connected")?;
    client
        .call(
            "agents.spawn",
            json!({ "session_id": session_id, "prompt": prompt }),
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

/// Sends a follow-up message to an existing session (resumed if necessary).
#[tauri::command]
pub async fn send_message(
    session_id: String,
    prompt: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let client = state.client().await.ok_or("server not connected")?;
    client
        .call(
            "sessions.send_message",
            json!({ "session_id": session_id, "prompt": prompt }),
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

/// Starts a task's first session (`tasks.start_session`). Returns
/// `{ session_id }` on success, or `{ needs_git_init: true }` when the project
/// folder must first be made a git repository. Pass `init_git: true` to
/// confirm initializing it, or `without_git: true` to run the agent directly
/// in the project folder with no worktree.
#[tauri::command]
pub async fn start_session(
    task_id: String,
    init_git: Option<bool>,
    without_git: Option<bool>,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let client = state.client().await.ok_or("server not connected")?;
    client
        .call(
            "tasks.start_session",
            json!({
                "task_id": task_id,
                "init_git": init_git.unwrap_or(false),
                "without_git": without_git.unwrap_or(false),
            }),
        )
        .await
        .map_err(|err| err.to_string())
}

/// Starts an additional attempt for a task (`attempts.create`). The attempt
/// matches the task's mode — a git worktree when the project is a repository,
/// otherwise a plain session. Returns `{ session_id }`.
#[tauri::command]
pub async fn create_attempt(task_id: String, state: State<'_, AppState>) -> Result<Value, String> {
    let client = state.client().await.ok_or("server not connected")?;
    client
        .call("attempts.create", json!({ "task_id": task_id }))
        .await
        .map_err(|err| err.to_string())
}

/// Lists a task's attempts, newest first (`attempts.list`).
#[tauri::command]
pub async fn list_attempts(task_id: String, state: State<'_, AppState>) -> Result<Value, String> {
    let client = state.client().await.ok_or("server not connected")?;
    client
        .call("attempts.list", json!({ "task_id": task_id }))
        .await
        .map_err(|err| err.to_string())
}

/// Makes an attempt the task's active one (`attempts.activate`).
#[tauri::command]
pub async fn activate_attempt(
    attempt_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let client = state.client().await.ok_or("server not connected")?;
    client
        .call("attempts.activate", json!({ "attempt_id": attempt_id }))
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

/// Lists every persisted session (`{ "sessions": [...] }`).
#[tauri::command]
pub async fn list_sessions(state: State<'_, AppState>) -> Result<Value, String> {
    let client = state.client().await.ok_or("server not connected")?;
    client
        .call("sessions.list", Value::Null)
        .await
        .map_err(|err| err.to_string())
}

/// Loads a session and its history (`{ "session": ..., "messages": [...] }`).
#[tauri::command]
pub async fn get_session(session_id: String, state: State<'_, AppState>) -> Result<Value, String> {
    let client = state.client().await.ok_or("server not connected")?;
    client
        .call("sessions.get", json!({ "session_id": session_id }))
        .await
        .map_err(|err| err.to_string())
}

/// Returns the workspace's board with its columns and tasks.
#[tauri::command]
pub async fn get_board(state: State<'_, AppState>) -> Result<Value, String> {
    let client = state.client().await.ok_or("server not connected")?;
    client
        .call("boards.get", Value::Null)
        .await
        .map_err(|err| err.to_string())
}

/// Creates a task at the end of a column.
#[tauri::command]
pub async fn create_task(
    column_id: String,
    title: String,
    description: String,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let client = state.client().await.ok_or("server not connected")?;
    client
        .call(
            "tasks.create",
            json!({ "column_id": column_id, "title": title, "description": description }),
        )
        .await
        .map_err(|err| err.to_string())
}

/// Updates a task — only the provided fields change.
#[tauri::command]
pub async fn update_task(
    task_id: String,
    title: Option<String>,
    description: Option<String>,
    session_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let client = state.client().await.ok_or("server not connected")?;
    client
        .call(
            "tasks.update",
            json!({
                "task_id": task_id,
                "title": title,
                "description": description,
                "session_id": session_id,
            }),
        )
        .await
        .map_err(|err| err.to_string())
}

/// Deletes a task.
#[tauri::command]
pub async fn delete_task(task_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let client = state.client().await.ok_or("server not connected")?;
    client
        .call("tasks.delete", json!({ "task_id": task_id }))
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

/// Applies a column's full task ordering.
#[tauri::command]
pub async fn reorder_tasks(
    column_id: String,
    task_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let client = state.client().await.ok_or("server not connected")?;
    client
        .call(
            "tasks.reorder",
            json!({ "column_id": column_id, "task_ids": task_ids }),
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

/// Returns a task's pending worktree changes for review (`git.diff`).
#[tauri::command]
pub async fn git_diff(task_id: String, state: State<'_, AppState>) -> Result<Value, String> {
    let client = state.client().await.ok_or("server not connected")?;
    client
        .call("git.diff", json!({ "task_id": task_id }))
        .await
        .map_err(|err| err.to_string())
}

/// Commits a task's worktree changes, moving it to Review (`git.commit`).
#[tauri::command]
pub async fn git_commit(task_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let client = state.client().await.ok_or("server not connected")?;
    client
        .call("git.commit", json!({ "task_id": task_id }))
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

/// Discards a task's worktree changes, moving it back to In Progress
/// (`git.restore`).
#[tauri::command]
pub async fn git_restore(task_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let client = state.client().await.ok_or("server not connected")?;
    client
        .call("git.restore", json!({ "task_id": task_id }))
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

/// Merges a task's branch into the project, finishing the task (`git.merge`).
#[tauri::command]
pub async fn git_merge(task_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let client = state.client().await.ok_or("server not connected")?;
    client
        .call("git.merge", json!({ "task_id": task_id }))
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

/// Saves a worktree checkpoint for a task (`checkpoints.create`).
#[tauri::command]
pub async fn create_checkpoint(
    task_id: String,
    label: String,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let client = state.client().await.ok_or("server not connected")?;
    client
        .call(
            "checkpoints.create",
            json!({ "task_id": task_id, "label": label }),
        )
        .await
        .map_err(|err| err.to_string())
}

/// Lists a task's saved checkpoints (`checkpoints.list`).
#[tauri::command]
pub async fn list_checkpoints(
    task_id: String,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let client = state.client().await.ok_or("server not connected")?;
    client
        .call("checkpoints.list", json!({ "task_id": task_id }))
        .await
        .map_err(|err| err.to_string())
}

/// Restores a task's worktree to a saved checkpoint (`checkpoints.restore`).
#[tauri::command]
pub async fn restore_checkpoint(
    checkpoint_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let client = state.client().await.ok_or("server not connected")?;
    client
        .call(
            "checkpoints.restore",
            json!({ "checkpoint_id": checkpoint_id }),
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}
