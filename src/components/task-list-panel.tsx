import { useState } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";

import { ScrollArea } from "@/components/ui/scroll-area";
import { STATUS_DOT, STATUS_LABEL } from "@/lib/agent-status";
import { cn } from "@/lib/utils";
import { useBoardStore } from "@/stores/useBoardStore";
import type { Task } from "@/types/board";

interface TaskListPanelProps {
  /** The session currently open in the work area, if any. */
  activeSessionId: string | null;
  /** The task currently under review in the work area, if any. */
  reviewTaskId: string | null;
  /** Opens a task's session chat. */
  onOpenSession: (sessionId: string) => void;
}

/**
 * The persistent task navigator — the "master" of the master-detail layout
 * (ADR-0004). It lists every task on the board grouped by column, each row
 * carrying its live agent-status dot, and highlights whichever task is open.
 * A task with a session opens its chat; a task without one is listed but not
 * yet navigable — a later increment gives it a detail view. Board data comes
 * from the shared `useBoardStore`, kept current by the board view.
 */
export function TaskListPanel({
  activeSessionId,
  reviewTaskId,
  onOpenSession,
}: TaskListPanelProps) {
  const [collapsed, setCollapsed] = useState(false);
  const columns = useBoardStore((state) => state.columns);
  const columnTasks = useBoardStore((state) => state.columnTasks);
  const tasks = useBoardStore((state) => state.tasks);

  if (collapsed) {
    return (
      <aside className="flex w-8 shrink-0 flex-col items-center border-r py-2">
        <button
          type="button"
          aria-label="Show tasks"
          title="Show tasks"
          onClick={() => setCollapsed(false)}
          className="flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
        >
          <ChevronRight className="h-4 w-4" />
        </button>
      </aside>
    );
  }

  // Only populated columns are shown — the navigator stays compact, and an
  // empty column is not somewhere you navigate to anyway.
  const groups = columns
    .map((column) => ({
      column,
      tasks: (columnTasks[column.id] ?? [])
        .map((id) => tasks[id])
        .filter((task): task is Task => Boolean(task)),
    }))
    .filter((group) => group.tasks.length > 0);

  return (
    <aside className="flex w-60 shrink-0 flex-col border-r">
      <div className="flex h-9 shrink-0 items-center justify-between border-b px-3">
        <span className="text-xs font-medium text-muted-foreground">
          Tasks
        </span>
        <button
          type="button"
          aria-label="Hide tasks"
          title="Hide tasks"
          onClick={() => setCollapsed(true)}
          className="flex h-6 w-6 items-center justify-center rounded text-muted-foreground hover:bg-accent hover:text-foreground"
        >
          <ChevronLeft className="h-4 w-4" />
        </button>
      </div>
      <ScrollArea className="flex-1">
        {groups.length === 0 ? (
          <p className="p-3 text-xs text-muted-foreground">No tasks yet.</p>
        ) : (
          <div className="py-1">
            {groups.map((group) => (
              <div key={group.column.id} className="mb-1">
                <p className="px-3 py-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
                  {group.column.name}
                </p>
                {group.tasks.map((task) => (
                  <TaskRow
                    key={task.id}
                    task={task}
                    selected={
                      task.id === reviewTaskId ||
                      (task.session_id !== null &&
                        task.session_id === activeSessionId)
                    }
                    onOpenSession={onOpenSession}
                  />
                ))}
              </div>
            ))}
          </div>
        )}
      </ScrollArea>
    </aside>
  );
}

interface TaskRowProps {
  task: Task;
  selected: boolean;
  onOpenSession: (sessionId: string) => void;
}

/** One task row. Clickable — opening the chat — only when the task has a
 *  session; without one it is shown but inert. */
function TaskRow({ task, selected, onOpenSession }: TaskRowProps) {
  const status = useBoardStore((state) => state.statuses[task.id]);
  const dot = status ? (
    <span
      className={cn("h-2 w-2 shrink-0 rounded-full", STATUS_DOT[status])}
      title={STATUS_LABEL[status]}
      aria-label={STATUS_LABEL[status]}
    />
  ) : (
    <span className="h-2 w-2 shrink-0" aria-hidden />
  );

  if (task.session_id === null) {
    return (
      <div className="flex items-center gap-2 px-3 py-1.5 text-sm text-muted-foreground">
        {dot}
        <span className="truncate">{task.title}</span>
      </div>
    );
  }

  const sessionId = task.session_id;
  return (
    <button
      type="button"
      onClick={() => onOpenSession(sessionId)}
      aria-current={selected ? "page" : undefined}
      className={cn(
        "flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm",
        selected ? "bg-accent text-accent-foreground" : "hover:bg-accent/50",
      )}
    >
      {dot}
      <span className="truncate">{task.title}</span>
    </button>
  );
}
