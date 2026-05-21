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

/// Spawns a Claude Code session for `prompt`, streaming its events to
/// `on_event`. Returns the new session id for follow-up messages.
#[tauri::command]
pub async fn spawn_session(
    prompt: String,
    on_event: Channel<AgentEvent>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let client = state.client().await.ok_or("server not connected")?;
    let subscription_id = new_id();

    // Subscribe before spawning so no early event is missed.
    let mut events = client.subscribe(&subscription_id).await;
    let response = client
        .call(
            "agents.spawn",
            json!({ "prompt": prompt, "subscription_id": subscription_id }),
        )
        .await
        .map_err(|err| err.to_string())?;

    let session_id = response
        .get("session_id")
        .and_then(Value::as_str)
        .ok_or("server did not return a session id")?
        .to_string();

    // Forward agent events to the frontend channel until the session ends.
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

    Ok(session_id)
}

/// Sends a follow-up message to a running session.
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

/// A short random hex id for an event subscription.
fn new_id() -> String {
    use std::fmt::Write as _;

    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).expect("OS RNG must be available");
    let mut id = String::with_capacity(16);
    for byte in bytes {
        let _ = write!(id, "{byte:02x}");
    }
    id
}
