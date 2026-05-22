import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";

import { Inspector } from "@/components/inspector";
import { useBoardStore } from "@/stores/useBoardStore";
import type { Task } from "@/types/board";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  // rpc.ts imports tauri's Channel at module load; the mock provides it.
  Channel: class {
    onmessage: ((message: unknown) => void) | null = null;
  },
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
    worktree_path: "/data/worktrees/a1",
    branch: "viban/add-auth-a1",
    created_at: 0,
    ...overrides,
  };
}

/** Routes get_session to a fixed snapshot. */
function sessionWith(
  usage: { input_tokens: number; output_tokens: number },
  files: string[],
) {
  invokeMock.mockImplementation((command) => {
    if (command === "get_session") {
      return Promise.resolve({ session: {}, messages: [], files, usage });
    }
    return Promise.resolve(undefined);
  });
}

beforeEach(() => {
  invokeMock.mockReset();
  useBoardStore.setState({
    columns: [],
    columnTasks: {},
    tasks: {},
    statuses: {},
  });
});

describe("Inspector", () => {
  it("shows the task's branch and worktree", () => {
    sessionWith({ input_tokens: 0, output_tokens: 0 }, []);
    render(<Inspector task={makeTask()} />);

    expect(screen.getByText("viban/add-auth-a1")).toBeInTheDocument();
    expect(screen.getByText("/data/worktrees/a1")).toBeInTheDocument();
  });

  it("shows the live agent status from the board store", () => {
    sessionWith({ input_tokens: 0, output_tokens: 0 }, []);
    useBoardStore.setState({ statuses: { t1: "running" } });
    render(<Inspector task={makeTask()} />);

    expect(screen.getByText("Agent running")).toBeInTheDocument();
  });

  it("shows token usage and the edited-file count from the session", async () => {
    sessionWith({ input_tokens: 1200, output_tokens: 345 }, ["a.rs", "b.rs"]);
    render(<Inspector task={makeTask()} />);

    expect(
      await screen.findByText("1,200 in · 345 out"),
    ).toBeInTheDocument();
    expect(screen.getByText("2 edited")).toBeInTheDocument();
  });

  it("collapses and expands", async () => {
    const user = userEvent.setup();
    sessionWith({ input_tokens: 0, output_tokens: 0 }, []);
    render(<Inspector task={makeTask()} />);

    expect(screen.getByText("viban/add-auth-a1")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Hide details" }));
    expect(
      screen.queryByText("viban/add-auth-a1"),
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Show details" }));
    expect(screen.getByText("viban/add-auth-a1")).toBeInTheDocument();
  });
});
