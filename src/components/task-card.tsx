import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { useBoardStore } from "@/stores/useBoardStore";
import type { AgentStatus, Task } from "@/types/board";

/** Tailwind classes for the live-status dot, by agent status. */
const STATUS_DOT: Record<AgentStatus, string> = {
  running: "animate-pulse bg-amber-500",
  done: "bg-emerald-500",
  failed: "bg-red-500",
};

const STATUS_LABEL: Record<AgentStatus, string> = {
  running: "Agent running",
  done: "Agent finished",
  failed: "Agent failed",
};

interface TaskCardProps {
  task: Task;
  onOpenSession: (sessionId: string) => void;
  onStartSession: (task: Task) => void;
  onReview: (task: Task) => void;
  onMerge: (task: Task) => void;
  onNewAttempt: (task: Task) => void;
  onEdit: (task: Task) => void;
}

/** A draggable task card. */
export function TaskCard({
  task,
  onOpenSession,
  onStartSession,
  onReview,
  onMerge,
  onNewAttempt,
  onEdit,
}: TaskCardProps) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } =
    useSortable({ id: task.id });
  const sessionId = task.session_id;
  const status = useBoardStore((state) => state.statuses[task.id]);

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
      <div className="flex items-center gap-1.5">
        {status && (
          <span
            className={cn(
              "h-2 w-2 shrink-0 rounded-full",
              STATUS_DOT[status],
            )}
            title={STATUS_LABEL[status]}
            aria-label={STATUS_LABEL[status]}
          />
        )}
        <p className="font-medium">{task.title}</p>
      </div>
      {task.description && (
        <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">
          {task.description}
        </p>
      )}
      {task.branch && (
        <p className="mt-1.5 truncate font-mono text-[11px] text-muted-foreground">
          ⎇ {task.branch}
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
        {task.worktree_path && (
          <Button size="xs" variant="secondary" onClick={() => onReview(task)}>
            Review
          </Button>
        )}
        {task.branch && (
          <Button size="xs" variant="secondary" onClick={() => onMerge(task)}>
            Merge
          </Button>
        )}
        {sessionId && (
          <Button
            size="xs"
            variant="ghost"
            onClick={() => onNewAttempt(task)}
          >
            New attempt
          </Button>
        )}
        <Button size="xs" variant="ghost" onClick={() => onEdit(task)}>
          Edit
        </Button>
      </div>
    </div>
  );
}
