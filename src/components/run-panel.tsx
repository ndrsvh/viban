import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { Channel } from "@tauri-apps/api/core";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { rpc } from "@/lib/rpc";
import { cn } from "@/lib/utils";
import type { CommandOutput, OutputStream } from "@/types/exec";

interface RunPanelProps {
  /** The task whose worktree the command runs in. */
  taskId: string;
}

interface OutLine {
  id: number;
  stream: OutputStream;
  text: string;
}

let lineCounter = 0;

/** Runs a shell command in a task's worktree and streams the output, so an
 *  agent's changes can be verified (tests, lint, build) before review. */
export function RunPanel({ taskId }: RunPanelProps) {
  const [command, setCommand] = useState("");
  const [lines, setLines] = useState<OutLine[]>([]);
  const [running, setRunning] = useState(false);
  // `undefined` until a run finishes; `null` if the process was signalled.
  const [exitCode, setExitCode] = useState<number | null | undefined>(undefined);
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const channel = new Channel<CommandOutput>();
    channel.onmessage = (event) => {
      if (event.type === "line") {
        lineCounter += 1;
        const id = lineCounter;
        setLines((prev) => [
          ...prev,
          { id, stream: event.stream, text: event.text },
        ]);
      } else {
        setExitCode(event.code);
        setRunning(false);
      }
    };
    void rpc.watchRun(taskId, channel);
    return () => {
      void rpc.unwatchRun(taskId);
    };
  }, [taskId]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView();
  }, [lines]);

  async function run() {
    const cmd = command.trim();
    if (!cmd || running) return;
    setLines([]);
    setExitCode(undefined);
    setRunning(true);
    try {
      await rpc.runCommand(taskId, cmd);
    } catch (err) {
      setRunning(false);
      lineCounter += 1;
      setLines([{ id: lineCounter, stream: "stderr", text: String(err) }]);
    }
  }

  function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Enter") {
      event.preventDefault();
      void run();
    }
  }

  const started = running || lines.length > 0 || exitCode !== undefined;

  return (
    <div className="flex shrink-0 flex-col border-t">
      <div className="flex items-center gap-2 px-3 py-2">
        <Input
          value={command}
          onChange={(event) => setCommand(event.target.value)}
          onKeyDown={onKeyDown}
          placeholder="Run a command in the worktree, e.g. npm test"
          className="font-mono text-xs"
        />
        <Button
          size="sm"
          onClick={() => void run()}
          disabled={running || !command.trim()}
        >
          {running ? "Running…" : "Run"}
        </Button>
      </div>
      {started && (
        <div className="max-h-48 overflow-auto border-t bg-muted/30 px-3 py-2">
          <pre className="font-mono text-[11px] leading-relaxed whitespace-pre-wrap">
            {lines.map((line) => (
              <div
                key={line.id}
                className={cn(line.stream === "stderr" && "text-destructive")}
              >
                {line.text}
              </div>
            ))}
          </pre>
          {running && lines.length === 0 && (
            <p className="text-[11px] text-muted-foreground">working…</p>
          )}
          {exitCode !== undefined && (
            <p
              className={cn(
                "mt-1 text-[11px]",
                exitCode === 0 ? "text-emerald-600" : "text-destructive",
              )}
            >
              {exitCode === null ? "terminated" : `exited ${exitCode}`}
            </p>
          )}
          <div ref={bottomRef} />
        </div>
      )}
    </div>
  );
}
