import type { ReactNode } from "react";
import { FolderOpen, LayoutGrid, type LucideIcon } from "lucide-react";

import { cn } from "@/lib/utils";

/** Server connection state, surfaced in the status bar. */
export type ServerStatus = "connecting" | "ready" | "reconnecting";

interface AppShellProps {
  /** Server connection state, shown in the status bar. */
  serverStatus: ServerStatus;
  /** The open project's path; its last segment names the project. */
  project: string;
  /** Switches to a different project. */
  onSwitchProject: () => void;
  /** Returns to the board, clearing any open session or review. */
  onGoBoard: () => void;
  /** Whether the board is the active destination (vs. a session or review). */
  boardActive: boolean;
  /** The persistent task-list panel, rendered between the rail and work area. */
  taskList: ReactNode;
  /** The work-area content. */
  children: ReactNode;
}

/** The last path segment of a project path, for display. */
function projectName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

const SERVER_STATUS: Record<ServerStatus, { label: string; dot: string }> = {
  connecting: { label: "Connecting…", dot: "bg-amber-500" },
  ready: { label: "Connected", dot: "bg-emerald-500" },
  reconnecting: { label: "Reconnecting…", dot: "bg-amber-500 animate-pulse" },
};

/**
 * The persistent application shell: a left activity rail, the work area, and a
 * bottom status bar. The chrome stays mounted across navigation — only the
 * `children` work area swaps — so every feature has a fixed, discoverable home
 * (see docs/decisions/0004-app-shell-and-information-architecture.md). This is
 * the shell skeleton; the task-list panel and inspector land in later work.
 */
export function AppShell({
  serverStatus,
  project,
  onSwitchProject,
  onGoBoard,
  boardActive,
  taskList,
  children,
}: AppShellProps) {
  const server = SERVER_STATUS[serverStatus];
  return (
    <div className="flex h-screen w-screen flex-col bg-background text-foreground">
      <div className="flex flex-1 overflow-hidden">
        <nav
          aria-label="Primary"
          className="flex w-12 shrink-0 flex-col items-center gap-1 border-r py-2"
        >
          <div
            aria-hidden
            className="mb-1 flex h-8 w-8 select-none items-center justify-center rounded-md bg-primary text-sm font-semibold text-primary-foreground"
          >
            v
          </div>
          <RailButton
            icon={LayoutGrid}
            label="Board"
            active={boardActive}
            onClick={onGoBoard}
          />
        </nav>
        {taskList}
        <div className="flex-1 overflow-hidden">{children}</div>
      </div>
      <footer className="flex h-7 shrink-0 items-center gap-3 border-t px-3 text-xs text-muted-foreground">
        <span className="flex items-center gap-1.5">
          <span
            aria-hidden
            className={cn("h-2 w-2 rounded-full", server.dot)}
          />
          <span>{server.label}</span>
        </span>
        <button
          type="button"
          aria-label="Switch project"
          onClick={onSwitchProject}
          className="flex items-center gap-1 rounded px-1 py-0.5 hover:bg-accent hover:text-foreground"
        >
          <FolderOpen className="h-3 w-3" />
          <span className="font-medium">{projectName(project)}</span>
        </button>
      </footer>
    </div>
  );
}

interface RailButtonProps {
  icon: LucideIcon;
  label: string;
  active: boolean;
  onClick: () => void;
}

/** A single icon button in the activity rail. */
function RailButton({ icon: Icon, label, active, onClick }: RailButtonProps) {
  return (
    <button
      type="button"
      aria-label={label}
      aria-current={active ? "page" : undefined}
      title={label}
      onClick={onClick}
      className={cn(
        "flex h-9 w-9 items-center justify-center rounded-md",
        active
          ? "bg-accent text-accent-foreground"
          : "text-muted-foreground hover:bg-accent hover:text-foreground",
      )}
    >
      <Icon className="h-5 w-5" />
    </button>
  );
}
