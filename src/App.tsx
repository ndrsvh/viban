import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import { BoardView } from "@/components/board-view";
import { ChatView } from "@/components/chat-view";
import { Button } from "@/components/ui/button";
import type { ServerHealth } from "@/types/server";

type Status = "connecting" | "ready" | "reconnecting";

// The server is polled continuously so the UI reflects sidecar restarts.
const POLL_INTERVAL_MS = 1500;

export default function App() {
  const [status, setStatus] = useState<Status>("connecting");
  const [activeSession, setActiveSession] = useState<string | null>(null);

  useEffect(() => {
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
  }, []);

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
    <div className="h-screen w-screen bg-background text-foreground">
      <BoardView onOpenSession={setActiveSession} />
    </div>
  );
}
