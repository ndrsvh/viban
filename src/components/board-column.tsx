import { useDroppable } from "@dnd-kit/core";
import {
  SortableContext,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";

import { TaskCard } from "@/components/task-card";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { Column, Task } from "@/types/board";

interface BoardColumnProps {
  column: Column;
  taskIds: string[];
  tasks: Record<string, Task>;
  /** True while a dragged card's drop target is this column. */
  isTarget: boolean;
  onOpenSession: (sessionId: string) => void;
  onStartSession: (task: Task) => void;
  onReview: (task: Task) => void;
  onMerge: (task: Task) => void;
  onNewAttempt: (task: Task) => void;
  onEdit: (task: Task) => void;
  onAddTask: (columnId: string) => void;
}

/** One Kanban column: a drop target holding its sortable task cards. */
export function BoardColumn({
  column,
  taskIds,
  tasks,
  isTarget,
  onOpenSession,
  onStartSession,
  onReview,
  onMerge,
  onNewAttempt,
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
                onReview={onReview}
                onMerge={onMerge}
                onNewAttempt={onNewAttempt}
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
