import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";

import { DiffView } from "@/components/diff-view";
import type { Attempt, Task } from "@/types/board";
import type { FileDiff } from "@/types/diff";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

// CodeMirror's MergeView needs real layout; stub it so tests stay in jsdom.
vi.mock("@codemirror/merge", () => ({
  MergeView: class {
    destroy() {}
  },
}));

const invokeMock = vi.mocked(invoke);

type InvokeImpl = (
  command: string,
  args?: Record<string, unknown>,
) => Promise<unknown>;

function setInvoke(impl: InvokeImpl) {
  invokeMock.mockImplementation(impl as unknown as typeof invoke);
}

function makeTask(overrides: Partial<Task> = {}): Task {
  return {
    id: "t1",
    column_id: "c1",
    title: "My task",
    description: "",
    position: 0,
    session_id: "sess-1",
    worktree_path: "/wt",
    branch: "viban/x",
    created_at: 0,
    ...overrides,
  };
}

const sampleFiles: FileDiff[] = [
  { path: "src/a.txt", status: "modified", old_text: "old", new_text: "new" },
  { path: "src/b.txt", status: "added", old_text: "", new_text: "added" },
];

/** Routes git_diff and list_attempts; everything else resolves undefined. */
function stub(files: FileDiff[], attempts: Attempt[] = []) {
  setInvoke((command) => {
    if (command === "git_diff") return Promise.resolve({ files });
    if (command === "list_attempts") return Promise.resolve({ attempts });
    return Promise.resolve(undefined);
  });
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe("DiffView", () => {
  it("loads and lists the changed files", async () => {
    stub(sampleFiles);
    render(<DiffView task={makeTask()} onDone={vi.fn()} />);

    expect(await screen.findByText("src/a.txt")).toBeInTheDocument();
    expect(screen.getByText("src/b.txt")).toBeInTheDocument();
    expect(screen.getByText("Review: My task")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("git_diff", { taskId: "t1" });
  });

  it("commits the changes when Accept all is clicked", async () => {
    const user = userEvent.setup();
    const onDone = vi.fn();
    stub(sampleFiles);

    render(<DiffView task={makeTask()} onDone={onDone} />);
    await screen.findByText("src/a.txt");
    await user.click(screen.getByRole("button", { name: "Accept all" }));

    await waitFor(() => expect(onDone).toHaveBeenCalled());
    expect(invokeMock).toHaveBeenCalledWith("git_commit", { taskId: "t1" });
  });

  it("discards the changes when Reject all is clicked", async () => {
    const user = userEvent.setup();
    const onDone = vi.fn();
    stub(sampleFiles);

    render(<DiffView task={makeTask()} onDone={onDone} />);
    await screen.findByText("src/a.txt");
    await user.click(screen.getByRole("button", { name: "Reject all" }));

    await waitFor(() => expect(onDone).toHaveBeenCalled());
    expect(invokeMock).toHaveBeenCalledWith("git_restore", { taskId: "t1" });
  });

  it("reports an empty worktree and disables Accept all", async () => {
    stub([]);
    render(<DiffView task={makeTask()} onDone={vi.fn()} />);

    expect(await screen.findByText("No changes.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Accept all" })).toBeDisabled();
  });

  it("returns to the board via the back button", async () => {
    const user = userEvent.setup();
    const onDone = vi.fn();
    stub(sampleFiles);

    render(<DiffView task={makeTask()} onDone={onDone} />);
    await screen.findByText("src/a.txt");
    await user.click(screen.getByRole("button", { name: "← Board" }));

    expect(onDone).toHaveBeenCalled();
  });

  it("switches the reviewed attempt", async () => {
    const user = userEvent.setup();
    const attempts: Attempt[] = [
      {
        id: "a2",
        task_id: "t1",
        session_id: "sess-2",
        worktree_path: "/wt2",
        branch: "viban/x-a2",
        created_at: 20,
      },
      {
        id: "a1",
        task_id: "t1",
        session_id: "sess-1",
        worktree_path: "/wt1",
        branch: "viban/x-a1",
        created_at: 10,
      },
    ];
    let activated: unknown;
    setInvoke((command, args) => {
      if (command === "git_diff") {
        return Promise.resolve({ files: sampleFiles });
      }
      if (command === "list_attempts") {
        return Promise.resolve({ attempts });
      }
      if (command === "activate_attempt") {
        activated = args?.attemptId;
      }
      return Promise.resolve(undefined);
    });

    render(<DiffView task={makeTask()} onDone={vi.fn()} />);
    await screen.findByText("src/a.txt");

    const selector = await screen.findByRole("combobox", { name: "Attempt" });
    await user.selectOptions(selector, "a2");
    await waitFor(() => expect(activated).toBe("a2"));
  });
});
