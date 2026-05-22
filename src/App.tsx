import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import { BoardView } from "@/components/board-view";
import { ChatView } from "@/components/chat-view";
import { DiffView } from "@/components/diff-view";
import { Button } from "@/components/ui/button";
import type { Task } from "@/types/board";
import type { ServerHealth } from "@/types/server";

type Status = "connecting" | "ready" | "reconnecting";

// The server is polled continuously so the UI reflects sidecar restarts.
const POLL_INTERVAL_MS = 1500;

/** The last path segment of a project path, for display. */
function projectName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

export default function App() {
  // `undefined` while still loading; `null` when no project is chosen.
  const [project, setProject] = useState<string | null | undefined>(undefined);
  const [status, setStatus] = useState<Status>("connecting");
  const [activeSession, setActiveSession] = useState<string | null>(null);
  const [reviewTask, setReviewTask] = useState<Task | null>(null);
  const [projectError, setProjectError] = useState<string | null>(null);

  useEffect(() => {
    void invoke<string | null>("current_project")
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
        await invoke<ServerHealth>("server_health");
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
      const path = await invoke<string | null>("open_project");
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

  if (status !== "ready") {
    return (
      <main className="flex h-screen w-screen flex-col items-center justify-center gap-2 bg-background text-foreground">
        <h1 className="text-2xl font-medium tracking-tight">viban</h1>
        <p className="text-sm text-muted-foreground">
          {status === "connecting"
            ? "server: connecting…"
            : "server: reconnecting…"}
        </p>
      </main>
    );
  }

  if (reviewTask) {
    return (
      <div className="h-screen w-screen bg-background text-foreground">
        <DiffView
          key={reviewTask.id}
          task={reviewTask}
          onDone={() => setReviewTask(null)}
        />
      </div>
    );
  }

  if (activeSession) {
    return (
      <div className="flex h-screen w-screen flex-col bg-background text-foreground">
        <header className="flex items-center border-b px-2 py-1.5">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setActiveSession(null)}
          >
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
  }

  return (
    <div className="flex h-screen w-screen flex-col bg-background text-foreground">
      <header className="flex items-center justify-between border-b px-3 py-1.5">
        <h1 className="text-sm font-medium">{projectName(project)}</h1>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => void handleOpenProject()}
        >
          Switch project
        </Button>
      </header>
      {projectError && (
        <p className="border-b bg-destructive/10 px-3 py-1.5 text-xs text-destructive">
          {projectError}
        </p>
      )}
      <div className="flex-1 overflow-hidden">
        <BoardView
          key={project}
          onOpenSession={setActiveSession}
          onReview={setReviewTask}
        />
      </div>
    </div>
  );
}
