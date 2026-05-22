import { useEffect, useState, type ReactNode } from "react";

import { ChatView } from "@/components/chat-view";
import { DiffView } from "@/components/diff-view";
import { RunPanel } from "@/components/run-panel";
import { ScrollArea } from "@/components/ui/scroll-area";
import { rpc } from "@/lib/rpc";
import { cn } from "@/lib/utils";
import type { Task } from "@/types/board";

/** The tabs of the task detail. */
export type TaskTab = "chat" | "diff" | "run" | "files";

const TABS: { id: TaskTab; label: string }[] = [
  { id: "chat", label: "Chat" },
  { id: "diff", label: "Diff" },
  { id: "run", label: "Run" },
  { id: "files", label: "Files" },
];

interface TaskDetailProps {
  /** The task being worked on. */
  task: Task;
  /** Which tab to open first. */
  initialTab?: TaskTab;
  /** Returns to the board. */
  onClose: () => void;
}

/**
 * The detail surface for one task (ADR-0004): a breadcrumb, a tab bar, and the
 * active tab's content. Chat is the agent conversation, Diff the worktree
 * review, Run an arbitrary command in the worktree, Files the session's edited
 * footprint. A tab's content mounts when the tab is first opened and then
 * stays mounted, so switching tabs never drops a live chat stream or a diff.
 */
export function TaskDetail({
  task,
  initialTab = "chat",
  onClose,
}: TaskDetailProps) {
  const [tab, setTab] = useState<TaskTab>(initialTab);
  const [visited, setVisited] = useState<Set<TaskTab>>(
    () => new Set([initialTab]),
  );

  const activate = (next: TaskTab) => {
    setTab(next);
    setVisited((prev) => {
      if (prev.has(next)) return prev;
      const updated = new Set(prev);
      updated.add(next);
      return updated;
    });
  };

  return (
    <div className="flex h-full flex-col">
      <header className="flex h-9 shrink-0 items-center gap-1.5 border-b px-2 text-sm">
        <button
          type="button"
          onClick={onClose}
          className="rounded px-1.5 py-0.5 text-muted-foreground hover:bg-accent hover:text-foreground"
        >
          Board
        </button>
        <span className="text-muted-foreground">/</span>
        <span className="truncate font-medium">{task.title}</span>
      </header>

      <div
        role="tablist"
        className="flex shrink-0 items-center gap-1 border-b px-2"
      >
        {TABS.map((entry) => (
          <button
            key={entry.id}
            type="button"
            role="tab"
            aria-selected={tab === entry.id}
            onClick={() => activate(entry.id)}
            className={cn(
              "border-b-2 px-3 py-1.5 text-sm",
              tab === entry.id
                ? "border-foreground font-medium"
                : "border-transparent text-muted-foreground hover:text-foreground",
            )}
          >
            {entry.label}
          </button>
        ))}
      </div>

      <div className="min-h-0 flex-1">
        {visited.has("chat") && (
          <TabPanel active={tab === "chat"}>
            {task.session_id ? (
              <ChatView
                key={task.session_id}
                sessionId={task.session_id}
                onSpawned={() => {}}
              />
            ) : (
              <Empty>This task has no session yet.</Empty>
            )}
          </TabPanel>
        )}
        {visited.has("diff") && (
          <TabPanel active={tab === "diff"}>
            <DiffView task={task} onDone={onClose} />
          </TabPanel>
        )}
        {visited.has("run") && (
          <TabPanel active={tab === "run"}>
            <RunPanel taskId={task.id} />
          </TabPanel>
        )}
        {visited.has("files") && (
          <TabPanel active={tab === "files"}>
            <FilesTab sessionId={task.session_id} />
          </TabPanel>
        )}
      </div>
    </div>
  );
}

/** Wraps a tab's content — kept mounted, but hidden, when not active. */
function TabPanel({
  active,
  children,
}: {
  active: boolean;
  children: ReactNode;
}) {
  return (
    <div role="tabpanel" className={cn("h-full", !active && "hidden")}>
      {children}
    </div>
  );
}

function Empty({ children }: { children: ReactNode }) {
  return (
    <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
      {children}
    </div>
  );
}

/** The session's edited-file footprint — a snapshot fetched when first shown. */
function FilesTab({ sessionId }: { sessionId: string | null }) {
  const [files, setFiles] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (!sessionId) {
      setLoading(false);
      return;
    }
    let cancelled = false;
    rpc
      .getSession(sessionId)
      .then((session) => {
        if (!cancelled) setFiles(session.files);
      })
      .catch(() => {
        if (!cancelled) setFiles([]);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [sessionId]);

  if (loading) {
    return <Empty>Loading…</Empty>;
  }
  if (files.length === 0) {
    return <Empty>No files edited yet.</Empty>;
  }
  return (
    <ScrollArea className="h-full">
      <ul className="p-3">
        {files.map((file) => (
          <li key={file} className="py-0.5 font-mono text-xs">
            {file}
          </li>
        ))}
      </ul>
    </ScrollArea>
  );
}
