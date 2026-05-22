import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { CommandPalette } from "@/components/command-palette";
import { useBoardStore } from "@/stores/useBoardStore";
import type { Task } from "@/types/board";

function makeTask(overrides: Partial<Task> = {}): Task {
  return {
    id: "t1",
    column_id: "c1",
    title: "Write tests",
    description: "",
    position: 0,
    session_id: "s1",
    worktree_path: null,
    branch: null,
    created_at: 0,
    ...overrides,
  };
}

/** Seeds the shared board store, the palette's only data source. */
function seed(tasks: Task[]) {
  const byId: Record<string, Task> = {};
  for (const task of tasks) byId[task.id] = task;
  useBoardStore.setState({ tasks: byId });
}

const baseProps = {
  open: true,
  onOpenChange: vi.fn(),
  onSelectTask: vi.fn(),
  onGoBoard: vi.fn(),
  onSwitchProject: vi.fn(),
};

beforeEach(() => {
  useBoardStore.setState({
    columns: [],
    columnTasks: {},
    tasks: {},
    statuses: {},
  });
});

describe("CommandPalette", () => {
  it("lists tasks when open", () => {
    seed([
      makeTask({ id: "t1", title: "Write tests" }),
      makeTask({ id: "t2", title: "Fix bug" }),
    ]);
    render(<CommandPalette {...baseProps} />);

    expect(screen.getByText("Write tests")).toBeInTheDocument();
    expect(screen.getByText("Fix bug")).toBeInTheDocument();
  });

  it("filters tasks by the query", async () => {
    const user = userEvent.setup();
    seed([
      makeTask({ id: "t1", title: "Write tests" }),
      makeTask({ id: "t2", title: "Fix bug" }),
    ]);
    render(<CommandPalette {...baseProps} />);

    await user.type(screen.getByPlaceholderText(/Search tasks/), "bug");
    expect(screen.getByText("Fix bug")).toBeInTheDocument();
    expect(screen.queryByText("Write tests")).not.toBeInTheDocument();
  });

  it("selects a task", async () => {
    const user = userEvent.setup();
    const onSelectTask = vi.fn();
    seed([makeTask({ id: "t1", title: "Write tests" })]);
    render(<CommandPalette {...baseProps} onSelectTask={onSelectTask} />);

    await user.click(screen.getByText("Write tests"));
    expect(onSelectTask).toHaveBeenCalledWith(
      expect.objectContaining({ id: "t1" }),
    );
  });

  it("runs an action from the > prefix", async () => {
    const user = userEvent.setup();
    const onGoBoard = vi.fn();
    seed([makeTask()]);
    render(<CommandPalette {...baseProps} onGoBoard={onGoBoard} />);

    await user.type(screen.getByPlaceholderText(/Search tasks/), ">board");
    // The task list gives way to the actions.
    expect(screen.queryByText("Write tests")).not.toBeInTheDocument();
    await user.click(screen.getByText("Go to board"));
    expect(onGoBoard).toHaveBeenCalled();
  });
});
