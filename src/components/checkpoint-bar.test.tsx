import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";

import { CheckpointBar } from "@/components/checkpoint-bar";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);

function checkpoint(id: string, label: string) {
  return { id, task_id: "t1", commit_sha: `sha-${id}`, label, created_at: 0 };
}

describe("CheckpointBar", () => {
  it("saves a checkpoint", async () => {
    const user = userEvent.setup();
    invokeMock.mockReset();
    invokeMock.mockImplementation(((command: string) => {
      if (command === "list_checkpoints") {
        return Promise.resolve({ checkpoints: [] });
      }
      if (command === "create_checkpoint") {
        return Promise.resolve({ checkpoint: checkpoint("c1", "Checkpoint 1") });
      }
      return Promise.resolve();
    }) as unknown as typeof invoke);

    render(<CheckpointBar taskId="t1" onRestored={vi.fn()} />);
    await screen.findByText("No checkpoints yet.");

    await user.click(screen.getByRole("button", { name: "Save checkpoint" }));

    expect(
      await screen.findByRole("button", { name: /Checkpoint 1/ }),
    ).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("create_checkpoint", {
      taskId: "t1",
      label: "Checkpoint 1",
    });
  });

  it("restores a checkpoint after confirmation", async () => {
    const user = userEvent.setup();
    const onRestored = vi.fn();
    invokeMock.mockReset();
    invokeMock.mockImplementation(((command: string) => {
      if (command === "list_checkpoints") {
        return Promise.resolve({
          checkpoints: [checkpoint("c1", "Before refactor")],
        });
      }
      return Promise.resolve();
    }) as unknown as typeof invoke);

    render(<CheckpointBar taskId="t1" onRestored={onRestored} />);
    await user.click(
      await screen.findByRole("button", { name: /Before refactor/ }),
    );

    // A confirmation dialog appears instead of restoring immediately.
    expect(
      await screen.findByText("Restore this checkpoint?"),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Restore" }));

    await waitFor(() => expect(onRestored).toHaveBeenCalled());
    expect(invokeMock).toHaveBeenCalledWith("restore_checkpoint", {
      checkpointId: "c1",
    });
  });
});
