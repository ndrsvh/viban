import type { AgentStatus } from "@/types/board";

// Live agent-status presentation, shared by every surface that shows the dot
// (the board card, the task-list panel) so they never drift apart.

/** Tailwind classes for the live agent-status dot, by status. */
export const STATUS_DOT: Record<AgentStatus, string> = {
  running: "animate-pulse bg-amber-500",
  done: "bg-emerald-500",
  failed: "bg-red-500",
};

/** Accessible label for the live agent-status dot, by status. */
export const STATUS_LABEL: Record<AgentStatus, string> = {
  running: "Agent running",
  done: "Agent finished",
  failed: "Agent failed",
};
