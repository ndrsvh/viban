import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";

import { ChatView } from "@/components/chat-view";
import type { AgentEvent } from "@/types/agent";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  // A stand-in for tauri's event Channel: the component sets `onmessage`,
  // and tests fire events by calling it.
  Channel: class {
    onmessage: ((message: unknown) => void) | null = null;
  },
}));

const invokeMock = vi.mocked(invoke);

type InvokeImpl = (
  command: string,
  args?: Record<string, unknown>,
) => Promise<unknown>;

/** Routes each invoked command through `impl`. */
function setInvoke(impl: InvokeImpl) {
  invokeMock.mockImplementation(impl as unknown as typeof invoke);
}

/** The event Channel the component handed to `open_session`. */
function openedChannel(): { onmessage: ((event: AgentEvent) => void) | null } {
  const call = invokeMock.mock.calls.find((entry) => entry[0] === "open_session");
  if (!call) throw new Error("open_session was never invoked");
  const args = call[1] as {
    onEvent: { onmessage: ((event: AgentEvent) => void) | null };
  };
  return args.onEvent;
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe("ChatView", () => {
  it("subscribes to the session and renders its history", async () => {
    setInvoke((command) => {
      if (command === "get_session") {
        return Promise.resolve({
          messages: [
            {
              id: "m1",
              session_id: "s1",
              role: "user",
              content: "hello there",
              created_at: 1,
              raw_json: null,
            },
            {
              id: "m2",
              session_id: "s1",
              role: "assistant",
              content: "hi back",
              created_at: 2,
              raw_json: null,
            },
          ],
          files: [],
        });
      }
      return Promise.resolve(undefined);
    });

    render(<ChatView sessionId="s1" onSpawned={vi.fn()} />);

    expect(await screen.findByText("hello there")).toBeInTheDocument();
    expect(screen.getByText("hi back")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith(
      "open_session",
      expect.objectContaining({ sessionId: "s1" }),
    );
  });

  it("spawns a fresh session on the first message", async () => {
    const user = userEvent.setup();
    const onSpawned = vi.fn();
    setInvoke((command) => {
      if (command === "get_session") {
        return Promise.reject(new Error("no such session yet"));
      }
      return Promise.resolve(undefined);
    });

    render(<ChatView sessionId="s-new" onSpawned={onSpawned} />);

    const textarea = await screen.findByPlaceholderText(
      "Message Claude Code…",
    );
    await user.type(textarea, "do a thing");
    await user.click(screen.getByRole("button", { name: "Send" }));

    await waitFor(() => expect(onSpawned).toHaveBeenCalled());
    expect(invokeMock).toHaveBeenCalledWith("spawn_session", {
      sessionId: "s-new",
      prompt: "do a thing",
    });
  });

  it("sends a follow-up message on an already-started session", async () => {
    const user = userEvent.setup();
    setInvoke((command) => {
      if (command === "get_session") {
        return Promise.resolve({
          messages: [
            {
              id: "m1",
              session_id: "s1",
              role: "user",
              content: "earlier message",
              created_at: 1,
              raw_json: null,
            },
          ],
          files: [],
        });
      }
      return Promise.resolve(undefined);
    });

    render(<ChatView sessionId="s1" onSpawned={vi.fn()} />);
    // Waiting for history confirms the session is marked as started.
    await screen.findByText("earlier message");

    await user.type(
      screen.getByPlaceholderText("Message Claude Code…"),
      "follow up",
    );
    await user.click(screen.getByRole("button", { name: "Send" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("send_message", {
        sessionId: "s1",
        prompt: "follow up",
      }),
    );
  });

  it("renders streamed agent events as they arrive", async () => {
    setInvoke((command) => {
      if (command === "get_session") {
        return Promise.resolve({ messages: [], files: [] });
      }
      return Promise.resolve(undefined);
    });

    render(<ChatView sessionId="s1" onSpawned={vi.fn()} />);
    await screen.findByText("Send a message to start the conversation.");

    const channel = openedChannel();
    act(() => {
      channel.onmessage?.({ type: "assistant_text", text: "streamed reply" });
    });
    expect(await screen.findByText("streamed reply")).toBeInTheDocument();

    act(() => {
      channel.onmessage?.({ type: "tool_use", name: "Read", input: {} });
    });
    expect(await screen.findByText("using Read")).toBeInTheDocument();
  });

  it("lists the files the session has touched", async () => {
    setInvoke((command) => {
      if (command === "get_session") {
        return Promise.resolve({ messages: [], files: ["src/main.rs"] });
      }
      return Promise.resolve(undefined);
    });

    render(<ChatView sessionId="s1" onSpawned={vi.fn()} />);
    // The footprint loaded from history.
    expect(await screen.findByText("src/main.rs")).toBeInTheDocument();

    // A live file-editing tool call adds another file.
    const channel = openedChannel();
    act(() => {
      channel.onmessage?.({
        type: "tool_use",
        name: "Edit",
        input: { file_path: "src/lib.rs" },
      });
    });
    expect(await screen.findByText("src/lib.rs")).toBeInTheDocument();
  });

  it("shows an error bubble when spawning fails", async () => {
    const user = userEvent.setup();
    setInvoke((command) => {
      if (command === "get_session") {
        return Promise.reject(new Error("no such session yet"));
      }
      if (command === "spawn_session") {
        return Promise.reject("spawn failed");
      }
      return Promise.resolve(undefined);
    });

    render(<ChatView sessionId="s-new" onSpawned={vi.fn()} />);
    await user.type(
      await screen.findByPlaceholderText("Message Claude Code…"),
      "trigger error",
    );
    await user.click(screen.getByRole("button", { name: "Send" }));

    expect(await screen.findByText("spawn failed")).toBeInTheDocument();
  });
});
