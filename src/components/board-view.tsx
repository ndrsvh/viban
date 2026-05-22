import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  closestCorners,
  DndContext,
  DragOverlay,
  KeyboardSensor,
  PointerSensor,
  pointerWithin,
  useDroppable,
  useSensor,
  useSensors,
  type CollisionDetection,
  type DragEndEvent,
  type DragOverEvent,
  type DragStartEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  sortableKeyboardCoordinates,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";

import { TaskCard } from "@/components/task-card";
import { TaskDialog } from "@/components/task-dialog";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { Column, Task } from "@/types/board";

interface BoardViewProps {
  onOpenSession: (sessionId: string) => void;
}

// Cards are as wide as a column, so closest-corners is ambiguous between
// adjacent columns. Resolve the drop by the pointer's position instead,
// falling back to closest-corners when the pointer is in a gap.
const boardCollision: CollisionDetection = (args) => {
  const byPointer = pointerWithin(args);
  return byPointer.length > 0 ? byPointer : closestCorners(args);
};

/** The Kanban board: columns of draggable task cards. */
export function BoardView({ onOpenSession }: BoardViewProps) {
  const [columns, setColumns] = useState<Column[]>([]);
  const [columnTasks, setColumnTasks] = useState<Record<string, string[]>>({});
  const [tasks, setTasks] = useState<Record<string, Task>>({});
  const [activeId, setActiveId] = useState<string | null>(null);
  const [hoverColumn, setHoverColumn] = useState<string | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [dialogTask, setDialogTask] = useState<Task | null>(null);
  const [dialogColumn, setDialogColumn] = useState<string | null>(null);

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 8 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  const loadBoard = useCallback(async () => {
    try {
      const result = await invoke<{ columns: Column[]; tasks: Task[] }>(
        "get_board",
      );
      const taskMap: Record<string, Task> = {};
      const grouped: Record<string, string[]> = {};
      for (const column of result.columns) grouped[column.id] = [];
      for (const task of result.tasks) {
        taskMap[task.id] = task;
        (grouped[task.column_id] ??= []).push(task.id);
      }
      setColumns(result.columns);
      setTasks(taskMap);
      setColumnTasks(grouped);
    } catch (err) {
      console.error(err);
    }
  }, []);

  useEffect(() => {
    void loadBoard();
  }, [loadBoard]);

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
      void invoke("reorder_tasks", { columnId, taskIds: next[columnId] });
    }
  }

  async function handleStartSession(task: Task) {
    try {
      // The server creates the worktree + branch and links a session.
      const sessionId = await invoke<string>("start_session", {
        taskId: task.id,
      });
      await loadBoard();
      onOpenSession(sessionId);
    } catch (err) {
      console.error(err);
    }
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
    <div className="h-full overflow-x-auto p-4">
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

      <TaskDialog
        open={dialogOpen}
        task={dialogTask}
        columnId={dialogColumn}
        onOpenChange={setDialogOpen}
        onChanged={loadBoard}
      />
    </div>
  );
}

interface BoardColumnProps {
  column: Column;
  taskIds: string[];
  tasks: Record<string, Task>;
  /** True while a dragged card's drop target is this column. */
  isTarget: boolean;
  onOpenSession: (sessionId: string) => void;
  onStartSession: (task: Task) => void;
  onEdit: (task: Task) => void;
  onAddTask: (columnId: string) => void;
}

function BoardColumn({
  column,
  taskIds,
  tasks,
  isTarget,
  onOpenSession,
  onStartSession,
  onEdit,
  onAddTask,
}: BoardColumnProps) {
  const { setNodeRef } = useDroppable({ id: column.id });
  return (
    <div
      className={cn(
        "flex w-72 shrink-0 flex-col rounded-md bg-muted/40 ring-2 transition-colors",
        isTarget ? "ring-primary" : "ring-transparent",
      )}
    >
      <div className="flex items-center justify-between px-3 py-2">
        <h2 className="text-sm font-medium">{column.name}</h2>
        <span className="text-xs text-muted-foreground">{taskIds.length}</span>
      </div>
      <div
        ref={setNodeRef}
        className="flex flex-1 flex-col gap-2 overflow-y-auto p-2"
      >
        <SortableContext items={taskIds} strategy={verticalListSortingStrategy}>
          {taskIds.map((id) => {
            const task = tasks[id];
            return task ? (
              <TaskCard
                key={id}
                task={task}
                onOpenSession={onOpenSession}
                onStartSession={onStartSession}
                onEdit={onEdit}
              />
            ) : null;
          })}
        </SortableContext>
        {taskIds.length === 0 && (
          <div className="rounded-md border border-dashed py-6 text-center text-xs text-muted-foreground">
            Drop tasks here
          </div>
        )}
        <Button
          variant="ghost"
          size="sm"
          className="justify-start text-muted-foreground"
          onClick={() => onAddTask(column.id)}
        >
          + Add task
        </Button>
      </div>
    </div>
  );
}
