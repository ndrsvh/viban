import { useEffect, useState } from "react";
import { Channel } from "@tauri-apps/api/core";
import { X } from "lucide-react";
import {
  closestCorners,
  DndContext,
  DragOverlay,
  KeyboardSensor,
  PointerSensor,
  pointerWithin,
  useSensor,
  useSensors,
  type CollisionDetection,
  type DragEndEvent,
  type DragOverEvent,
  type DragStartEvent,
} from "@dnd-kit/core";
import { sortableKeyboardCoordinates } from "@dnd-kit/sortable";

import { BoardColumn } from "@/components/board-column";
import { ConfirmDialog } from "@/components/confirm-dialog";
import { GitInitDialog } from "@/components/git-init-dialog";
import { TaskDialog } from "@/components/task-dialog";
import { rpc } from "@/lib/rpc";
import { useBoardStore } from "@/stores/useBoardStore";
import { toast } from "@/stores/useToastStore";
import type { Task, TaskStatusUpdate } from "@/types/board";

interface BoardViewProps {
  onOpenSession: (sessionId: string) => void;
  onReview: (task: Task) => void;
}

// Cards are as wide as a column, so closest-corners is ambiguous between
// adjacent columns. Resolve the drop by the pointer's position instead,
// falling back to closest-corners when the pointer is in a gap.
const boardCollision: CollisionDetection = (args) => {
  const byPointer = pointerWithin(args);
  return byPointer.length > 0 ? byPointer : closestCorners(args);
};

/** The Kanban board: columns of draggable task cards. Board data lives in
 *  `useBoardStore`; this component owns only the transient UI state. */
