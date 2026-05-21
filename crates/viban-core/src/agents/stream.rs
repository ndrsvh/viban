//! Parses Claude Code's NDJSON stdout into `AgentEvent`s.

use serde_json::Value;

use super::AgentEvent;

/// Classifies one line of Claude Code stdout. Unrecognized shapes pass
/// through as `AgentEvent::Raw`.
pub fn parse_line(line: &str) -> AgentEvent {
    let value: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(err) => {
            return AgentEvent::Error {
                message: format!("invalid agent output: {err}"),
            }
        }
    };

    match value.get("type").and_then(Value::as_str) {
        Some("system") => parse_system(value),
        Some("assistant") => parse_assistant(value),
        Some("result") => AgentEvent::Result {
            is_error: value
                .get("is_error")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        _ => AgentEvent::Raw { payload: value },
    }
}

fn parse_system(value: Value) -> AgentEvent {
    if value.get("subtype").and_then(Value::as_str) == Some("init") {
        if let Some(session_id) = value.get("session_id").and_then(Value::as_str) {
            return AgentEvent::SessionStarted {
                session_id: session_id.to_string(),
            };
        }
    }
    AgentEvent::Raw { payload: value }
}

fn parse_assistant(value: Value) -> AgentEvent {
    let Some(content) = value
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_array)
    else {
        return AgentEvent::Raw { payload: value };
    };

    let mut text = String::new();
    for block in content {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(chunk) = block.get("text").and_then(Value::as_str) {
                    text.push_str(chunk);
                }
            }
            Some("tool_use") if text.is_empty() => {
                return AgentEvent::ToolUse {
                    name: block
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    input: block.get("input").cloned().unwrap_or(Value::Null),
                };
            }
            _ => {}
        }
    }

    if text.is_empty() {
        AgentEvent::Raw { payload: value }
    } else {
        AgentEvent::AssistantText { text }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_init() {
        let line = r#"{"type":"system","subtype":"init","session_id":"abc-123"}"#;
        match parse_line(line) {
            AgentEvent::SessionStarted { session_id } => assert_eq!(session_id, "abc-123"),
            other => panic!("expected SessionStarted, got {other:?}"),
        }
    }

    #[test]
    fn extracts_assistant_text() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hi"}]}}"#;
        match parse_line(line) {
            AgentEvent::AssistantText { text } => assert_eq!(text, "hi"),
            other => panic!("expected AssistantText, got {other:?}"),
        }
    }

    #[test]
    fn unknown_type_is_raw() {
        match parse_line(r#"{"type":"rate_limit_event"}"#) {
            AgentEvent::Raw { .. } => {}
            other => panic!("expected Raw, got {other:?}"),
        }
    }

    #[test]
    fn result_carries_error_flag() {
        match parse_line(r#"{"type":"result","subtype":"success","is_error":false}"#) {
            AgentEvent::Result { is_error } => assert!(!is_error),
            other => panic!("expected Result, got {other:?}"),
        }
    }
}
