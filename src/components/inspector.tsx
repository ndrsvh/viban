import { useEffect, useState, type ReactNode } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";

import { STATUS_DOT, STATUS_LABEL } from "@/lib/agent-status";
import { rpc } from "@/lib/rpc";
import { cn } from "@/lib/utils";
import { useBoardStore } from "@/stores/useBoardStore";
import type { Task } from "@/types/board";
import type { TokenUsage } from "@/types/session";

interface InspectorProps {
  /** The task whose context is shown. */
  task: Task;
}

/** Groups an integer with comma thousands separators, locale-independent. */
function grouped(value: number): string {
  return value.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

/**
 * The right-hand context panel for the open task (ADR-0004): agent status,
 * branch and worktree, token usage, and the edited-file count. Token and file
 * data is a `sessions.get` snapshot, refetched whenever the agent's status
 * changes — so it refreshes after every turn.
 */
export function Inspector({ task }: InspectorProps) {
  const [collapsed, setCollapsed] = useState(false);
  const status = useBoardStore((state) => state.statuses[task.id]);
  const [usage, setUsage] = useState<TokenUsage | null>(null);
  const [fileCount, setFileCount] = useState<number | null>(null);

  const sessionId = task.session_id;
  useEffect(() => {
    if (!sessionId) {
      setUsage(null);
      setFileCount(null);
      return;
    }
    let cancelled = false;
    rpc
      .getSession(sessionId)
      .then((session) => {
        if (cancelled) return;
        setUsage(session.usage);
        setFileCount(session.files.length);
      })
      .catch(() => {
        // Leave the last known values — the panel just goes a little stale.
      });
    return () => {
      cancelled = true;
    };
    // `status` is a trigger, not an input: a turn ending refetches the totals.
  }, [sessionId, status]);

  if (collapsed) {
    return (
      <aside className="flex w-8 shrink-0 flex-col items-center border-l py-2">
        <button
          type="button"
          aria-label="Show details"
          title="Show details"
          onClick={() => setCollapsed(false)}
          className="flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
        >
          <ChevronLeft className="h-4 w-4" />
        </button>
      </aside>
    );
  }

  return (
    <aside className="flex w-64 shrink-0 flex-col border-l">
      <div className="flex h-9 shrink-0 items-center justify-between border-b px-3">
        <span className="text-xs font-medium text-muted-foreground">
          Details
        </span>
        <button
          type="button"
          aria-label="Hide details"
          title="Hide details"
          onClick={() => setCollapsed(true)}
          className="flex h-6 w-6 items-center justify-center rounded text-muted-foreground hover:bg-accent hover:text-foreground"
        >
          <ChevronRight className="h-4 w-4" />
        </button>
      </div>
      <div className="flex flex-col gap-3 p-3 text-sm">
        <Row label="Status">
          {status ? (
            <span className="flex items-center gap-1.5">
              <span
                className={cn(
                  "h-2 w-2 shrink-0 rounded-full",
                  STATUS_DOT[status],
                )}
                aria-label={STATUS_LABEL[status]}
              />
              {STATUS_LABEL[status]}
            </span>
          ) : (
            <Muted>idle</Muted>
          )}
        </Row>
        <Row label="Branch">
          {task.branch ? (
            <span className="font-mono text-xs break-all">{task.branch}</span>
          ) : (
            <Muted>none</Muted>
          )}
        </Row>
        <Row label="Worktree">
          {task.worktree_path ? (
            <span className="font-mono text-xs break-all">
              {task.worktree_path}
            </span>
          ) : (
            <Muted>none</Muted>
          )}
        </Row>
        <Row label="Tokens">
          {usage && (usage.input_tokens > 0 || usage.output_tokens > 0) ? (
            <span className="font-mono text-xs">
              {`${grouped(usage.input_tokens)} in · ${grouped(
                usage.output_tokens,
              )} out`}
            </span>
          ) : (
            <Muted>—</Muted>
          )}
        </Row>
        <Row label="Files">
          {fileCount && fileCount > 0 ? (
            <span>{fileCount} edited</span>
          ) : (
            <Muted>none edited</Muted>
          )}
        </Row>
      </div>
    </aside>
  );
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
        {label}
      </span>
      <div>{children}</div>
    </div>
  );
}

function Muted({ children }: { children: ReactNode }) {
  return <span className="text-muted-foreground">{children}</span>;
}
