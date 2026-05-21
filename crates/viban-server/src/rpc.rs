//! Hand-rolled JSON-RPC 2.0 request handling.
//!
//! Methods are namespaced `<area>.<action>`: `server.health`, `agents.spawn`,
//! `sessions.send_message`, `sessions.list`, `sessions.get`. A running agent
//! streams its output as `events.update` notifications, and every session and
//! message is persisted to SQLite so conversations survive a restart.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex};

use viban_core::agents::{spawn_claude, ClaudeSession};
use viban_core::db::Db;
use viban_core::types::{Message, Session};
use viban_core::{new_id, AgentEvent};

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

    ctx.db
        .create_session(Session {
            id: session_id.clone(),
            claude_session_id: None,
            title: make_title(prompt),
            created_at: now_millis(),
            project_path: ctx.workspace.display().to_string(),
        })
        .await
        .map_err(|err| RpcError::internal(format!("failed to create session: {err}")))?;
    persist_user_message(&ctx.db, &session_id, prompt).await?;

    let (mut agent, events) = spawn_claude(&ctx.workspace, None)
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

        let (mut agent, events) = spawn_claude(&ctx.workspace, Some(&claude_session_id))
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
