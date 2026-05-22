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
        return Promise.resolve({ session_id: "session-xyz" });
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
      initGit: false,
      withoutGit: false,
    });
  });

  it("starts a new attempt for a task with a session", async () => {
    const user = userEvent.setup();
    const onOpenSession = vi.fn();
    boardWith(
      [
        makeTask({
          session_id: "s1",
          worktree_path: "/repo/.viban/worktrees/a1",
          branch: "viban/write-tests-a1",
        }),
      ],
      (command) => {
        if (command === "create_attempt") {
          return Promise.resolve({ session_id: "attempt-2" });
        }
        return Promise.resolve();
      },
    );
    render(<BoardView onOpenSession={onOpenSession} onReview={vi.fn()} />);

    await screen.findByText("Write tests");
    await user.click(screen.getByRole("button", { name: "New attempt" }));

    await waitFor(() =>
      expect(onOpenSession).toHaveBeenCalledWith("attempt-2"),
    );
    expect(invokeMock).toHaveBeenCalledWith("create_attempt", {
      taskId: "t1",
    });
  });

  it("merges a task after confirmation", async () => {
    const user = userEvent.setup();
    let mergedTaskId: unknown;
    boardWith(
      [
        makeTask({
          session_id: "s1",
          worktree_path: "/repo/.viban/worktrees/t1",
          branch: "viban/write-tests-t1",
        }),
      ],
      (command, args) => {
        if (command === "git_merge") {
          mergedTaskId = args?.taskId;
        }
        return Promise.resolve();
      },
    );
    render(<BoardView onOpenSession={vi.fn()} onReview={vi.fn()} />);

    await screen.findByText("Write tests");
    await user.click(screen.getByRole("button", { name: "Merge" }));

    // A confirmation dialog appears instead of merging immediately.
    expect(
      await screen.findByText("Merge this task?"),
    ).toBeInTheDocument();
    expect(mergedTaskId).toBeUndefined();

    await user.click(screen.getByRole("button", { name: "Merge branch" }));
    await waitFor(() => expect(mergedTaskId).toBe("t1"));
  });

  it("confirms git initialization when the folder is not a repo", async () => {
    const user = userEvent.setup();
    const onOpenSession = vi.fn();
    let confirmedWithInit = false;
    boardWith([makeTask()], (command, args) => {
      if (command === "start_session") {
        if (args?.initGit === true) {
          confirmedWithInit = true;
          return Promise.resolve({ session_id: "session-after-init" });
        }
        return Promise.resolve({ needs_git_init: true });
      }
      return Promise.resolve();
    });
    render(<BoardView onOpenSession={onOpenSession} onReview={vi.fn()} />);

    await screen.findByText("Write tests");
    await user.click(screen.getByRole("button", { name: "Start session" }));

    // The confirmation dialog appears instead of starting immediately.
    expect(
      await screen.findByText("Set up git for this project?"),
    ).toBeInTheDocument();
    expect(onOpenSession).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "Initialize git" }));

    await waitFor(() =>
      expect(onOpenSession).toHaveBeenCalledWith("session-after-init"),
    );
    expect(confirmedWithInit).toBe(true);
  });

  it("can start a session without git from the dialog", async () => {
    const user = userEvent.setup();
    const onOpenSession = vi.fn();
    let startedWithoutGit = false;
    boardWith([makeTask()], (command, args) => {
      if (command === "start_session") {
        if (args?.withoutGit === true) {
          startedWithoutGit = true;
          return Promise.resolve({ session_id: "session-no-git" });
        }
        return Promise.resolve({ needs_git_init: true });
      }
      return Promise.resolve();
    });
    render(<BoardView onOpenSession={onOpenSession} onReview={vi.fn()} />);

    await screen.findByText("Write tests");
    await user.click(screen.getByRole("button", { name: "Start session" }));

    await screen.findByText("Set up git for this project?");
    await user.click(
      screen.getByRole("button", { name: "Work without Git" }),
    );

    await waitFor(() =>
      expect(onOpenSession).toHaveBeenCalledWith("session-no-git"),
    );
    expect(startedWithoutGit).toBe(true);
  });

  it("surfaces a dismissible banner when starting a session fails", async () => {
    const user = userEvent.setup();
    vi.spyOn(console, "error").mockImplementation(() => {});
    boardWith([makeTask()], (command) => {
      if (command === "start_session") {
        return Promise.reject("worktree creation blew up");
      }
      return Promise.resolve();
    });
    render(<BoardView onOpenSession={vi.fn()} onReview={vi.fn()} />);

    await screen.findByText("Write tests");
    await user.click(screen.getByRole("button", { name: "Start session" }));

    // The failure is shown instead of being swallowed.
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("worktree creation blew up");

    // And the banner can be dismissed.
    await user.click(screen.getByRole("button", { name: "Dismiss error" }));
    await waitFor(() =>
      expect(screen.queryByRole("alert")).not.toBeInTheDocument(),
    );
  });
});
