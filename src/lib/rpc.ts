// Typed wrapper over the Tauri command surface. Every server call goes
// through here so components never pass bare command-name strings to
// `invoke`, and each call carries a checked signature and return type.

import { Channel, invoke } from "@tauri-apps/api/core";

import type { AgentEvent } from "@/types/agent";
import type {
  AgentStatus,
  Attempt,
  Board,
  Column,
  Task,
  TaskStatusUpdate,
} from "@/types/board";
import type { FileDiff } from "@/types/diff";
import type { ServerHealth } from "@/types/server";
import type { Message, Session } from "@/types/session";

/** Result of `start_session` / `create_attempt`. */
export interface StartSessionResult {
  session_id?: string;
  needs_git_init?: boolean;
}

/** The board payload returned by `get_board`. */
export interface BoardSnapshot {
  board: Board;
  columns: Column[];
  tasks: Task[];
  /** Live agent status per task id. */
  statuses: Record<string, AgentStatus>;
}

export const rpc = {
  // -- project / server -----------------------------------------------------
  currentProject: () => invoke<string | null>("current_project"),
  openProject: () => invoke<string | null>("open_project"),
  serverHealth: () => invoke<ServerHealth>("server_health"),

  // -- sessions / agents ----------------------------------------------------
  openSession: (sessionId: string, onEvent: Channel<AgentEvent>) =>
    invoke<null>("open_session", { sessionId, onEvent }),
  closeSession: (sessionId: string) =>
    invoke<null>("close_session", { sessionId }),
  watchTaskStatus: (onEvent: Channel<TaskStatusUpdate>) =>
    invoke<null>("watch_task_status", { onEvent }),
  unwatchTaskStatus: () => invoke<null>("unwatch_task_status"),
  spawnSession: (sessionId: string, prompt: string) =>
    invoke<null>("spawn_session", { sessionId, prompt }),
  sendMessage: (sessionId: string, prompt: string) =>
    invoke<null>("send_message", { sessionId, prompt }),
  listSessions: () => invoke<{ sessions: Session[] }>("list_sessions"),
  getSession: (sessionId: string) =>
    invoke<{ session: Session; messages: Message[]; files: string[] }>(
      "get_session",
      { sessionId },
    ),

  // -- board / tasks --------------------------------------------------------
  getBoard: () => invoke<BoardSnapshot>("get_board"),
  createTask: (columnId: string, title: string, description: string) =>
    invoke<{ task: Task }>("create_task", { columnId, title, description }),
  updateTask: (input: {
    taskId: string;
    title?: string;
    description?: string;
    sessionId?: string;
  }) => invoke<{ task: Task }>("update_task", input),
  deleteTask: (taskId: string) => invoke<null>("delete_task", { taskId }),
  reorderTasks: (columnId: string, taskIds: string[]) =>
    invoke<null>("reorder_tasks", { columnId, taskIds }),

  // -- task sessions / attempts --------------------------------------------
  startSession: (
    taskId: string,
    options: { initGit?: boolean; withoutGit?: boolean } = {},
  ) =>
    invoke<StartSessionResult>("start_session", {
      taskId,
      initGit: options.initGit ?? false,
      withoutGit: options.withoutGit ?? false,
    }),
  createAttempt: (taskId: string) =>
    invoke<StartSessionResult>("create_attempt", { taskId }),
  listAttempts: (taskId: string) =>
    invoke<{ attempts: Attempt[] }>("list_attempts", { taskId }),
  activateAttempt: (attemptId: string) =>
    invoke<null>("activate_attempt", { attemptId }),

  // -- git review -----------------------------------------------------------
  gitDiff: (taskId: string) =>
    invoke<{ files: FileDiff[] }>("git_diff", { taskId }),
  gitCommit: (taskId: string) => invoke<null>("git_commit", { taskId }),
  gitRestore: (taskId: string) => invoke<null>("git_restore", { taskId }),
  gitMerge: (taskId: string) => invoke<null>("git_merge", { taskId }),
};
