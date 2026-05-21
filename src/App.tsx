import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import type { ServerHealth } from "@/types/server";

type Status =
  | { state: "connecting" }
  | { state: "ok"; health: ServerHealth }
  | { state: "error"; message: string };

// The sidecar handshake runs asynchronously after the window opens, so the
// first few invokes can race ahead of it — retry briefly before giving up.
const RETRY_DELAY_MS = 500;
const MAX_ATTEMPTS = 20;

export default function App() {
  const [status, setStatus] = useState<Status>({ state: "connecting" });

  useEffect(() => {
    let cancelled = false;

    const attempt = async (n: number): Promise<void> => {
      try {
        const health = await invoke<ServerHealth>("server_health");
        if (!cancelled) setStatus({ state: "ok", health });
      } catch (err) {
        if (cancelled) return;
        if (n + 1 >= MAX_ATTEMPTS) {
          setStatus({ state: "error", message: String(err) });
          return;
        }
        setTimeout(() => void attempt(n + 1), RETRY_DELAY_MS);
      }
    };

    void attempt(0);
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <main className="flex h-screen w-screen flex-col items-center justify-center gap-2 bg-background text-foreground">
      <h1 className="text-2xl font-medium tracking-tight">viban</h1>
      <p className="text-sm text-muted-foreground">{describe(status)}</p>
    </main>
  );
}

function describe(status: Status): string {
  switch (status.state) {
    case "connecting":
      return "server: connecting…";
    case "ok":
      return `server: ok · v${status.health.version}`;
    case "error":
      return `server: error · ${status.message}`;
  }
}
