import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";

import { TaskDialog } from "@/components/task-dialog";
import type { Task } from "@/types/board";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const invokeMock = vi.mocked(invoke);

function makeTask(overrides: Partial<Task> = {}): Task {
  return {
    id: "t1",
    column_id: "c1",
    title: "Existing",
    description: "Existing desc",
    position: 0,
    session_id: null,
    worktree_path: null,
    branch: null,
    created_at: 0,
    ...overrides,
  };
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockResolvedValue(undefined);
});

describe("TaskDialog", () => {
  it("creates a task in the target column", async () => {
    const user = userEvent.setup();
    const onChanged = vi.fn();
    const onOpenChange = vi.fn();
    render(
      <TaskDialog
        open
        task={null}
        columnId="col-1"
        onOpenChange={onOpenChange}
        onChanged={onChanged}
      />,
    );

    await user.type(screen.getByPlaceholderText("Task title"), "New task");
    await user.type(
      screen.getByPlaceholderText("Description (optional)"),
      "details",
    );
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    expect(invokeMock).toHaveBeenCalledWith("create_task", {
      columnId: "col-1",
      title: "New task",
      description: "details",
    });
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("pre-fills and updates an existing task", async () => {
    const user = userEvent.setup();
    const onChanged = vi.fn();
    render(
      <TaskDialog
        open
        task={makeTask()}
        columnId={null}
        onOpenChange={vi.fn()}
        onChanged={onChanged}
      />,
    );

    const title = screen.getByPlaceholderText("Task title");
    expect(title).toHaveValue("Existing");

    await user.clear(title);
    await user.type(title, "Renamed");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    expect(invokeMock).toHaveBeenCalledWith("update_task", {
      taskId: "t1",
      title: "Renamed",
      description: "Existing desc",
    });
  });

  it("deletes an existing task", async () => {
    const user = userEvent.setup();
    const onChanged = vi.fn();
    render(
      <TaskDialog
        open
        task={makeTask()}
        columnId={null}
        onOpenChange={vi.fn()}
        onChanged={onChanged}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Delete" }));

    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    expect(invokeMock).toHaveBeenCalledWith("delete_task", { taskId: "t1" });
  });

  it("has no Delete button while creating a task", () => {
    render(
      <TaskDialog
        open
        task={null}
        columnId="col-1"
        onOpenChange={vi.fn()}
        onChanged={vi.fn()}
      />,
    );
    expect(
      screen.queryByRole("button", { name: "Delete" }),
    ).not.toBeInTheDocument();
  });

  it("keeps Save disabled until a title is entered", async () => {
    const user = userEvent.setup();
    render(
      <TaskDialog
        open
        task={null}
        columnId="col-1"
        onOpenChange={vi.fn()}
        onChanged={vi.fn()}
      />,
    );

    const save = screen.getByRole("button", { name: "Save" });
    expect(save).toBeDisabled();
    await user.type(screen.getByPlaceholderText("Task title"), "x");
    expect(save).toBeEnabled();
  });
});
