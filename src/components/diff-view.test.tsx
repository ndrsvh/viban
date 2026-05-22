import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";

import { DiffView } from "@/components/diff-view";
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

const sampleFiles: FileDiff[] = [
  { path: "src/a.txt", status: "modified", old_text: "old", new_text: "new" },
  { path: "src/b.txt", status: "added", old_text: "", new_text: "added" },
];

beforeEach(() => {
  invokeMock.mockReset();
});

describe("DiffView", () => {
  it("loads and lists the changed files", async () => {
    setInvoke((command) => {
      if (command === "git_diff") return Promise.resolve({ files: sampleFiles });
      return Promise.resolve(undefined);
    });

    render(<DiffView taskId="t1" taskTitle="My task" onDone={vi.fn()} />);

    expect(await screen.findByText("src/a.txt")).toBeInTheDocument();
    expect(screen.getByText("src/b.txt")).toBeInTheDocument();
    expect(screen.getByText("Review: My task")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("git_diff", { taskId: "t1" });
  });

  it("commits the changes when Accept all is clicked", async () => {
    const user = userEvent.setup();
    const onDone = vi.fn();
    setInvoke((command) => {
      if (command === "git_diff") return Promise.resolve({ files: sampleFiles });
      return Promise.resolve(undefined);
    });

    render(<DiffView taskId="t1" taskTitle="My task" onDone={onDone} />);
    await screen.findByText("src/a.txt");
    await user.click(screen.getByRole("button", { name: "Accept all" }));

    await waitFor(() => expect(onDone).toHaveBeenCalled());
    expect(invokeMock).toHaveBeenCalledWith("git_commit", { taskId: "t1" });
  });

  it("discards the changes when Reject all is clicked", async () => {
    const user = userEvent.setup();
    const onDone = vi.fn();
    setInvoke((command) => {
      if (command === "git_diff") return Promise.resolve({ files: sampleFiles });
      return Promise.resolve(undefined);
    });

    render(<DiffView taskId="t1" taskTitle="My task" onDone={onDone} />);
    await screen.findByText("src/a.txt");
    await user.click(screen.getByRole("button", { name: "Reject all" }));

    await waitFor(() => expect(onDone).toHaveBeenCalled());
    expect(invokeMock).toHaveBeenCalledWith("git_restore", { taskId: "t1" });
  });

  it("reports an empty worktree and disables Accept all", async () => {
    setInvoke((command) => {
      if (command === "git_diff") return Promise.resolve({ files: [] });
      return Promise.resolve(undefined);
    });

    render(<DiffView taskId="t1" taskTitle="My task" onDone={vi.fn()} />);

    expect(await screen.findByText("No changes.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Accept all" })).toBeDisabled();
  });

  it("returns to the board via the back button", async () => {
    const user = userEvent.setup();
    const onDone = vi.fn();
    setInvoke((command) => {
      if (command === "git_diff") return Promise.resolve({ files: sampleFiles });
      return Promise.resolve(undefined);
    });

    render(<DiffView taskId="t1" taskTitle="My task" onDone={onDone} />);
    await screen.findByText("src/a.txt");
    await user.click(screen.getByRole("button", { name: "← Board" }));

    expect(onDone).toHaveBeenCalled();
  });
});
