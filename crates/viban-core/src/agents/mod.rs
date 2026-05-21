//! Agent process layer: spawning Claude Code CLI sessions and normalizing
//! their output into `AgentEvent`s. Pure logic — no transport, no Tauri.

mod claude_code;
mod stream;

pub use claude_code::{spawn_claude, ClaudeSession};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A normalized event from a running Claude Code session.
///
/// Claude Code's stdout schema shifts between CLI versions, so only the
/// events viban acts on are classified; everything else passes through as
/// `Raw` so nothing is silently lost.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    /// The session announced itself; carries the Claude Code session id.
    SessionStarted { session_id: String },
    /// The assistant produced text.
    AssistantText { text: String },
    /// The assistant invoked a tool.
    ToolUse { name: String, input: Value },
    /// The turn finished.
    Result { is_error: bool },
    /// A fatal error in the session.
    Error { message: String },
    /// An unclassified event, passed through verbatim.
    Raw { payload: Value },
}
