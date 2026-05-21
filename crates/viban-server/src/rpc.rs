//! Hand-rolled JSON-RPC 2.0 request handling.
//!
//! Methods are namespaced `<area>.<action>`: `server.health`, `agents.spawn`,
//! `sessions.send_message`. A spawned agent streams its output back as
//! `events.update` notifications, pushed onto the connection's outbound queue
//! by a background pump task.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex};

use viban_core::agents::{spawn_claude, ClaudeSession};
use viban_core::AgentEvent;

/// Running agent sessions for one connection, keyed by Claude session id.
pub type SessionRegistry = Arc<Mutex<HashMap<String, ClaudeSession>>>;

/// Shared, read-only state for method handlers.
pub struct Context {
    pub workspace: PathBuf,
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
        "sessions.send_message" => sessions_send_message(params, registry).await,
        other => Err(RpcError::method_not_found(other)),
    }
}

/// Spawns a Claude Code session, waits for its init event, and starts pumping
/// its events to the client as `events.update` notifications.
async fn agents_spawn(
    params: Value,
    ctx: &Context,
    registry: &SessionRegistry,
    outbound: &mpsc::UnboundedSender<String>,
) -> Result<Value, RpcError> {
    let prompt = str_param(&params, "prompt")?;
    let subscription_id = str_param(&params, "subscription_id")?.to_string();

    let (mut session, mut events) = spawn_claude(&ctx.workspace)
        .map_err(|err| RpcError::internal(format!("failed to spawn agent: {err}")))?;

    // Send the prompt before waiting for the init event: Claude Code does not
    // emit its `system/init` line until it has received the first stdin
    // message, so blocking on init first would deadlock.
    session
        .send_message(prompt)
        .await
        .map_err(|err| RpcError::internal(format!("failed to send prompt: {err}")))?;

    let session_id =
        match tokio::time::timeout(Duration::from_secs(30), wait_for_session_id(&mut events)).await
        {
            Ok(Some(session_id)) => session_id,
            Ok(None) => return Err(RpcError::internal("agent exited before initializing")),
            Err(_) => {
                session.kill().await;
                return Err(RpcError::internal("agent initialization timed out"));
            }
        };

    registry.lock().await.insert(session_id.clone(), session);
    spawn_event_pump(
        events,
        subscription_id,
        session_id.clone(),
        Arc::clone(registry),
        outbound.clone(),
    );

    Ok(json!({ "session_id": session_id }))
}

/// Sends a follow-up message to an already-running session.
async fn sessions_send_message(
    params: Value,
    registry: &SessionRegistry,
) -> Result<Value, RpcError> {
    let session_id = str_param(&params, "session_id")?;
    let prompt = str_param(&params, "prompt")?;

    let mut sessions = registry.lock().await;
    let session = sessions
        .get_mut(session_id)
        .ok_or_else(|| RpcError::invalid_params(format!("unknown session: {session_id}")))?;
    session
        .send_message(prompt)
        .await
        .map_err(|err| RpcError::internal(format!("failed to send message: {err}")))?;
    Ok(json!({ "ok": true }))
}

/// Consumes events until the init event carrying the session id arrives.
async fn wait_for_session_id(events: &mut mpsc::UnboundedReceiver<AgentEvent>) -> Option<String> {
    while let Some(event) = events.recv().await {
        if let AgentEvent::SessionStarted { session_id } = event {
            return Some(session_id);
        }
    }
    None
}

/// Forwards every agent event as an `events.update` notification until the
/// agent exits, then drops the session from the registry.
fn spawn_event_pump(
    mut events: mpsc::UnboundedReceiver<AgentEvent>,
    subscription_id: String,
    session_id: String,
    registry: SessionRegistry,
    outbound: mpsc::UnboundedSender<String>,
) {
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            let notification = json!({
                "jsonrpc": "2.0",
                "method": "events.update",
                "params": { "subscription_id": subscription_id, "event": event },
            });
            if outbound.send(notification.to_string()).is_err() {
                break;
            }
        }
        registry.lock().await.remove(&session_id);
        tracing::debug!(session_id, "agent session ended");
    });
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
