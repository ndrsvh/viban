import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  closestCorners,
  DndContext,
  DragOverlay,
  KeyboardSensor,
  PointerSensor,
  useDroppable,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragOverEvent,
  type DragStartEvent,
} from "@dnd-kit/core";
import {
  arrayMove,
  SortableContext,
  sortableKeyboardCoordinates,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";

import { TaskCard } from "@/components/task-card";
import { TaskDialog } from "@/components/task-dialog";
import { Button } from "@/components/ui/button";
import type { Column, Task } from "@/types/board";

interface BoardViewProps {
  onOpenSession: (sessionId: string) => void;
}

/** The Kanban board: columns of draggable task cards. */
export function BoardView({ onOpenSession }: BoardViewProps) {
  const [columns, setColumns] = useState<Column[]>([]);
  const [columnTasks, setColumnTasks] = useState<Record<string, string[]>>({});
  const [tasks, setTasks] = useState<Record<string, Task>>({});
  const [activeId, setActiveId] = useState<string | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [dialogTask, setDialogTask] = useState<Task | null>(null);
  const [dialogColumn, setDialogColumn] = useState<string | null>(null);
  const draggedFrom = useRef<string | null>(null);

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
    const id = String(event.active.id);
    setActiveId(id);
    draggedFrom.current = columnOf(id) ?? null;
  }

  // Live cross-column move so the card visually relocates mid-drag.
  function onDragOver(event: DragOverEvent) {
    const { active, over } = event;
    if (!over) return;
    const activeId = String(active.id);
    const overId = String(over.id);
    const from = columnOf(activeId);
    const to = columnOf(overId);
    if (!from || !to || from === to) return;
    setColumnTasks((prev) => {
      const fromIds = prev[from].filter((id) => id !== activeId);
      const toIds = prev[to].filter((id) => id !== activeId);
      const overIndex = toIds.indexOf(overId);
      toIds.splice(overIndex === -1 ? toIds.length : overIndex, 0, activeId);
      return { ...prev, [from]: fromIds, [to]: toIds };
    });
  }

  function onDragEnd(event: DragEndEvent) {
    const { active, over } = event;
    setActiveId(null);
    const from = draggedFrom.current;
    draggedFrom.current = null;
    if (!over) return;

    const activeId = String(active.id);
    const overId = String(over.id);
    const column = columnOf(activeId);
    if (!column) return;

    let next = columnTasks;
    if (activeId !== overId && columnOf(overId) === column) {
      const ids = columnTasks[column];
      const oldIndex = ids.indexOf(activeId);
      const newIndex = ids.indexOf(overId);
      if (oldIndex !== -1 && newIndex !== -1) {
        next = { ...columnTasks, [column]: arrayMove(ids, oldIndex, newIndex) };
        setColumnTasks(next);
      }
    }

    const affected = new Set<string>([column]);
    if (from && from !== column) affected.add(from);
    for (const columnId of affected) {
      void invoke("reorder_tasks", { columnId, taskIds: next[columnId] ?? [] });
    }
  }

  async function handleStartSession(task: Task) {
    const sessionId = crypto.randomUUID();
    try {
      await invoke("update_task", { taskId: task.id, sessionId });
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
        collisionDetection={closestCorners}
        onDragStart={onDragStart}
        onDragOver={onDragOver}
        onDragEnd={onDragEnd}
        onDragCancel={() => setActiveId(null)}
      >
        <div className="flex h-full gap-3">
          {columns.map((column) => (
            <BoardColumn
              key={column.id}
              column={column}
              taskIds={columnTasks[column.id] ?? []}
              tasks={tasks}
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
  onOpenSession: (sessionId: string) => void;
  onStartSession: (task: Task) => void;
  onEdit: (task: Task) => void;
  onAddTask: (columnId: string) => void;
}

function BoardColumn({
  column,
  taskIds,
  tasks,
  onOpenSession,
  onStartSession,
  onEdit,
  onAddTask,
}: BoardColumnProps) {
  const { setNodeRef } = useDroppable({ id: column.id });
  return (
    <div className="flex w-72 shrink-0 flex-col rounded-md bg-muted/40">
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
