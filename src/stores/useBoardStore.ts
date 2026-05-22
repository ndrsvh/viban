import { create } from "zustand";

import { rpc } from "@/lib/rpc";
import type { AgentStatus, Column, Task } from "@/types/board";

interface BoardState {
  columns: Column[];
  /** Ordered task ids per column id. */
  columnTasks: Record<string, string[]>;
  /** Every task on the board, keyed by id. */
  tasks: Record<string, Task>;
  /** Live agent status per task id. */
  statuses: Record<string, AgentStatus>;
  /** Fetches the board and replaces all board state. Never throws. */
  loadBoard: () => Promise<void>;
  /** Replaces the per-column task ordering — used for optimistic drag moves. */
  setColumnTasks: (columnTasks: Record<string, string[]>) => void;
  /** Applies a single live agent-status update. */
  setStatus: (taskId: string, status: AgentStatus) => void;
}

/**
 * The Kanban board's shared state.
 *
 * It lives in a store rather than `BoardView`'s component state so the board
 * can be updated from outside the view — the live task-status feed pushes
 * `setStatus` updates straight in — without prop-drilling or lifting state.
 */
export const useBoardStore = create<BoardState>((set) => ({
  columns: [],
  columnTasks: {},
  tasks: {},
  statuses: {},
  loadBoard: async () => {
    try {
      const result = await rpc.getBoard();
      const tasks: Record<string, Task> = {};
      const columnTasks: Record<string, string[]> = {};
      for (const column of result.columns) columnTasks[column.id] = [];
      for (const task of result.tasks) {
        tasks[task.id] = task;
        (columnTasks[task.column_id] ??= []).push(task.id);
      }
      set({ columns: result.columns, columnTasks, tasks, statuses: result.statuses });
    } catch (err) {
      console.error(err);
    }
  },
  setColumnTasks: (columnTasks) => set({ columnTasks }),
  setStatus: (taskId, status) =>
    set((state) => ({ statuses: { ...state.statuses, [taskId]: status } })),
}));
