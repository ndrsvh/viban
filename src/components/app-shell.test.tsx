import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { AppShell } from "@/components/app-shell";

const baseProps = {
  serverStatus: "ready" as const,
  project: "/home/me/myproject",
  onSwitchProject: vi.fn(),
  onGoBoard: vi.fn(),
  boardActive: true,
  taskList: <div />,
  inspector: null,
};

describe("AppShell", () => {
  it("renders the work-area children", () => {
    render(
      <AppShell {...baseProps}>
        <p>work area</p>
      </AppShell>,
    );
    expect(screen.getByText("work area")).toBeInTheDocument();
  });

  it("renders the task-list slot", () => {
    render(
      <AppShell {...baseProps} taskList={<p>task list</p>}>
        <div />
      </AppShell>,
    );
    expect(screen.getByText("task list")).toBeInTheDocument();
  });

  it("renders the inspector slot", () => {
    render(
      <AppShell {...baseProps} inspector={<p>inspector</p>}>
        <div />
      </AppShell>,
    );
    expect(screen.getByText("inspector")).toBeInTheDocument();
  });

  it("shows the project name and a connected status", () => {
    render(
      <AppShell {...baseProps}>
        <div />
      </AppShell>,
    );
    expect(screen.getByText("myproject")).toBeInTheDocument();
    expect(screen.getByText("Connected")).toBeInTheDocument();
  });

  it("reflects the server status in the status bar", () => {
    const { rerender } = render(
      <AppShell {...baseProps} serverStatus="connecting">
        <div />
      </AppShell>,
    );
    expect(screen.getByText(/^Connecting/)).toBeInTheDocument();

    rerender(
      <AppShell {...baseProps} serverStatus="reconnecting">
        <div />
      </AppShell>,
    );
    expect(screen.getByText(/^Reconnecting/)).toBeInTheDocument();
  });

  it("calls onGoBoard when the Board rail button is clicked", async () => {
    const user = userEvent.setup();
    const onGoBoard = vi.fn();
    render(
      <AppShell {...baseProps} onGoBoard={onGoBoard}>
        <div />
      </AppShell>,
    );
    await user.click(screen.getByRole("button", { name: "Board" }));
    expect(onGoBoard).toHaveBeenCalled();
  });

  it("calls onSwitchProject when the project button is clicked", async () => {
    const user = userEvent.setup();
    const onSwitchProject = vi.fn();
    render(
      <AppShell {...baseProps} onSwitchProject={onSwitchProject}>
        <div />
      </AppShell>,
    );
    await user.click(screen.getByRole("button", { name: "Switch project" }));
    expect(onSwitchProject).toHaveBeenCalled();
  });
});
