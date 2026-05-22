//! `agents.spawn` and the `sessions.*` methods — the agent/session lifecycle:
//! spawning Claude Code, resuming dead sessions, and streaming their events.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::mpsc;

use viban_core::agents::spawn_claude;
use viban_core::db::Db;
use viban_core::types::{Message, Session};
use viban_core::{new_id, AgentEvent};

use super::{now_millis, str_param, Context, RpcError, SessionRegistry};

/// Creates and persists a session, spawns a fresh Claude Code agent, and
/// starts streaming + persisting its events. Serves the `agents.spawn` method.
pub(super) async fn spawn(
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
        .await?;
    persist_user_message(&ctx.db, &session_id, prompt).await?;

    let (mut agent, events) = spawn_claude(&workdir, None)?;
    agent.send_message(prompt).await?;

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
pub(super) async fn send_message(
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
                agent.send_message(prompt).await?;
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
            .await?
            .ok_or_else(|| RpcError::invalid_params(format!("unknown session: {session_id}")))?;
        let claude_session_id = stored
            .claude_session_id
            .ok_or_else(|| RpcError::internal("session cannot be resumed: no Claude Code id"))?;

        let (mut agent, events) =
            spawn_claude(Path::new(&stored.project_path), Some(&claude_session_id))?;
        agent.send_message(prompt).await?;

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
pub(super) async fn list(ctx: &Context) -> Result<Value, RpcError> {
    let sessions = ctx.db.list_sessions().await?;
    Ok(json!({ "sessions": sessions }))
}

/// Returns a session and its full message history.
pub(super) async fn get(params: Value, ctx: &Context) -> Result<Value, RpcError> {
    let session_id = str_param(&params, "session_id")?.to_string();
    let session = ctx
        .db
        .get_session(session_id.clone())
        .await?
        .ok_or_else(|| RpcError::invalid_params(format!("unknown session: {session_id}")))?;
    let messages = ctx.db.get_messages(session_id).await?;
    Ok(json!({ "session": session, "messages": messages }))
}

/// Resolves the working directory for a session's agent: the worktree of the
/// attempt that session belongs to, otherwise the shared workspace.
async fn agent_workdir(ctx: &Context, session_id: &str) -> Result<PathBuf, RpcError> {
    let attempt = ctx
        .db
        .get_attempt_by_session(session_id.to_string())
        .await?;
    Ok(attempt
        .and_then(|attempt| attempt.worktree_path)
        .map(PathBuf::from)
        .unwrap_or_else(|| ctx.workspace.clone()))
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
    .await?;
    Ok(())
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
