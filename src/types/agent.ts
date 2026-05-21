/** Mirror of `viban_core::AgentEvent` — a normalized Claude Code session event. */
export type AgentEvent =
  | { type: "session_started"; session_id: string }
  | { type: "assistant_text"; text: string }
  | { type: "tool_use"; name: string; input: unknown }
  | { type: "result"; is_error: boolean }
  | { type: "error"; message: string }
  | { type: "raw"; payload: unknown };
