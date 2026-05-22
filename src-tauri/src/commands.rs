//! Thin Tauri command proxies. Business logic lives in viban-server; these
//! commands only forward calls to it over JSON-RPC.

use serde_json::{json, Value};
use tauri::ipc::Channel;
use tauri::State;
use viban_core::AgentEvent;

use crate::AppState;

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
