//! Thin Tauri command proxies. Business logic lives in viban-server; these
//! commands only forward calls to it over JSON-RPC.

use serde_json::Value;
use tauri::State;

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
