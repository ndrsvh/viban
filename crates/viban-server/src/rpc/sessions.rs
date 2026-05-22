//! `agents.spawn` and the `sessions.*` methods — the agent/session lifecycle:
//! spawning Claude Code, resuming dead sessions, and streaming their events.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::mpsc;

use viban_core::agents::spawn_claude;
use viban_core::db::Db;
use viban_core::types::{AgentStatus, Message, Session, TaskStatusUpdate};
use viban_core::{new_id, AgentEvent};

use super::{now_millis, str_param, Context, EventSink, RpcError, SessionRegistry, TaskStatuses};

/// Creates and persists a session, spawns a fresh Claude Code agent, and
/// starts streaming + persisting its events. Serves the `agents.spawn` method.
pub(super) async fn spawn(params: Value, ctx: &Context) -> Result<Value, RpcError> {
    let session_id = str_param(&params, "session_id")?.to_string();
    let prompt = str_param(&params, "prompt")?;

    let workdir = agent_workdir(ctx, &session_id).await?;
    let task_id = task_of(ctx, &session_id).await?;
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

    let (mut agent, agent_events) = spawn_claude(&workdir, None)?;
    agent.send_message(prompt).await?;

    ctx.registry.lock().await.insert(session_id.clone(), agent);
    if let Some(task_id) = &task_id {
        publish_status(&ctx.statuses, &ctx.events, task_id, AgentStatus::Running).await;
    }
    spawn_event_pump(
        agent_events,
        session_id.clone(),
        task_id,
        ctx.db.clone(),
        Arc::clone(&ctx.registry),
        ctx.events.clone(),
        Arc::clone(&ctx.statuses),
    );

    Ok(json!({ "session_id": session_id }))
}

/// Sends a follow-up message, transparently resuming the agent from SQLite if
/// it is no longer running.
pub(super) async fn send_message(params: Value, ctx: &Context) -> Result<Value, RpcError> {
    let session_id = str_param(&params, "session_id")?.to_string();
    let prompt = str_param(&params, "prompt")?;
    let task_id = task_of(ctx, &session_id).await?;

    // Live session: send straight to the running agent.
    let delivered = {
        let mut sessions = ctx.registry.lock().await;
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

        let (mut agent, agent_events) =
            spawn_claude(Path::new(&stored.project_path), Some(&claude_session_id))?;
        agent.send_message(prompt).await?;

        ctx.registry.lock().await.insert(session_id.clone(), agent);
        spawn_event_pump(
            agent_events,
            session_id.clone(),
            task_id.clone(),
            ctx.db.clone(),
            Arc::clone(&ctx.registry),
            ctx.events.clone(),
            Arc::clone(&ctx.statuses),
        );
    }

    // The agent is now processing this turn.
    if let Some(task_id) = &task_id {
        publish_status(&ctx.statuses, &ctx.events, task_id, AgentStatus::Running).await;
    }
    persist_user_message(&ctx.db, &session_id, prompt).await?;
    Ok(json!({ "ok": true }))
}

/// Lists every persisted session, newest first.
pub(super) async fn list(ctx: &Context) -> Result<Value, RpcError> {
    let sessions = ctx.db.list_sessions().await?;
    Ok(json!({ "sessions": sessions }))
}

