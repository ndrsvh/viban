import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";

import { TaskDetail } from "@/components/task-detail";
import type { Task } from "@/types/board";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  // rpc.ts imports tauri's Channel at module load; the mock provides it.
  Channel: class {
    onmessage: ((message: unknown) => void) | null = null;
  },
}));

// The tab contents have their own tests — stub them so these tests cover
// only the detail's own behaviour: the breadcrumb and the tab switching.
vi.mock("@/components/chat-view", () => ({
  ChatView: () => <div>chat view</div>,
}));
vi.mock("@/components/diff-view", () => ({
  DiffView: () => <div>diff view</div>,
}));
vi.mock("@/components/run-panel", () => ({
  RunPanel: () => <div>run panel</div>,
}));

const invokeMock = vi.mocked(invoke);

function makeTask(overrides: Partial<Task> = {}): Task {
  return {
    id: "t1",
    column_id: "c1",
    title: "Add auth",
    description: "",
    position: 0,
    session_id: "s1",
    worktree_path: "/wt",
    branch: "viban/x",
    created_at: 0,
    ...overrides,
  };
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe("TaskDetail", () => {
  it("shows the task title in the breadcrumb", () => {
    render(<TaskDetail task={makeTask()} onClose={vi.fn()} />);
    expect(screen.getByText("Add auth")).toBeInTheDocument();
  });

  it("returns to the board via the breadcrumb", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    render(<TaskDetail task={makeTask()} onClose={onClose} />);

    await user.click(screen.getByRole("button", { name: "Board" }));
    expect(onClose).toHaveBeenCalled();
  });

  it("opens the Chat tab first and leaves other tabs unmounted", () => {
    render(<TaskDetail task={makeTask()} onClose={vi.fn()} />);

    expect(screen.getByText("chat view")).toBeInTheDocument();
    expect(screen.queryByText("diff view")).not.toBeInTheDocument();
    expect(screen.queryByText("run panel")).not.toBeInTheDocument();
  });

  it("honors the initial tab", () => {
    render(
      <TaskDetail task={makeTask()} initialTab="diff" onClose={vi.fn()} />,
    );
    expect(screen.getByText("diff view")).toBeInTheDocument();
  });

  it("switches tabs and keeps visited tabs mounted", async () => {
    const user = userEvent.setup();
    render(<TaskDetail task={makeTask()} onClose={vi.fn()} />);

    await user.click(screen.getByRole("tab", { name: "Diff" }));
    expect(screen.getByText("diff view")).toBeInTheDocument();
    // The chat tab stays mounted so a live stream is never dropped.
    expect(screen.getByText("chat view")).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: "Run" }));
    expect(screen.getByText("run panel")).toBeInTheDocument();
  });

  it("lists the session's edited files on the Files tab", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation((command) => {
      if (command === "get_session") {
        return Promise.resolve({
          session: {},
          messages: [],
          files: ["src/auth.rs", "src/lib.rs"],
          usage: { input_tokens: 0, output_tokens: 0 },
        });
      }
      return Promise.resolve(undefined);
    });

    render(<TaskDetail task={makeTask()} onClose={vi.fn()} />);
    await user.click(screen.getByRole("tab", { name: "Files" }));

    expect(await screen.findByText("src/auth.rs")).toBeInTheDocument();
    expect(screen.getByText("src/lib.rs")).toBeInTheDocument();
  });
});