export function BoardView({ onOpenSession, onReview }: BoardViewProps) {
  const columns = useBoardStore((state) => state.columns);
  const columnTasks = useBoardStore((state) => state.columnTasks);
  const tasks = useBoardStore((state) => state.tasks);
  const loadBoard = useBoardStore((state) => state.loadBoard);
  const setColumnTasks = useBoardStore((state) => state.setColumnTasks);
  const setStatus = useBoardStore((state) => state.setStatus);

  const [activeId, setActiveId] = useState<string | null>(null);
  const [hoverColumn, setHoverColumn] = useState<string | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [dialogTask, setDialogTask] = useState<Task | null>(null);
  const [dialogColumn, setDialogColumn] = useState<string | null>(null);
  // The task awaiting git-init confirmation, and whether init is running.
  const [gitInitTask, setGitInitTask] = useState<Task | null>(null);
  const [gitInitBusy, setGitInitBusy] = useState(false);
  // The task awaiting merge confirmation, and whether the merge is running.
  const [mergeTask, setMergeTask] = useState<Task | null>(null);
  const [mergeBusy, setMergeBusy] = useState(false);
  // The last action error, shown as a dismissible banner. Without this a
  // failed start_session / merge fails silently — the dialog just vanishes.
  const [actionError, setActionError] = useState<string | null>(null);

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 8 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  useEffect(() => {
    void loadBoard();
  }, [loadBoard]);

  // Subscribe to the live task-status feed while the board is shown: the
  // cards' status dots update in place, and a finished/failed agent toasts.
  useEffect(() => {
    const channel = new Channel<TaskStatusUpdate>();
    channel.onmessage = (update) => {
      setStatus(update.task_id, update.status);
      if (update.status === "running") return;
      const title = useBoardStore.getState().tasks[update.task_id]?.title;
      const name = title ? `"${title}"` : "A task";
      if (update.status === "done") {
        toast.info(`${name} — the agent finished.`);
      } else {
        toast.error(`${name} — the agent failed.`);
      }
    };
    void rpc.watchTaskStatus(channel);
    return () => {
      void rpc.unwatchTaskStatus();
    };
  }, [setStatus]);

  function columnOf(id: string): string | undefined {
    if (columnTasks[id]) return id;
    return Object.keys(columnTasks).find((columnId) =>
      columnTasks[columnId].includes(id),
    );
  }

  function onDragStart(event: DragStartEvent) {
    setActiveId(String(event.active.id));
  }

  // Only tracks the hovered column for the drop-target highlight. The actual
  // move happens on drop — moving mid-drag oscillates between adjacent columns.
  function onDragOver(event: DragOverEvent) {
    const overId = event.over ? String(event.over.id) : null;
    setHoverColumn(overId ? (columnOf(overId) ?? null) : null);
  }

  function onDragEnd(event: DragEndEvent) {
    const { active, over } = event;
    setActiveId(null);
    setHoverColumn(null);
    if (!over) return;

    const activeId = String(active.id);
    const overId = String(over.id);
    const from = columnOf(activeId);
    const to = columnOf(overId);
    if (!from || !to) return;
    if (from === to && activeId === overId) return;

    const next = { ...columnTasks };
    next[from] = next[from].filter((id) => id !== activeId);
    const toIds = next[to].filter((id) => id !== activeId);
    const overIndex = overId === to ? -1 : toIds.indexOf(overId);
    toIds.splice(overIndex === -1 ? toIds.length : overIndex, 0, activeId);
    next[to] = toIds;
    setColumnTasks(next);

    for (const columnId of new Set([from, to])) {
      void rpc.reorderTasks(columnId, next[columnId]);
    }
  }

  async function handleStartSession(
    task: Task,
    options: { initGit?: boolean; withoutGit?: boolean } = {},
  ) {
    setActionError(null);
    try {
      // The server links a session to the task. By default that means an
      // isolated git worktree; if the project folder is not a git repo it
      // asks how to proceed first (init git, or work without git).
      const result = await rpc.startSession(task.id, options);
      if (result.needs_git_init) {
        setGitInitTask(task);
        return;
      }
      setGitInitTask(null);
      if (result.session_id) {
        await loadBoard();
        onOpenSession(result.session_id);
      }
    } catch (err) {
      console.error(err);
      setGitInitTask(null);
      setActionError(`Could not start the session: ${String(err)}`);
    }
  }

  async function confirmGitInit() {
    const task = gitInitTask;
    if (!task) return;
    setGitInitBusy(true);
    await handleStartSession(task, { initGit: true });
    setGitInitBusy(false);
  }

  async function confirmWorkWithoutGit() {
    const task = gitInitTask;
    if (!task) return;
    setGitInitBusy(true);
    await handleStartSession(task, { withoutGit: true });
    setGitInitBusy(false);
  }

  async function handleNewAttempt(task: Task) {
    setActionError(null);
    try {
      const result = await rpc.createAttempt(task.id);
      if (result.session_id) {
        await loadBoard();
        onOpenSession(result.session_id);
      }
    } catch (err) {
      console.error(err);
      setActionError(`Could not start a new attempt: ${String(err)}`);
    }
  }

  async function confirmMerge() {
    const task = mergeTask;
    if (!task) return;
    setActionError(null);
    setMergeBusy(true);
    try {
      await rpc.gitMerge(task.id);
      await loadBoard();
    } catch (err) {
      console.error(err);
      setActionError(`Could not merge the task: ${String(err)}`);
    }
    setMergeBusy(false);
    setMergeTask(null);
  }

  function openCreate(columnId: string) {
    setDialogTask(null);
    setDialogColumn(columnId);
    setDialogOpen(true);
  }

  function openEdit(task: Task) {
    setDialogTask(task);
    setDialogColumn(null);
    setDialogOpen(true);
  }

  const activeTask = activeId ? tasks[activeId] : null;

  return (
    <div className="flex h-full flex-col">
      {actionError && (
        <div
          role="alert"
          className="flex items-start gap-2 border-b border-destructive/30 bg-destructive/10 px-4 py-2 text-sm text-destructive"
        >
          <span className="flex-1 break-words">{actionError}</span>
          <button
            type="button"
            aria-label="Dismiss error"
            className="shrink-0 rounded p-0.5 hover:bg-destructive/20"
            onClick={() => setActionError(null)}
          >
            <X className="h-4 w-4" />
          </button>
        </div>
      )}
      <div className="flex-1 overflow-x-auto p-4">
        <DndContext
          sensors={sensors}
          collisionDetection={boardCollision}
          onDragStart={onDragStart}
          onDragOver={onDragOver}
          onDragEnd={onDragEnd}
          onDragCancel={() => {
            setActiveId(null);
            setHoverColumn(null);
          }}
        >
          <div className="flex h-full gap-3">
            {columns.map((column) => (
              <BoardColumn
                key={column.id}
                column={column}
                taskIds={columnTasks[column.id] ?? []}
                tasks={tasks}
                isTarget={column.id === hoverColumn}
                onOpenSession={onOpenSession}
                onStartSession={handleStartSession}
                onReview={onReview}
                onMerge={(task) => setMergeTask(task)}
                onNewAttempt={handleNewAttempt}
                onEdit={openEdit}
                onAddTask={openCreate}
              />
            ))}
          </div>
          <DragOverlay>
            {activeTask ? (
              <div className="rounded-md border bg-card p-2 text-sm shadow-md">
                <p className="font-medium">{activeTask.title}</p>
              </div>
            ) : null}
          </DragOverlay>
        </DndContext>
      </div>

      <TaskDialog
        open={dialogOpen}
        task={dialogTask}
        columnId={dialogColumn}
        onOpenChange={setDialogOpen}
        onChanged={loadBoard}
      />

      <GitInitDialog
        open={gitInitTask !== null}
        busy={gitInitBusy}
        onConfirm={() => void confirmGitInit()}
        onWorkWithoutGit={() => void confirmWorkWithoutGit()}
        onOpenChange={(open) => {
          if (!open && !gitInitBusy) setGitInitTask(null);
        }}
      />

      <ConfirmDialog
        open={mergeTask !== null}
        title="Merge this task?"
        description={
          mergeTask
            ? `Merge ${mergeTask.branch ?? "the task branch"} into the project, ` +
              "remove the worktree, and move the task to Done."
            : ""
        }
        confirmLabel="Merge branch"
        busyLabel="Merging…"
        busy={mergeBusy}
        onConfirm={() => void confirmMerge()}
        onOpenChange={(open) => {
          if (!open && !mergeBusy) setMergeTask(null);
        }}
      />
    </div>
  );
}
