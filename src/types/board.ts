/** Mirror of `viban_core::types::Board`. */
export interface Board {
  id: string;
  name: string;
  project_path: string;
  created_at: number;
}

/** Mirror of `viban_core::types::Column`. */
export interface Column {
  id: string;
  board_id: string;
  name: string;
  position: number;
}

/** Mirror of `viban_core::types::Task`. */
export interface Task {
  id: string;
  column_id: string;
  title: string;
  description: string;
  position: number;
  session_id: string | null;
  worktree_path: string | null;
  branch: string | null;
  created_at: number;
}

/** Mirror of `viban_core::types::Attempt` — one agent run of a task. */
export interface Attempt {
  id: string;
  task_id: string;
  session_id: string | null;
  worktree_path: string | null;
  branch: string | null;
  created_at: number;
}