/// Returns a session, its full message history, and the files it has edited.
pub(super) async fn get(params: Value, ctx: &Context) -> Result<Value, RpcError> {
    let session_id = str_param(&params, "session_id")?.to_string();
    let session = ctx
        .db
        .get_session(session_id.clone())
        .await?
        .ok_or_else(|| RpcError::invalid_params(format!("unknown session: {session_id}")))?;
    let messages = ctx.db.get_messages(session_id.clone()).await?;
    let files = ctx.db.list_session_files(session_id).await?;
    Ok(json!({ "session": session, "messages": messages, "files": files }))
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

/// The id of the task a session belongs to, via its attempt, if any.
async fn task_of(ctx: &Context, session_id: &str) -> Result<Option<String>, RpcError> {
    let attempt = ctx
        .db
        .get_attempt_by_session(session_id.to_string())
        .await?;
    Ok(attempt.map(|attempt| attempt.task_id))
}

/// The path of the file an agent event edited, if it is a file-modifying
/// tool call. Used to build a session's file footprint.
fn edited_path(event: &AgentEvent) -> Option<String> {
    let AgentEvent::ToolUse { name, input } = event else {
        return None;
    };
    // Claude Code's file-editing tools carry the path in `file_path`; the
    // notebook tool uses `notebook_path`. Read-only tools are ignored.
    let key = match name.as_str() {
        "Edit" | "Write" | "MultiEdit" => "file_path",
        "NotebookEdit" => "notebook_path",
        _ => return None,
    };
    input
        .get(key)
        .and_then(|value| value.as_str())
        .map(String::from)
}

/// The status transition an agent event triggers, if any. Conversational
/// events leave the agent `Running`; only the turn's end moves it.
fn status_for(event: &AgentEvent) -> Option<AgentStatus> {
    match event {
        AgentEvent::Result { is_error: false } => Some(AgentStatus::Done),
        AgentEvent::Result { is_error: true } => Some(AgentStatus::Failed),
        AgentEvent::Error { .. } => Some(AgentStatus::Failed),
        _ => None,
    }
}

/// Records a task's live agent status and pushes it on the `tasks` topic.
async fn publish_status(
    statuses: &TaskStatuses,
    events: &EventSink,
    task_id: &str,
    status: AgentStatus,
) {
    statuses.lock().await.insert(task_id.to_string(), status);
    events.emit(
        "tasks",
        &TaskStatusUpdate {
            task_id: task_id.to_string(),
            status,
        },
    );
}

/// Forwards every agent event as an `events.update` notification on the
/// session's topic, persists the conversational ones, records the Claude Code
/// session id, updates the task's live status, and drops the session when the
/// agent exits.
#[allow(clippy::too_many_arguments)]
fn spawn_event_pump(
    mut agent_events: mpsc::UnboundedReceiver<AgentEvent>,
    session_id: String,
    task_id: Option<String>,
    db: Db,
    registry: SessionRegistry,
    events: EventSink,
    statuses: TaskStatuses,
) {
    tokio::spawn(async move {
        while let Some(event) = agent_events.recv().await {
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

            // Record any file the agent edited, for the session's footprint.
            if let Some(path) = edited_path(&event) {
                if let Err(err) = db.record_session_file(session_id.clone(), path).await {
                    tracing::warn!(%err, "failed to record session file");
                }
            }

            // Move the task's live status on the turn's end.
            if let (Some(task_id), Some(status)) = (&task_id, status_for(&event)) {
                publish_status(&statuses, &events, task_id, status).await;
            }

            // The session id is the topic — the chat view subscribes to it.
            events.emit(&session_id, &event);
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
    use serde_json::json;

    use super::make_title;
    use crate::rpc::test_support::context;

    #[tokio::test]
    async fn list_is_empty_for_a_fresh_database() {
        let (ctx, _ws, _data) = context().await;
        let result = super::list(&ctx).await.expect("list");
        assert_eq!(result["sessions"].as_array().expect("sessions").len(), 0);
    }

    #[tokio::test]
    async fn get_rejects_an_unknown_session() {
        let (ctx, _ws, _data) = context().await;
        let err = super::get(json!({ "session_id": "ghost" }), &ctx)
            .await
            .expect_err("unknown session errors");
        assert_eq!(err.code, -32602);
    }

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

    #[test]
    fn status_for_maps_the_turn_end_events() {
        use viban_core::types::AgentStatus;
        use viban_core::AgentEvent;
        assert_eq!(
            super::status_for(&AgentEvent::Result { is_error: false }),
            Some(AgentStatus::Done),
        );
        assert_eq!(
            super::status_for(&AgentEvent::Result { is_error: true }),
            Some(AgentStatus::Failed),
        );
        assert_eq!(
            super::status_for(&AgentEvent::Error {
                message: "boom".into(),
            }),
            Some(AgentStatus::Failed),
        );
    }

    #[test]
    fn status_for_ignores_conversational_events() {
        use viban_core::AgentEvent;
        assert!(super::status_for(&AgentEvent::AssistantText { text: "hi".into() }).is_none());
        assert!(super::status_for(&AgentEvent::SessionStarted {
            session_id: "s".into(),
        })
        .is_none());
    }

    #[test]
    fn edited_path_extracts_paths_from_file_editing_tools() {
        use viban_core::AgentEvent;
        let edit = AgentEvent::ToolUse {
            name: "Edit".into(),
            input: json!({ "file_path": "src/a.rs" }),
        };
        assert_eq!(super::edited_path(&edit).as_deref(), Some("src/a.rs"));
        let write = AgentEvent::ToolUse {
            name: "Write".into(),
            input: json!({ "file_path": "README.md" }),
        };
        assert_eq!(super::edited_path(&write).as_deref(), Some("README.md"));
        let notebook = AgentEvent::ToolUse {
            name: "NotebookEdit".into(),
            input: json!({ "notebook_path": "nb.ipynb" }),
        };
        assert_eq!(super::edited_path(&notebook).as_deref(), Some("nb.ipynb"));
    }

    #[test]
    fn edited_path_ignores_read_only_tools_and_other_events() {
        use viban_core::AgentEvent;
        let read = AgentEvent::ToolUse {
            name: "Read".into(),
            input: json!({ "file_path": "src/a.rs" }),
        };
        assert!(super::edited_path(&read).is_none(), "a read is not an edit");
        assert!(super::edited_path(&AgentEvent::AssistantText { text: "hi".into() }).is_none());
    }

    #[tokio::test]
    async fn get_includes_the_sessions_edited_files() {
        use viban_core::types::Session;
        let (ctx, _ws, _data) = context().await;
        ctx.db
            .create_session(Session {
                id: "s1".into(),
                claude_session_id: None,
                title: "t".into(),
                created_at: 0,
                project_path: "/p".into(),
            })
            .await
            .expect("create session");
        ctx.db
            .record_session_file("s1".into(), "src/main.rs".into())
            .await
            .expect("record file");

        let result = super::get(json!({ "session_id": "s1" }), &ctx)
            .await
            .expect("get");
        assert_eq!(result["files"][0], "src/main.rs");
    }
}
