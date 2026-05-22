//! Agent process layer: spawning Claude Code CLI sessions and normalizing
//! their output into `AgentEvent`s. Pure logic — no transport, no Tauri.

mod claude_code;
mod stream;

pub use claude_code::{generate_commit_message, spawn_claude, ClaudeSession};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// A normalized event from a running Claude Code session.
///
/// Claude Code's stdout schema shifts between CLI versions, so only the
/// events viban acts on are classified; everything else passes through as
/// `Raw` so nothing is silently lost.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src/types/generated/")]
pub enum AgentEvent {
    /// The session announced itself; carries the Claude Code session id.
    SessionStarted { session_id: String },
    /// The assistant produced text.
    AssistantText { text: String },
    /// The assistant invoked a tool.
    ToolUse {
        name: String,
        /// Free-form tool input — opaque to viban.
        #[ts(type = "unknown")]
        input: Value,
    },
    /// The turn finished.
    Result { is_error: bool },
    /// A fatal error in the session.
    Error { message: String },
    /// An unclassified event, passed through verbatim.
    Raw {
        /// The original event JSON.
        #[ts(type = "unknown")]
        payload: Value,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn session_started_serializes_with_a_snake_case_tag() {
        let event = AgentEvent::SessionStarted {
            session_id: "abc".into(),
        };
        let value = serde_json::to_value(&event).expect("to_value");
        assert_eq!(value["type"], "session_started");
        assert_eq!(value["session_id"], "abc");
    }

    #[test]
    fn every_variant_carries_the_expected_type_tag() {
        let cases: [(AgentEvent, &str); 5] = [
            (
                AgentEvent::AssistantText { text: "x".into() },
                "assistant_text",
            ),
            (
                AgentEvent::ToolUse {
                    name: "Read".into(),
                    input: json!({}),
                },
                "tool_use",
            ),
            (AgentEvent::Result { is_error: true }, "result"),
            (
                AgentEvent::Error {
                    message: "boom".into(),
                },
                "error",
            ),
            (
                AgentEvent::Raw {
                    payload: json!({ "k": 1 }),
                },
                "raw",
            ),
        ];
        for (event, tag) in cases {
            let value = serde_json::to_value(&event).expect("to_value");
            assert_eq!(value["type"], tag, "wrong tag for {event:?}");
        }
    }

    #[test]
    fn tool_use_round_trips_through_json() {
        let event = AgentEvent::ToolUse {
            name: "Edit".into(),
            input: json!({ "path": "a.txt" }),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        match serde_json::from_str::<AgentEvent>(&json).expect("deserialize") {
            AgentEvent::ToolUse { name, input } => {
                assert_eq!(name, "Edit");
                assert_eq!(input["path"], "a.txt");
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }
}
