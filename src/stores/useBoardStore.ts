import { create } from "zustand";

import { rpc } from "@/lib/rpc";
import type { Column, Task } from "@/types/board";

interface BoardState {
  columns: Column[];
  /** Ordered task ids per column id. */
  columnTasks: Record<string, string[]>;
  /** Every task on the board, keyed by id. */
  tasks: Record<string, Task>;
  /** Fetches the board and replaces all board state. Never throws. */
  loadBoard: () => Promise<void>;
  /** Replaces the per-column task ordering — used for optimistic drag moves. */
  setColumnTasks: (columnTasks: Record<string, string[]>) => void;
}

/**
 * The Kanban board's shared state.
 *
 * It lives in a store rather than `BoardView`'s component state so the board
 * can be updated from outside the view — e.g. a live task-status feed pushing
 * changes — without prop-drilling or lifting state into `App`.
 */
export const useBoardStore = create<BoardState>((set) => ({
  columns: [],
  columnTasks: {},
  tasks: {},
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
      set({ columns: result.columns, columnTasks, tasks });
    } catch (err) {
      console.error(err);
    }
  },
  setColumnTasks: (columnTasks) => set({ columnTasks }),
}));
