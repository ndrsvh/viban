# ADR-0004: Application shell and information architecture

- Status: Accepted
- Date: 2026-05-22

## Context

viban's MVP and post-MVP work shipped a broad feature set — a Kanban board,
worktree-per-attempt isolation (ADR-0002), visual diff review, run-a-command
in a worktree, session→file linkage, live task status, token usage, and
checkpoints. The frontend, however, never grew an information architecture to
match. `src/App.tsx` renders the UI as a set of mutually exclusive
full-screen states: a project picker, a server-status screen, the board, a
chat view, or a diff review. Opening a task replaces the whole window; there
is no persistent navigation, no home for cross-cutting context, and the
server connection status is itself a full-screen takeover.

The consequence is that most of the shipped capability is undiscoverable.
The run-command panel, the file footprint, attempt switching, and token
usage all live deep inside a view the user can only reach through a sequence
of full-screen swaps, with no affordance hinting they exist.

`docs/research/ui-competitor-research.md` surveyed seven peer tools. The
finding was unambiguous: every polished tool (Conductor, Crystal, Nimbalyst)
runs on a persistent application shell and swaps only its work area, while
the one tool that shares viban's exact stack and *also* uses full-screen view
swaps — opcode — is the category's cautionary tale: undiscoverable features,
session state bleed, now abandoned. viban is currently repeating opcode's
structural mistake.

## Decision

viban's frontend is restructured around a **persistent application shell**.
The shell chrome mounts once and stays; only the work area changes. The
layout is **master-detail**: a task list is always present, and a selected
task's workspace fills the main area.

```
┌──────────────────────────────────────────────────────────────────┐
│ ▤  │ Tasks             │  Main work area         │ Inspector       │
│    │ ▸ Fix bug     ●   │ ┌─────────────────────┐ │ Attempt   ▾     │
│    │ ▸ Add auth    ●   │ │ ← Board / Add auth  │ │ Status    ●     │
│    │ ▸ Refactor    ●   │ │ [Chat|Diff|Run|Files]│ │ Branch    …     │
│    │ ▸ …               │ │                     │ │ Tokens    …     │
│ ⚙  │                   │ └─────────────────────┘ │ Files     …     │
├────┴───────────────────┴─────────────────────────┴─────────────────┤
│ ● server: ready    │   project: viban    │   branch: feature-…     │
└──────────────────────────────────────────────────────────────────┘
 activity   task list         main work area          inspector
  rail      (master)            (detail)            (collapsible)
```

The shell has five regions plus a command palette:

1. **Activity rail** — a narrow, far-left icon rail of top-level sections.
   For the MVP: the board section, with Settings pinned at the bottom. It is
   the extension point for later top-level destinations.

2. **Task list panel** — the *master* of the master-detail layout: a
   persistent, resizable, collapsible navigator listing every task on the
   active board, grouped by column, each row carrying its live `AgentStatus`
   dot. The selected task is highlighted so the user can jump between sibling
   tasks without losing context. This panel is what the never-built "session
   sidebar" from the Phase 2 spec should have been — sessions belong to tasks
   (Phase 3), so tasks are the unit of navigation.

3. **Main work area** — the *detail*. It shows one of two things:
   - the **Kanban board** — the full drag-and-drop planning surface — when no
     task is open;
   - a **task detail** when a task is selected: a tab bar
     `Chat | Diff | Run | Files` over the task's active attempt. Chat is the
     agent conversation; Diff is the worktree review; Run is the
     run-a-command panel; Files is the session's edited-file footprint. One
     surface is shown at a time.

4. **Inspector** — a resizable, collapsible right panel holding context for
   the selected task: the attempt selector (ADR-0002), agent status, branch
   and worktree path, token usage, and the file-footprint summary.

5. **Status bar** — a thin bottom strip: server connection state, the current
   project, the active branch, and reserved space for the Phase 6 remote
   ("Remote: WSL/<distro>") badge. Server status stops being a full-screen
   takeover; transient connection loss degrades to a status-bar indicator.

6. **Command palette** — a `Cmd/Ctrl-K` overlay. A bare query navigates tasks
   and boards; a `>` prefix runs actions (create task, start session, run
   command, switch attempt). It complements the rail and menus; it does not
   replace them.

Two layout forks were decided with the project owner:

- **Task detail uses tabs**, not a split — `Chat | Diff | Run | Files`, one
  surface at a time. This matches Crystal (viban's closest architectural
  cousin), keeps each surface from being cramped, and is the lower-risk first
  redesign. A chat+diff split can be revisited by a later ADR.
- **The task list is always visible** (master-detail), not a full
  board↔detail swap. The task list panel persists; opening a task swaps only
  the main work area. The board remains a distinct planning view reached from
  the activity rail.

Visual language, building on the locked tech decisions in CLAUDE.md:

- Keep the **OKLCH** theme tokens already in `src/index.css`, but treat the
  current values as a starting point to be tuned, not the final palette.
- Both light and dark themes stay first-class; the app follows the OS
  preference by default and exposes a manual toggle.
- **Flat surfaces**: 1px hairline borders instead of shadows, no
  `backdrop-filter`, no large gradients. This is simultaneously the current
  "Linear" design idiom and the WebKitGTK performance requirement already in
  CLAUDE.md — the on-trend choice and the performant choice coincide.
- **Monospace** type for paths, branch names, session/attempt IDs, and token
  counts.
- Status is a discrete colored dot (amber pulsing = running, green = done,
  red = failed) paired with an `aria-label` — never color alone.

Implementation notes:

- The shell is composed from shadcn primitives — `Sidebar` (with
  `SidebarProvider` / `SidebarRail`), `Resizable` for the panel splits, and
  `Command` for the palette — added via `npx shadcn add`. These become owned
  source under `src/components/ui/`, per CLAUDE.md.
- Panel sizes and collapsed/expanded state are persisted so the layout
  survives a restart.
- **This is a frontend-only restructuring.** No server method, notification
  topic, or wire type changes: `boards.get` already returns the `statuses`
  map, `sessions.get` already returns `files` and `usage`, and the
  `run:<task_id>` topic already exists. The redesign rearranges how existing
  data is surfaced; it does not touch `viban-core` or `viban-server`.

## Consequences

- Every shipped feature gains a fixed, discoverable home: the run panel and
  file footprint become tabs, attempt switching and token usage move into the
  always-visible inspector, and agent status shows on every task row.
- `src/App.tsx` stops being a switch over full-screen states; it renders the
  shell, and the views (`BoardView`, `ChatView`, `DiffView`, `RunPanel`)
  become content hosted inside the work area and the tabs rather than
  top-level screens.
- New shadcn components (`sidebar`, `resizable`, `command`) enter
  `src/components/ui/`.
- The work is incremental and can land across several PRs (shell skeleton →
  task list panel → tabbed task detail → inspector → status bar → command
  palette) without a flag day, because no backend change gates it.
- CLAUDE.md has no UI/IA section; it should gain a short one, or a pointer to
  this ADR, so the navigation model is documented alongside the locked tech
  decisions. (Follow-up, not done here.)
- The decision is reversible per region: the tabbed task detail can become a
  split, and the inspector or activity rail can be dropped, by a later ADR
  with no protocol impact.
