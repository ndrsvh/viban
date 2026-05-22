import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";

import App from "@/App";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: ((message: unknown) => void) | null = null;
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

beforeEach(() => {
  invokeMock.mockReset();
});

describe("App", () => {
  it("shows the open-project screen when no project is set", async () => {
    setInvoke((command) => {
      if (command === "current_project") return Promise.resolve(null);
      return Promise.resolve(undefined);
    });

    render(<App />);

    expect(
      await screen.findByText("Open a git repository to start."),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Open project…" }),
    ).toBeInTheDocument();
  });

  it("invokes open_project when the open button is clicked", async () => {
    const user = userEvent.setup();
    setInvoke((command) => {
      if (command === "current_project") return Promise.resolve(null);
      // The user cancels the native dialog.
      if (command === "open_project") return Promise.resolve(null);
      return Promise.resolve(undefined);
    });

    render(<App />);
    await user.click(
      await screen.findByRole("button", { name: "Open project…" }),
    );

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("open_project"),
    );
  });

  it("surfaces the reason when opening a project fails", async () => {
    const user = userEvent.setup();
    setInvoke((command) => {
      if (command === "current_project") return Promise.resolve(null);
      if (command === "open_project") {
        return Promise.reject("the selected folder is not a git repository");
      }
      return Promise.resolve(undefined);
    });

    render(<App />);
    await user.click(
      await screen.findByRole("button", { name: "Open project…" }),
    );

    expect(
      await screen.findByText("the selected folder is not a git repository"),
    ).toBeInTheDocument();
  });

  it("shows the board and the project once connected", async () => {
    setInvoke((command) => {
      if (command === "current_project") {
        return Promise.resolve("/home/me/myproject");
      }
      if (command === "server_health") {
        return Promise.resolve({
          status: "ok",
          version: "0.1.0",
          workspace: "/home/me/myproject",
        });
      }
      if (command === "get_board") {
        return Promise.resolve({
          columns: [
            { id: "c1", board_id: "b1", name: "Backlog", position: 0 },
          ],
          tasks: [],
        });
      }
      return Promise.resolve(undefined);
    });

    render(<App />);

    // The status bar shows the project's folder name.
    expect(await screen.findByText("myproject")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Switch project" }),
    ).toBeInTheDocument();
    // The board itself rendered.
    expect(await screen.findByText("Backlog")).toBeInTheDocument();
  });

  it("updates a task's status dot from the live status feed", async () => {
    setInvoke((command) => {
      if (command === "current_project") {
        return Promise.resolve("/home/me/myproject");
      }
      if (command === "server_health") {
        return Promise.resolve({
          status: "ok",
          version: "0.1.0",
          workspace: "/home/me/myproject",
        });
      }
      if (command === "get_board") {
        return Promise.resolve({
          columns: [
            { id: "c1", board_id: "b1", name: "Backlog", position: 0 },
          ],
          tasks: [
            {
              id: "t1",
              column_id: "c1",
              title: "Write tests",
              description: "",
              position: 0,
              session_id: "s1",
              worktree_path: null,
              branch: null,
              created_at: 0,
            },
          ],
          statuses: {},
        });
      }
      return Promise.resolve(undefined);
    });

    render(<App />);
    // The task renders on both the board card and the task-list panel.
    await screen.findAllByText("Write tests");

    // The shell subscribed to the task-status feed once connected — fire one.
    const call = invokeMock.mock.calls.find(
      ([command]) => command === "watch_task_status",
    );
    const args = call?.[1] as {
      onEvent: { onmessage?: (message: unknown) => void };
    };
    args.onEvent.onmessage?.({ task_id: "t1", status: "running" });

    expect(
      (await screen.findAllByLabelText("Agent running")).length,
    ).toBeGreaterThan(0);
  });

  it("opens the command palette with Ctrl-K", async () => {
    const user = userEvent.setup();
    setInvoke((command) => {
      if (command === "current_project") {
        return Promise.resolve("/home/me/myproject");
      }
      if (command === "server_health") {
        return Promise.resolve({
          status: "ok",
          version: "0.1.0",
          workspace: "/home/me/myproject",
        });
      }
      if (command === "get_board") {
        return Promise.resolve({
          columns: [
            { id: "c1", board_id: "b1", name: "Backlog", position: 0 },
          ],
          tasks: [],
          statuses: {},
        });
      }
      return Promise.resolve(undefined);
    });

    render(<App />);
    await screen.findByText("Backlog");

    await user.keyboard("{Control>}k{/Control}");
    expect(
      await screen.findByPlaceholderText(/Search tasks/),
    ).toBeInTheDocument();
  });
});
