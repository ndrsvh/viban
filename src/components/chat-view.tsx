import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { Channel, invoke } from "@tauri-apps/api/core";

import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";
import type { AgentEvent } from "@/types/agent";

type Role = "user" | "assistant" | "tool" | "error";

interface Message {
  id: string;
  role: Role;
  text: string;
}

let messageCounter = 0;
const nextId = () => String(++messageCounter);

/** A minimal single-session chat over one Claude Code agent. */
export function ChatView() {
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const sessionId = useRef<string | null>(null);
  const channel = useRef<Channel<AgentEvent> | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  function push(role: Role, text: string) {
    const id = nextId();
    setMessages((prev) => [...prev, { id, role, text }]);
  }

  // Stable across renders: only touches state setters and refs.
  function handleEvent(event: AgentEvent) {
    switch (event.type) {
      case "assistant_text":
        push("assistant", event.text);
        break;
      case "tool_use":
        push("tool", `using ${event.name}`);
        break;
      case "error":
        push("error", event.message);
        setBusy(false);
        break;
      case "result":
        if (event.is_error) push("error", "the agent reported an error");
        setBusy(false);
        break;
      default:
        break;
    }
  }

  async function send() {
    const prompt = input.trim();
    if (!prompt || busy) return;
    setInput("");
    push("user", prompt);
    setBusy(true);
    try {
      if (sessionId.current === null) {
        const newChannel = new Channel<AgentEvent>();
        newChannel.onmessage = handleEvent;
        channel.current = newChannel;
        sessionId.current = await invoke<string>("spawn_session", {
          prompt,
          onEvent: newChannel,
        });
      } else {
        await invoke("send_message", {
          sessionId: sessionId.current,
          prompt,
        });
      }
    } catch (err) {
      push("error", String(err));
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
    <div className="flex h-screen w-screen flex-col bg-background text-foreground">
      <header className="border-b px-4 py-2 text-sm font-medium tracking-tight">
        viban
      </header>

      <ScrollArea className="flex-1">
        <div className="mx-auto flex max-w-2xl flex-col gap-3 p-4">
          {messages.length === 0 && (
            <p className="text-sm text-muted-foreground">
              Send a message to start a Claude Code session.
            </p>
          )}
          {messages.map((message) => (
            <MessageBubble key={message.id} message={message} />
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

function MessageBubble({ message }: { message: Message }) {
  return (
    <div
      className={cn(
        "flex",
        message.role === "user" ? "justify-end" : "justify-start",
      )}
    >
      <div
        className={cn(
          "max-w-[85%] rounded-md px-3 py-2 text-sm whitespace-pre-wrap",
          message.role === "user" && "bg-primary text-primary-foreground",
          message.role === "assistant" && "bg-muted",
          message.role === "tool" && "bg-muted text-xs text-muted-foreground",
          message.role === "error" && "bg-destructive/10 text-destructive",
        )}
      >
        {message.text}
      </div>
    </div>
  );
}
