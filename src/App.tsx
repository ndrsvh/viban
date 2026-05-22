import { useCallback, useEffect, useState, type ReactNode } from "react";
import { Channel } from "@tauri-apps/api/core";

import { AppShell, type ServerStatus } from "@/components/app-shell";
import { BoardView } from "@/components/board-view";
import { CommandPalette } from "@/components/command-palette";
import { Inspector } from "@/components/inspector";
import { TaskDetail } from "@/components/task-detail";
import { TaskListPanel } from "@/components/task-list-panel";
import { Button } from "@/components/ui/button";
import { rpc } from "@/lib/rpc";
import { useBoardStore } from "@/stores/useBoardStore";
import { toast } from "@/stores/useToastStore";
import type { Task, TaskStatusUpdate } from "@/types/board";

// The server is polled continuously so the UI reflects sidecar restarts.
const POLL_INTERVAL_MS = 1500;

export default function App() {
  // `undefined` while still loading; `null` when no project is chosen.
  const [project, setProject] = useState<string | null | undefined>(undefined);
  const [status, setStatus] = useState<ServerStatus>("connecting");
  const [activeSession, setActiveSession] = useState<string | null>(null);
  const [reviewTask, setReviewTask] = useState<Task | null>(null);
  const [projectError, setProjectError] = useState<string | null>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);

  // The task whose session is open in the chat, resolved from the board store.
  const taskForSession = useBoardStore((state) =>
    activeSession
      ? (Object.values(state.tasks).find(
          (task) => task.session_id === activeSession,
        ) ?? null)
      : null,
  );

  useEffect(() => {
    void rpc
      .currentProject()
      .then((path) => setProject(path))
      .catch(() => setProject(null));
  }, []);

  // Poll the server's health, but only once a project is open — without one
  // the sidecar idles and there is nothing to connect to.
  useEffect(() => {
    if (!project) return;
    let cancelled = false;
    let failures = 0;
    let everConnected = false;

    const poll = async () => {
      try {
        await rpc.serverHealth();
        if (cancelled) return;
        failures = 0;
        everConnected = true;
        setStatus("ready");
      } catch {
        if (cancelled) return;
        failures += 1;
        if (failures >= 2) {
          setStatus(everConnected ? "reconnecting" : "connecting");
        }
      }
    };

    void poll();
    const timer = setInterval(() => void poll(), POLL_INTERVAL_MS);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [project]);

  // Subscribe to the live task-status feed while connected. It lives here, at
  // the shell level, so the task-list panel and board cards stay live no
  // matter which work area is open — and it re-subscribes after a reconnect.
  useEffect(() => {
    if (status !== "ready") return;
    const channel = new Channel<TaskStatusUpdate>();
    channel.onmessage = (update) => {
      useBoardStore.getState().setStatus(update.task_id, update.status);
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
  }, [status]);

  // Cmd/Ctrl-K toggles the command palette, app-wide, once a project is open.
  useEffect(() => {
    if (!project) return;
    const onKey = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setPaletteOpen((current) => !current);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [project]);

  const handleOpenProject = useCallback(async () => {
    setProjectError(null);
    try {
      const path = await rpc.openProject();
      if (path) {
        setActiveSession(null);
        setReviewTask(null);
        setStatus("connecting");
        setProject(path);
      }
    } catch (err) {
      // Surface the reason (e.g. "not a git repository") instead of
      // failing silently.
      setProjectError(String(err));
    }
  }, []);

  if (project === undefined) {
    return (
      <main className="flex h-screen w-screen items-center justify-center bg-background text-foreground">
        <h1 className="text-2xl font-medium tracking-tight">viban</h1>
      </main>
    );
  }

  if (project === null) {
    return (
      <main className="flex h-screen w-screen flex-col items-center justify-center gap-4 bg-background text-foreground">
        <div className="flex flex-col items-center gap-1">
          <h1 className="text-2xl font-medium tracking-tight">viban</h1>
          <p className="text-sm text-muted-foreground">
            Open a git repository to start.
          </p>
        </div>
        <Button onClick={() => void handleOpenProject()}>Open project…</Button>
        {projectError && (
          <p className="max-w-sm text-center text-sm text-destructive">
            {projectError}
          </p>
        )}
      </main>
    );
  }

  // A project is open: the persistent shell hosts every view. The work area
  // swaps between the board and a task detail; the rail, task-list panel, and
  // status bar stay mounted (ADR-0004).
  const goBoard = () => {
    setActiveSession(null);
    setReviewTask(null);
  };

  const openSession = (sessionId: string) => {
    setReviewTask(null);
    setActiveSession(sessionId);
  };

  // The task open in the detail, if any: a review names its task directly; a
  // chat names a session, resolved to its task above.
  const detailTask = reviewTask ?? taskForSession;

  let workArea: ReactNode;
  if (status === "connecting") {
    // The first connection is not established yet. The rail, task list, and
    // status bar still render, so this is a work-area placeholder — not, as
    // before, a full-screen takeover.
    workArea = (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        Connecting to the server…
      </div>
    );
  } else if (detailTask) {
    workArea = (
      <TaskDetail
        key={detailTask.id}
        task={detailTask}
        initialTab={reviewTask ? "diff" : "chat"}
        onClose={goBoard}
      />
    );
  } else {
    workArea = (
      <BoardView
        key={project}
        onOpenSession={openSession}
        onReview={setReviewTask}
      />
    );
  }

  return (
    <>
      <AppShell
        serverStatus={status}
        project={project}
        onSwitchProject={() => void handleOpenProject()}
        onGoBoard={goBoard}
        boardActive={!detailTask}
        taskList={
          <TaskListPanel
            activeSessionId={activeSession}
            reviewTaskId={reviewTask?.id ?? null}
            onOpenSession={openSession}
          />
        }
        inspector={detailTask ? <Inspector task={detailTask} /> : null}
      >
        <div className="flex h-full flex-col">
          {projectError && (
            <p className="border-b bg-destructive/10 px-3 py-1.5 text-xs text-destructive">
              {projectError}
            </p>
          )}
          <div className="flex-1 overflow-hidden">{workArea}</div>
        </div>
      </AppShell>
      <CommandPalette
        open={paletteOpen}
        onOpenChange={setPaletteOpen}
        onSelectTask={(task) => {
          if (task.session_id) openSession(task.session_id);
          else goBoard();
        }}
        onGoBoard={goBoard}
        onSwitchProject={() => void handleOpenProject()}
      />
    </>
  );
}
