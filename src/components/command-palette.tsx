import { useMemo, useState } from "react";

import {
  Command,
  CommandDialog,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { useBoardStore } from "@/stores/useBoardStore";
import type { Task } from "@/types/board";

interface CommandPaletteProps {
  /** Whether the palette is open. */
  open: boolean;
  /** Opens or closes the palette. */
  onOpenChange: (open: boolean) => void;
  /** Navigates to a task. */
  onSelectTask: (task: Task) => void;
  /** Returns to the board. */
  onGoBoard: () => void;
  /** Switches the project. */
  onSwitchProject: () => void;
}

interface Action {
  id: string;
  label: string;
  run: () => void;
}

/**
 * The Cmd/Ctrl-K command palette (ADR-0004). A bare query searches tasks; a
 * `>` prefix switches to actions. It complements the rail and panels — a
 * keyboard accelerator, not a replacement for them. Its own filtering is used
 * (`shouldFilter={false}`) so the `>` prefix can route between the two modes.
 */
export function CommandPalette({
  open,
  onOpenChange,
  onSelectTask,
  onGoBoard,
  onSwitchProject,
}: CommandPaletteProps) {
  const [query, setQuery] = useState("");
  const tasks = useBoardStore((state) => state.tasks);

  const actionMode = query.startsWith(">");
  const term = (actionMode ? query.slice(1) : query).trim().toLowerCase();

  const actions = useMemo<Action[]>(
    () => [
      { id: "board", label: "Go to board", run: onGoBoard },
      { id: "switch-project", label: "Switch project", run: onSwitchProject },
    ],
    [onGoBoard, onSwitchProject],
  );

  const shownTasks = actionMode
    ? []
    : Object.values(tasks).filter((task) =>
        task.title.toLowerCase().includes(term),
      );
  const shownActions = actionMode
    ? actions.filter((action) => action.label.toLowerCase().includes(term))
    : [];
  const empty = shownTasks.length === 0 && shownActions.length === 0;

  const change = (next: boolean) => {
    if (!next) setQuery("");
    onOpenChange(next);
  };

  const pickTask = (task: Task) => {
    change(false);
    onSelectTask(task);
  };
  const pickAction = (action: Action) => {
    change(false);
    action.run();
  };

  return (
    <CommandDialog open={open} onOpenChange={change}>
      <Command shouldFilter={false}>
        <CommandInput
          value={query}
          onValueChange={setQuery}
          placeholder="Search tasks…  ( type > for actions )"
        />
        <CommandList>
          {empty && (
            <p className="py-6 text-center text-sm text-muted-foreground">
              No results.
            </p>
          )}
          {shownTasks.length > 0 && (
            <CommandGroup heading="Tasks">
              {shownTasks.map((task) => (
                <CommandItem
                  key={task.id}
                  value={task.id}
                  onSelect={() => pickTask(task)}
                >
                  {task.title}
                </CommandItem>
              ))}
            </CommandGroup>
          )}
          {shownActions.length > 0 && (
            <CommandGroup heading="Actions">
              {shownActions.map((action) => (
                <CommandItem
                  key={action.id}
                  value={action.id}
                  onSelect={() => pickAction(action)}
                >
                  {action.label}
                </CommandItem>
              ))}
            </CommandGroup>
          )}
        </CommandList>
      </Command>
    </CommandDialog>
  );
}
