import { useCallback, useEffect, useState, type ReactNode } from "react";

import { AppShell, type ServerStatus } from "@/components/app-shell";
import { BoardView } from "@/components/board-view";
import { ChatView } from "@/components/chat-view";
import { DiffView } from "@/components/diff-view";
import { Button } from "@/components/ui/button";
import { rpc } from "@/lib/rpc";
import type { Task } from "@/types/board";

// The server is polled continuously so the UI reflects sidecar restarts.
const POLL_INTERVAL_MS = 1500;

export default function App() {
  // `undefined` while still loading; `null` when no project is chosen.
  const [project, setProject] = useState<string | null | undefined>(undefined);
  const [status, setStatus] = useState<ServerStatus>("connecting");
  const [activeSession, setActiveSession] = useState<string | null>(null);
  const [reviewTask, setReviewTask] = useState<Task | null>(null);
  const [projectError, setProjectError] = useState<string | null>(null);

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
  // swaps between the board, a session chat, and a diff review; the rail and
  // status bar stay mounted (ADR-0004).
  const goBoard = () => {
    setActiveSession(null);
    setReviewTask(null);
  };

  let workArea: ReactNode;
  if (status === "connecting") {
    // The first connection is not established yet. The rail and status bar
    // still render, so this is a work-area placeholder — not, as before, a
    // full-screen takeover.
    workArea = (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        Connecting to the server…
      </div>
    );
  } else if (reviewTask) {
    workArea = (
      <DiffView
        key={reviewTask.id}
        task={reviewTask}
        onDone={() => setReviewTask(null)}
      />
    );
  } else if (activeSession) {
    workArea = (
      <div className="flex h-full flex-col">
        <header className="flex items-center border-b px-2 py-1.5">
          <Button variant="ghost" size="sm" onClick={goBoard}>
            ← Board
          </Button>
        </header>
        <div className="flex-1 overflow-hidden">
          <ChatView
            key={activeSession}
            sessionId={activeSession}
            onSpawned={() => {}}
          />
        </div>
      </div>
    );
  } else {
    workArea = (
      <BoardView
        key={project}
        onOpenSession={setActiveSession}
        onReview={setReviewTask}
      />
    );
  }

  return (
    <AppShell
      serverStatus={status}
      project={project}
      onSwitchProject={() => void handleOpenProject()}
      onGoBoard={goBoard}
      boardActive={!activeSession && !reviewTask}
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
  );
}
