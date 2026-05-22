import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { Channel } from "@tauri-apps/api/core";

import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Textarea } from "@/components/ui/textarea";
import { rpc } from "@/lib/rpc";
import { cn } from "@/lib/utils";
import type { AgentEvent } from "@/types/agent";
import type { TokenUsage } from "@/types/session";

type Role = "user" | "assistant" | "tool" | "error";

interface Bubble {
  id: string;
  role: Role;
  text: string;
}

let bubbleCounter = 0;

function makeBubble(role: Role, text: string): Bubble {
  bubbleCounter += 1;
  return { id: `b${bubbleCounter}`, role, text };
}

function roleOf(raw: string): Role {
  return raw === "user" || raw === "assistant" || raw === "tool" || raw === "error"
    ? raw
    : "assistant";
}

/** Formats an integer with comma thousands separators — locale-independent,
 *  so it renders the same everywhere (and in tests). */
function grouped(value: number): string {
  return value.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

/** Claude Code's file-editing tools and the input key carrying the path. */
const EDIT_TOOLS: Record<string, "file_path" | "notebook_path"> = {
  Edit: "file_path",
  Write: "file_path",
  MultiEdit: "file_path",
  NotebookEdit: "notebook_path",
};

/** The path a `tool_use` event edited, for file-modifying tools only. */
function editedPath(event: AgentEvent): string | null {
  if (event.type !== "tool_use") return null;
  const key = EDIT_TOOLS[event.name];
  if (!key) return null;
  const input = event.input;
  if (typeof input !== "object" || input === null) return null;
  const value = (input as Record<string, unknown>)[key];
  return typeof value === "string" ? value : null;
}

interface ChatViewProps {
  /** The viban session id this view is bound to. */
  sessionId: string;
  /** Called once a brand-new session is spawned, so the sidebar refreshes. */
  onSpawned: () => void;
}

/** A session-scoped chat: loads history, streams live agent events, sends. */
export function ChatView({ sessionId, onSpawned }: ChatViewProps) {
  const [bubbles, setBubbles] = useState<Bubble[]>([]);
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  // Files the agent has edited this session — its footprint.
  const [files, setFiles] = useState<string[]>([]);
  // Accumulated token usage for the session.
  const [usage, setUsage] = useState<TokenUsage>({
    input_tokens: 0,
    output_tokens: 0,
  });
  // Whether the session already exists server-side (vs. a fresh, unspawned one).
  const startedRef = useRef(false);
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [bubbles]);

  // Open the session: subscribe to live events and load any history.
  useEffect(() => {
    let cancelled = false;

    const channel = new Channel<AgentEvent>();
    channel.onmessage = (event) => {
      switch (event.type) {
        case "assistant_text":
          setBubbles((prev) => [...prev, makeBubble("assistant", event.text)]);
          break;
        case "tool_use": {
          setBubbles((prev) => [
            ...prev,
            makeBubble("tool", `using ${event.name}`),
          ]);
          const path = editedPath(event);
          if (path) {
            setFiles((prev) =>
              prev.includes(path) ? prev : [...prev, path],
            );
          }
          break;
        }
        case "error":
          setBubbles((prev) => [...prev, makeBubble("error", event.message)]);
          setBusy(false);
          break;
        case "result": {
          if (event.is_error) {
            setBubbles((prev) => [
              ...prev,
              makeBubble("error", "the agent reported an error"),
            ]);
          }
          const turn = event.usage;
          if (turn) {
            setUsage((prev) => ({
              input_tokens: prev.input_tokens + turn.input_tokens,
              output_tokens: prev.output_tokens + turn.output_tokens,
            }));
          }
          setBusy(false);
          break;
        }
        default:
          break;
      }
    };

    void rpc.openSession(sessionId, channel);
    rpc
      .getSession(sessionId)
      .then((history) => {
        if (cancelled) return;
        startedRef.current = true;
        setBubbles(
          history.messages.map((message) => ({
            id: message.id,
            role: roleOf(message.role),
            text: message.content,
          })),
        );
        setFiles(history.files);
        setUsage(history.usage);
      })
      .catch(() => {
        // No row for this id yet — it is a brand-new session.
        if (!cancelled) startedRef.current = false;
      });

    return () => {
      cancelled = true;
      void rpc.closeSession(sessionId);
    };
  }, [sessionId]);

  async function send() {
    const prompt = input.trim();
    if (!prompt || busy) return;
    setInput("");
    setBubbles((prev) => [...prev, makeBubble("user", prompt)]);
    setBusy(true);
    try {
      if (startedRef.current) {
        await rpc.sendMessage(sessionId, prompt);
      } else {
        await rpc.spawnSession(sessionId, prompt);
        startedRef.current = true;
        onSpawned();
      }
    } catch (err) {
      setBubbles((prev) => [...prev, makeBubble("error", String(err))]);
      setBusy(false);
    }
  }

  function onKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void send();
    }
  }

  return (
    <div className="flex h-full flex-col">
      {files.length > 0 && (
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1 border-b px-4 py-1.5 text-xs text-muted-foreground">
          <span className="font-medium">
            Files touched ({files.length}):
          </span>
          {files.map((file) => (
            <span key={file} className="font-mono">
              {file}
            </span>
          ))}
        </div>
      )}
      {(usage.input_tokens > 0 || usage.output_tokens > 0) && (
        <div className="border-b px-4 py-1.5 text-xs text-muted-foreground">
          {`Tokens: ${grouped(usage.input_tokens)} in · ${grouped(usage.output_tokens)} out`}
        </div>
      )}
      <ScrollArea className="flex-1">
        <div className="mx-auto flex max-w-2xl flex-col gap-3 p-4">
          {bubbles.length === 0 && (
            <p className="text-sm text-muted-foreground">
              Send a message to start the conversation.
            </p>
          )}
          {bubbles.map((bubble) => (
            <MessageBubble key={bubble.id} bubble={bubble} />
          ))}
          {busy && <p className="text-sm text-muted-foreground">working…</p>}
          <div ref={bottomRef} />
        </div>
      </ScrollArea>

      <div className="border-t p-3">
        <div className="mx-auto flex max-w-2xl items-end gap-2">
          <Textarea
            value={input}
            onChange={(event) => setInput(event.target.value)}
            onKeyDown={onKeyDown}
            placeholder="Message Claude Code…"
            rows={1}
            className="max-h-40 min-h-10 resize-none"
          />
          <Button onClick={() => void send()} disabled={busy || !input.trim()}>
            Send
          </Button>
        </div>
      </div>
    </div>
  );
}

function MessageBubble({ bubble }: { bubble: Bubble }) {
  return (
    <div
      className={cn(
        "flex",
        bubble.role === "user" ? "justify-end" : "justify-start",
      )}
    >
      <div
        className={cn(
          "max-w-[85%] rounded-md px-3 py-2 text-sm whitespace-pre-wrap",
          bubble.role === "user" && "bg-primary text-primary-foreground",
          bubble.role === "assistant" && "bg-muted",
          bubble.role === "tool" && "bg-muted text-xs text-muted-foreground",
          bubble.role === "error" && "bg-destructive/10 text-destructive",
        )}
      >
        {bubble.text}
      </div>
    </div>
  );
}
