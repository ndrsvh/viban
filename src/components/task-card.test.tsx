import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import {
  DndContext,
  PointerSensor,
  useSensor,
  useSensors,
} from "@dnd-kit/core";

import { TaskCard } from "@/components/task-card";
import { useBoardStore } from "@/stores/useBoardStore";
import type { Task } from "@/types/board";

afterEach(() => {
  // The status dot reads the shared board store — clear it between tests.
  useBoardStore.setState({ statuses: {} });
});

function makeTask(overrides: Partial<Task> = {}): Task {
  return {
    id: "t1",
    column_id: "c1",
    title: "My task",
    description: "",
    position: 0,
    session_id: null,
    worktree_path: null,
    branch: null,
    created_at: 0,
    ...overrides,
  };
}

// A drag context with the same activation distance the board uses, so a plain
// click on a card button never starts a drag.
function DndWrapper({ children }: { children: ReactNode }) {
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 8 } }),
  );
  return <DndContext sensors={sensors}>{children}</DndContext>;
}

interface Handlers {
  onOpenSession: (sessionId: string) => void;
  onStartSession: (task: Task) => void;
  onReview: (task: Task) => void;
  onMerge: (task: Task) => void;
  onNewAttempt: (task: Task) => void;
  onEdit: (task: Task) => void;
}

function renderCard(task: Task, handlers: Partial<Handlers> = {}) {
  const props: Handlers = {
    onOpenSession: handlers.onOpenSession ?? vi.fn(),
    onStartSession: handlers.onStartSession ?? vi.fn(),
    onReview: handlers.onReview ?? vi.fn(),
    onMerge: handlers.onMerge ?? vi.fn(),
    onNewAttempt: handlers.onNewAttempt ?? vi.fn(),
    onEdit: handlers.onEdit ?? vi.fn(),
  };
  render(
    <DndWrapper>
      <TaskCard task={task} {...props} />
    </DndWrapper>,
  );
  return props;
}

describe("TaskCard", () => {
  it("renders the title and description", () => {
    renderCard(
      makeTask({ title: "Build the thing", description: "Some detail" }),
    );
    expect(screen.getByText("Build the thing")).toBeInTheDocument();
    expect(screen.getByText("Some detail")).toBeInTheDocument();
  });

  it("omits the description paragraph when empty", () => {
    renderCard(makeTask({ description: "" }));
    // Only the title text is present.
    expect(screen.getByText("My task")).toBeInTheDocument();
  });

  it("shows the branch name once the task has a worktree", () => {
    renderCard(makeTask({ branch: "viban/build-thing-12ab" }));
    expect(screen.getByText(/viban\/build-thing-12ab/)).toBeInTheDocument();
  });

  it("offers Start session when no session exists yet", async () => {
    const user = userEvent.setup();
    const onStartSession = vi.fn();
    const task = makeTask();
    renderCard(task, { onStartSession });

    await user.click(screen.getByRole("button", { name: "Start session" }));
    expect(onStartSession).toHaveBeenCalledWith(task);
  });

  it("offers Open chat once a session is linked", async () => {
    const user = userEvent.setup();
    const onOpenSession = vi.fn();
    renderCard(makeTask({ session_id: "sess-9" }), { onOpenSession });

    expect(
      screen.queryByRole("button", { name: "Start session" }),
    ).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Open chat" }));
    expect(onOpenSession).toHaveBeenCalledWith("sess-9");
  });

  it("invokes the edit handler with the task", async () => {
    const user = userEvent.setup();
    const onEdit = vi.fn();
    const task = makeTask();
    renderCard(task, { onEdit });

    await user.click(screen.getByRole("button", { name: "Edit" }));
    expect(onEdit).toHaveBeenCalledWith(task);
  });

  it("offers Review only once the task has a worktree", async () => {
    const user = userEvent.setup();

    renderCard(makeTask());
    expect(
      screen.queryByRole("button", { name: "Review" }),
    ).not.toBeInTheDocument();

    const onReview = vi.fn();
    const task = makeTask({
      session_id: "sess-1",
      worktree_path: "/repo/.viban/worktrees/t1",
      branch: "viban/my-task-1",
    });
    renderCard(task, { onReview });
    await user.click(screen.getByRole("button", { name: "Review" }));
    expect(onReview).toHaveBeenCalledWith(task);
  });

  it("offers New attempt only once the task has a session", async () => {
    const user = userEvent.setup();

    renderCard(makeTask());
    expect(
      screen.queryByRole("button", { name: "New attempt" }),
    ).not.toBeInTheDocument();

    const onNewAttempt = vi.fn();
    const task = makeTask({ session_id: "sess-1" });
    renderCard(task, { onNewAttempt });
    await user.click(screen.getByRole("button", { name: "New attempt" }));
    expect(onNewAttempt).toHaveBeenCalledWith(task);
  });

  it("shows a live status dot once the task's agent reports", () => {
    useBoardStore.setState({ statuses: { t1: "running" } });
    renderCard(makeTask({ id: "t1" }));
    expect(screen.getByLabelText("Agent running")).toBeInTheDocument();
  });

  it("shows no status dot for a task with no agent activity", () => {
    renderCard(makeTask({ id: "t1" }));
    expect(screen.queryByLabelText(/^Agent /)).not.toBeInTheDocument();
  });

  it("offers Merge only once the task has a branch", async () => {
    const user = userEvent.setup();

    renderCard(makeTask());
    expect(
      screen.queryByRole("button", { name: "Merge" }),
    ).not.toBeInTheDocument();

    const onMerge = vi.fn();
    const task = makeTask({
      session_id: "sess-1",
      worktree_path: "/repo/.viban/worktrees/t1",
      branch: "viban/my-task-1",
    });
    renderCard(task, { onMerge });
    await user.click(screen.getByRole("button", { name: "Merge" }));
    expect(onMerge).toHaveBeenCalledWith(task);
  });
});
