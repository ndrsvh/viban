import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import { ChatView } from "@/components/chat-view";
import { SessionSidebar } from "@/components/session-sidebar";
import type { ServerHealth } from "@/types/server";
import type { Session } from "@/types/session";

type Status = "connecting" | "ready" | "reconnecting";

// The server is polled continuously so the UI reflects sidecar restarts.
const POLL_INTERVAL_MS = 1500;

export default function App() {
  const [status, setStatus] = useState<Status>("connecting");
  const [sessions, setSessions] = useState<Session[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);

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
        // Tolerate one transient miss before showing a degraded state.
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

  const refreshSessions = useCallback(async () => {
    try {
      const result = await invoke<{ sessions: Session[] }>("list_sessions");
      setSessions(result.sessions);
    } catch {
      // A failed list just means the server blipped; the poll surfaces it.
    }
  }, []);

  useEffect(() => {
    if (status === "ready") void refreshSessions();
  }, [status, refreshSessions]);

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

  return (
    <div className="flex h-screen w-screen bg-background text-foreground">
      <SessionSidebar
        sessions={sessions}
        selectedId={selectedId}
        onSelect={setSelectedId}
        onNew={() => setSelectedId(crypto.randomUUID())}
      />
      <main className="flex-1 overflow-hidden">
        {selectedId ? (
          <ChatView
            key={selectedId}
            sessionId={selectedId}
            onSpawned={refreshSessions}
          />
        ) : (
          <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
            Select a session or start a new one.
          </div>
        )}
      </main>
    </div>
  );
}
