import { describe, expect, it, vi } from "vitest";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";

import { RunPanel } from "@/components/run-panel";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: ((message: unknown) => void) | null = null;
  },
}));

const invokeMock = vi.mocked(invoke);

/** The output Channel handed to `watch_run`. */
function runChannel(): { onmessage: ((message: unknown) => void) | null } {
  const call = invokeMock.mock.calls.find((entry) => entry[0] === "watch_run");
  if (!call) throw new Error("watch_run was never invoked");
  return (call[1] as { onEvent: { onmessage: ((m: unknown) => void) | null } })
    .onEvent;
}

describe("RunPanel", () => {
  it("runs a command and streams its output and exit code", async () => {
    const user = userEvent.setup();
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
    render(<RunPanel taskId="t1" />);

    await user.type(
      screen.getByPlaceholderText(/Run a command/),
      "cargo test",
    );
    await user.click(screen.getByRole("button", { name: "Run" }));

    expect(invokeMock).toHaveBeenCalledWith("run_command", {
      taskId: "t1",
      command: "cargo test",
    });

    const channel = runChannel();
    act(() => {
      channel.onmessage?.({
        type: "line",
        stream: "stdout",
        text: "running 3 tests",
      });
      channel.onmessage?.({ type: "exited", code: 0 });
    });

    expect(await screen.findByText("running 3 tests")).toBeInTheDocument();
    expect(screen.getByText("exited 0")).toBeInTheDocument();
  });

  it("reports a non-zero exit", async () => {
    const user = userEvent.setup();
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
    render(<RunPanel taskId="t1" />);

    await user.type(screen.getByPlaceholderText(/Run a command/), "false");
    await user.click(screen.getByRole("button", { name: "Run" }));

    act(() => {
      runChannel().onmessage?.({ type: "exited", code: 1 });
    });
    expect(await screen.findByText("exited 1")).toBeInTheDocument();
  });
});
