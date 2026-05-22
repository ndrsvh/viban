import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { TaskListPanel } from "@/components/task-list-panel";
import { useBoardStore } from "@/stores/useBoardStore";
import type { Column, Task } from "@/types/board";

const columns: Column[] = [
  { id: "c1", board_id: "b1", name: "Backlog", position: 0 },
  { id: "c2", board_id: "b1", name: "In Progress", position: 1 },
];

function makeTask(overrides: Partial<Task> = {}): Task {
  return {
    id: "t1",
    column_id: "c1",
    title: "Write tests",
    description: "",
    position: 0,
    session_id: null,
    worktree_path: null,
    branch: null,
    created_at: 0,
    ...overrides,
  };
}

/** Seeds the shared board store, the panel's only data source. */
function seed(tasks: Task[]) {
  const byId: Record<string, Task> = {};
  const columnTasks: Record<string, string[]> = { c1: [], c2: [] };
  for (const task of tasks) {
    byId[task.id] = task;
    columnTasks[task.column_id].push(task.id);
  }
  useBoardStore.setState({ columns, columnTasks, tasks: byId, statuses: {} });
}

const baseProps = {
  activeSessionId: null,
  reviewTaskId: null,
  onOpenSession: vi.fn(),
};

beforeEach(() => {
  // The board store is a module singleton — clear it so tests don't leak.
  useBoardStore.setState({
    columns: [],
    columnTasks: {},
    tasks: {},
    statuses: {},
  });
});

describe("TaskListPanel", () => {
  it("shows the empty state when there are no tasks", () => {
    render(<TaskListPanel {...baseProps} />);
    expect(screen.getByText("No tasks yet.")).toBeInTheDocument();
  });

  it("lists tasks grouped by their column", () => {
    seed([
      makeTask({ id: "t1", column_id: "c1", title: "Backlog task" }),
      makeTask({
        id: "t2",
        column_id: "c2",
        title: "Active task",
        session_id: "s2",
      }),
    ]);
    render(<TaskListPanel {...baseProps} />);

    expect(screen.getByText("Backlog")).toBeInTheDocument();
    expect(screen.getByText("In Progress")).toBeInTheDocument();
    expect(screen.getByText("Backlog task")).toBeInTheDocument();
    expect(screen.getByText("Active task")).toBeInTheDocument();
  });

  it("opens the chat when a task with a session is clicked", async () => {
    const user = userEvent.setup();
    const onOpenSession = vi.fn();
    seed([
      makeTask({
        id: "t2",
        column_id: "c2",
        title: "Active task",
        session_id: "s2",
      }),
    ]);
    render(<TaskListPanel {...baseProps} onOpenSession={onOpenSession} />);

    await user.click(screen.getByRole("button", { name: "Active task" }));
    expect(onOpenSession).toHaveBeenCalledWith("s2");
  });

  it("shows a task without a session but does not make it a button", () => {
    seed([makeTask({ id: "t1", title: "Idle task" })]);
    render(<TaskListPanel {...baseProps} />);

    expect(screen.getByText("Idle task")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Idle task" }),
    ).not.toBeInTheDocument();
  });

  it("marks the open task's row as current", () => {
    seed([
      makeTask({
        id: "t2",
        column_id: "c2",
        title: "Active task",
        session_id: "s2",
      }),
    ]);
    render(<TaskListPanel {...baseProps} activeSessionId="s2" />);

    expect(
      screen.getByRole("button", { name: "Active task" }),
    ).toHaveAttribute("aria-current", "page");
  });

  it("collapses and expands the panel", async () => {
    const user = userEvent.setup();
    seed([makeTask({ id: "t1", title: "Idle task" })]);
    render(<TaskListPanel {...baseProps} />);

    expect(screen.getByText("Idle task")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Hide tasks" }));
    expect(screen.queryByText("Idle task")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Show tasks" }));
    expect(screen.getByText("Idle task")).toBeInTheDocument();
  });
});
