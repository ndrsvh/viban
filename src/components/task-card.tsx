import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { Task } from "@/types/board";

interface TaskCardProps {
  task: Task;
  onOpenSession: (sessionId: string) => void;
  onStartSession: (task: Task) => void;
  onEdit: (task: Task) => void;
}

/** A draggable task card. */
export function TaskCard({
  task,
  onOpenSession,
  onStartSession,
  onEdit,
}: TaskCardProps) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } =
    useSortable({ id: task.id });
  const sessionId = task.session_id;

  return (
    <div
      ref={setNodeRef}
      style={{ transform: CSS.Transform.toString(transform), transition }}
      {...attributes}
      {...listeners}
      className={cn(
        "cursor-grab rounded-md border bg-card p-2 text-sm shadow-xs",
        isDragging && "opacity-50",
      )}
    >
      <p className="font-medium">{task.title}</p>
      {task.description && (
        <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">
          {task.description}
        </p>
      )}
      <div className="mt-2 flex gap-1">
        {sessionId ? (
          <Button
            size="xs"
            variant="secondary"
            onClick={() => onOpenSession(sessionId)}
          >
            Open chat
          </Button>
        ) : (
          <Button
            size="xs"
            variant="secondary"
            onClick={() => onStartSession(task)}
          >
            Start session
          </Button>
        )}
        <Button size="xs" variant="ghost" onClick={() => onEdit(task)}>
          Edit
        </Button>
      </div>
    </div>
  );
}
