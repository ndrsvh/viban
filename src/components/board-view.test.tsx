import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";

import { BoardView } from "@/components/board-view";
import type { Column, Task } from "@/types/board";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const invokeMock = vi.mocked(invoke);

type InvokeImpl = (
  command: string,
  args?: Record<string, unknown>,
) => Promise<unknown>;

function setInvoke(impl: InvokeImpl) {
  invokeMock.mockImplementation(impl as unknown as typeof invoke);
}

const columns: Column[] = [
  { id: "col-backlog", board_id: "b1", name: "Backlog", position: 0 },
  { id: "col-done", board_id: "b1", name: "Done", position: 1 },
];

function makeTask(overrides: Partial<Task> = {}): Task {
  return {
    id: "t1",
    column_id: "col-backlog",
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

/** Routes `get_board` to a fixed board; everything else resolves to `extra`. */
function boardWith(tasks: Task[], extra: InvokeImpl = () => Promise.resolve()) {
  setInvoke((command, args) => {
    if (command === "get_board") {
      return Promise.resolve({ columns, tasks });
    }
    return extra(command, args);
  });
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe("BoardView", () => {
  it("loads the board and renders columns with their tasks", async () => {
    boardWith([makeTask()]);
    render(<BoardView onOpenSession={vi.fn()} onReview={vi.fn()} />);

    expect(await screen.findByText("Backlog")).toBeInTheDocument();
    expect(screen.getByText("Done")).toBeInTheDocument();
    expect(screen.getByText("Write tests")).toBeInTheDocument();
  });

  it("shows a placeholder in an empty column", async () => {
    boardWith([makeTask()]);
    render(<BoardView onOpenSession={vi.fn()} onReview={vi.fn()} />);

    await screen.findByText("Backlog");
    // The Done column has no tasks.
    expect(screen.getByText("Drop tasks here")).toBeInTheDocument();
  });

  it("opens the new-task dialog from a column", async () => {
    const user = userEvent.setup();
    boardWith([]);
    render(<BoardView onOpenSession={vi.fn()} onReview={vi.fn()} />);

    await screen.findByText("Backlog");
    const addButtons = screen.getAllByRole("button", { name: "+ Add task" });
    await user.click(addButtons[0]);

    expect(await screen.findByText("New task")).toBeInTheDocument();
  });

  it("starts a session for a task and opens its chat", async () => {
    const user = userEvent.setup();
    const onOpenSession = vi.fn();
    boardWith([makeTask()], (command) => {
      if (command === "start_session") {
        return Promise.resolve("session-xyz");
      }
      return Promise.resolve();
    });
    render(<BoardView onOpenSession={onOpenSession} onReview={vi.fn()} />);

    await screen.findByText("Write tests");
    await user.click(screen.getByRole("button", { name: "Start session" }));

    await waitFor(() =>
      expect(onOpenSession).toHaveBeenCalledWith("session-xyz"),
    );
    expect(invokeMock).toHaveBeenCalledWith("start_session", {
      taskId: "t1",
    });
  });
});
