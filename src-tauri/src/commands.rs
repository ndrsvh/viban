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
