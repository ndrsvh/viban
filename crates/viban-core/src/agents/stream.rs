//! Parses Claude Code's NDJSON stdout into `AgentEvent`s.

use serde_json::Value;

use super::AgentEvent;
use crate::types::TokenUsage;

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
            usage: parse_usage(&value),
        },
        _ => AgentEvent::Raw { payload: value },
    }
}

/// Extracts token counts from a `result` event's `usage` object. Best-effort:
/// returns `None` when there is no `usage`, and zeroes a missing field.
fn parse_usage(value: &Value) -> Option<TokenUsage> {
    let usage = value.get("usage")?;
    Some(TokenUsage {
        input_tokens: usage
            .get("input_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        output_tokens: usage
            .get("output_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0),
    })
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
            AgentEvent::Result { is_error, .. } => assert!(!is_error),
            other => panic!("expected Result, got {other:?}"),
        }
    }

    #[test]
    fn invalid_json_becomes_an_error_event() {
        match parse_line("this is not json") {
            AgentEvent::Error { message } => {
                assert!(message.contains("invalid agent output"), "got: {message}");
            }
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn assistant_tool_use_block_is_classified() {
        let line = r#"{"type":"assistant","message":{"content":[
            {"type":"tool_use","name":"Read","input":{"file":"a.txt"}}]}}"#;
        match parse_line(line) {
            AgentEvent::ToolUse { name, input } => {
                assert_eq!(name, "Read");
                assert_eq!(input["file"], "a.txt");
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }

    #[test]
    fn assistant_concatenates_multiple_text_blocks() {
        let line = r#"{"type":"assistant","message":{"content":[
            {"type":"text","text":"foo "},{"type":"text","text":"bar"}]}}"#;
        match parse_line(line) {
            AgentEvent::AssistantText { text } => assert_eq!(text, "foo bar"),
            other => panic!("expected AssistantText, got {other:?}"),
        }
    }

    #[test]
    fn assistant_without_content_is_raw() {
        match parse_line(r#"{"type":"assistant","message":{}}"#) {
            AgentEvent::Raw { .. } => {}
            other => panic!("expected Raw, got {other:?}"),
        }
    }

    #[test]
    fn system_without_init_subtype_is_raw() {
        match parse_line(r#"{"type":"system","subtype":"other"}"#) {
            AgentEvent::Raw { .. } => {}
            other => panic!("expected Raw, got {other:?}"),
        }
    }

    #[test]
    fn init_without_session_id_is_raw() {
        match parse_line(r#"{"type":"system","subtype":"init"}"#) {
            AgentEvent::Raw { .. } => {}
            other => panic!("expected Raw, got {other:?}"),
        }
    }

    #[test]
    fn result_defaults_is_error_to_false_when_absent() {
        match parse_line(r#"{"type":"result"}"#) {
            AgentEvent::Result { is_error, .. } => assert!(!is_error),
            other => panic!("expected Result, got {other:?}"),
        }
    }

    #[test]
    fn result_captures_token_usage() {
        let line = r#"{"type":"result","is_error":false,
            "usage":{"input_tokens":1200,"output_tokens":340}}"#;
        match parse_line(line) {
            AgentEvent::Result {
                usage: Some(usage), ..
            } => {
                assert_eq!(usage.input_tokens, 1200);
                assert_eq!(usage.output_tokens, 340);
            }
            other => panic!("expected Result with usage, got {other:?}"),
        }
    }

    #[test]
    fn result_without_usage_has_none() {
        match parse_line(r#"{"type":"result","is_error":false}"#) {
            AgentEvent::Result { usage, .. } => assert!(usage.is_none()),
            other => panic!("expected Result, got {other:?}"),
        }
    }

    #[test]
    fn result_reports_a_true_error_flag() {
        match parse_line(r#"{"type":"result","is_error":true}"#) {
            AgentEvent::Result { is_error, .. } => assert!(is_error),
            other => panic!("expected Result, got {other:?}"),
        }
    }

    #[test]
    fn json_without_a_type_field_is_raw() {
        match parse_line(r#"{"hello":"world"}"#) {
            AgentEvent::Raw { payload } => assert_eq!(payload["hello"], "world"),
            other => panic!("expected Raw, got {other:?}"),
        }
    }
}
