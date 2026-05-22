# UI competitor research — agentic development tools

- Date: 2026-05-22
- Status: Reference note (input to ADR-0004)

## Purpose

viban has shipped a large feature set — a Kanban board, worktree-per-attempt
isolation, multiple attempts per task, visual diff review, run-a-command,
session→file linkage, live task status, token usage, and checkpoints — but the
UI is a set of full-screen view swaps in `src/App.tsx` with no persistent
application shell. The main area renders the board *or* a chat *or* a diff
review; server status is itself a full-screen state. Most of the capability is
therefore hard to discover.

Before redesigning, we surveyed seven peer tools and the state of desktop
dev-tool UI design (2025–2026). This note records the findings. ADR-0004 turns
them into a decision.

## Tools surveyed

| Tool | Form | Shell / navigation | Isolation | Diff review | Status |
| --- | --- | --- | --- | --- | --- |
| Conductor (Melty Labs) | macOS app, closed | Persistent 3-pane: workspace sidebar \| chat \| diff+terminal | git worktree | PR-style, inline comments routed to agent | Active |
| Vibe Kanban (Bloop) | Web, Rust+TS | Split: Kanban board \| agent panel | git worktree | Line diff, inline comments | Community-maintained |
| opcode (getAsterisk) | Tauri 2 + React + shadcn | Full-screen view swaps + top tabs | none | Checkpoint-to-checkpoint diff | Abandoned |
| Crystal (stravu) | Electron + React + Zustand | Persistent project-tree sidebar + tabbed detail | git worktree | Full git diff, git-command preview | Deprecated |
| Nimbalyst (ex-Evergage) | Desktop + iOS | Multi-pane, Kanban-by-phase | git worktree (optional) | Inline red/green in WYSIWYG editors | Active |
| Claude Squad (smtg-ai) | Terminal TUI | List \| preview/diff, `tab` toggles | tmux + worktree | Text diff tab | Active (AGPL) |
| Sculptor (Imbue) | Desktop app | Chat + separate "Agent Tasks" panel | Docker container | — | Beta |

## Per-tool notes

### Conductor

The closest thing to a polished reference. A **persistent three-panel shell**:
a left sidebar listing workspaces (a single dev stream each, auto-named after
cities), a center Claude-Code-style chat, a right pane with a PR-style diff
viewer and an integrated terminal. The sidebar groups workspaces by status
(backlog / in progress / in review / done) — effectively a vertical Kanban in
the rail — and every row carries an at-a-glance GitHub status badge (merged,
CI failing, conflicts, "agent needs permission"). Review comments on diff
lines are routed back to the agent as attachments; Claude can post inline
comments too. Keyboard-driven (Cmd+Shift+N/D/P), Linear-like flat aesthetic,
nine syntax themes. Weaknesses: macOS-only, closed-source, setup-script
friction.

### Vibe Kanban

A browser-hosted **split**: classic five-column Kanban (To Do → In Progress →
In Review → Done → Cancelled) on the left, a streaming agent panel on the
right. Card position *is* the status. Moving a card spins up a worktree and
launches the agent; review happens in the In Review column with a line diff
and inline comments. Pioneered the **"Attempt" model** — multiple agent runs
on one task coexist for side-by-side comparison (this is exactly viban's
multiple-attempts feature). HN criticism: AI moves cards so fast that
intermediate columns feel redundant, and card creation adds friction versus
just chatting.

### opcode — the cautionary twin

Same stack as viban (Tauri 2 + React + TypeScript + Tailwind + shadcn/ui),
~21k stars, now abandoned. Its IA is **full-screen view swaps** driven from
`App.tsx` plus a top tab bar (Projects / Agents / Settings) — i.e. viban's
current mistake. Documented problems: scroll stutter, hidden/overlapping
menus, UI freezes on large pasted payloads, and **session state bleed**
(messages jumping between sessions). It has **no git worktree isolation** at
all. Its liked features are the **checkpoint timeline** and a **token/cost
usage dashboard**. The lesson: our stack is validated, but full-screen swaps
and unscoped streams are the failure mode to avoid.

### Crystal — the architectural cousin

Closest to viban's model: multiple parallel Claude Code sessions, each in its
own git worktree, Zustand for state. A **persistent left sidebar** (a
draggable project tree; projects expand into sessions, each row a color-coded
animated status badge) plus a main detail pane with a **tab system**:
`Output | Diff | Logs | Editor`. The diff view surfaces rebase/squash with
**git-command preview tooltips**. Sessions are AI-named from the first prompt.
Now deprecated (succeeded by Nimbalyst), which — together with opcode's
abandonment — leaves an opening.

### Nimbalyst

Crystal's successor, by the ex-Evergage team. A multi-pane workspace with
sessions organised on a **Kanban board by phase** (Backlog → Planning →
Implementing → Complete) — very close to viban. Its differentiator is WYSIWYG
editors where AI edits appear as **inline red/green diffs** you accept/reject
per hunk. Criticised as unsuitable for casual or non-git users — viban's
"Work without Git" mode hedges exactly this.

### Claude Squad & Sculptor

Claude Squad is a fast keyboard-driven TUI: session list on the left,
preview/diff on the right, `tab` toggles between them. Its text-only diff is
its weakest surface. Sculptor runs each agent in a Docker container (heavy on
laptops) and notably **separates the chat stream from an "Agent Tasks" panel**
— a to-do list of long jobs — and puts model/permission/effort/context-usage
controls in the input footer. Its "Pairing Mode" (one click to bring an
agent's changes into the user's real IDE) is a compelling later idea.

## Cross-cutting patterns to adopt

1. **Persistent app shell.** Every polished tool keeps a sidebar / nav and a
   status surface mounted; only the work area changes. Full-screen swaps
   (opcode, viban today) bury features.
2. **Status on the work item.** Conductor's per-row badges and Crystal's
   color-coded session dots make the list a live dashboard. viban's
   `AgentStatus` (amber/green/red) needs a place to live.
3. **Tabbed task detail.** Crystal's `Output | Diff | Logs | Editor` is the
   natural container for viban's chat / diff / run-panel / session-files.
4. **Diff review feeds the agent.** Conductor and Vibe Kanban turn an inline
   diff comment into agent feedback without leaving the UI.
5. **Multiple attempts compared side by side** (Vibe Kanban) — validates
   viban's already-shipped attempts model.
6. **Git-command preview before action** (Crystal) and **AI-named sessions
   from the first prompt** (Crystal, Conductor).
7. **Separate the activity feed from the chat** (Sculptor) — autonomous work
   and conversation have different cadences.

## Pitfalls to avoid

- **Full-screen view swaps with hidden menus** (opcode) — undiscoverable.
- **Scroll stutter / render-thread blocking on large payloads** (opcode).
- **Unscoped event streams → state bleed between sessions** (opcode). viban's
  topic-tagged `events.update` is the right defense; honor it strictly.
- **Too many Kanban columns** — AI moves cards fast; keep card creation light.
- **git-mandatory / expert-only UX** (Nimbalyst) — keep "Work without Git".
- **Cramped terminal-only diff** (Claude Squad) — CodeMirror merge view wins.
- **Heavy per-agent isolation** (Sculptor's containers) — worktrees are
  lighter; still watch parallel-agent count.

## Design trends 2025–2026

- **The "Linear aesthetic"** dominates dev tooling: flat surfaces, 1px
  hairline borders instead of shadows, restrained color with a single accent,
  dark-mode-first, tight typography.
- **OKLCH color tokens** are the standard (shadcn moved its theme to OKLCH in
  March 2025). viban's `index.css` is already OKLCH — in trend, but it is the
  unmodified default theme.
- **Cmd/Ctrl-K command palette** is the de facto navigation accelerator
  (Linear, Raycast, VS Code). It complements menus, it does not replace them.
- **Monospace accents** for paths, IDs, branch names, token counts.
- **Discrete status indicators** (colored dots) beat numeric percentages for
  at-a-glance reading; pair color with a text/`aria-label`.
- **Convergence:** flat borders, no `backdrop-filter`, no heavy shadows is
  *both* the Linear trend and the WebKitGTK performance requirement — the
  on-trend choice is the performant one.
- **shadcn primitives for the shell:** `Sidebar` (+ `SidebarProvider`,
  `SidebarRail`), `Resizable` (split panes), `Command` (palette), plus the
  Oct-2025 additions `Kbd`, `Item`, `Empty`.

## viban's competitive advantages

The market is favorable: Crystal and opcode are abandoned; Conductor is
macOS-only and closed.

1. **Three OSes from day one** — every competitor is weak or absent on
   Windows. The strongest differentiator.
2. **Planning board + worktree-per-attempt + multiple attempts** as one
   integrated loop — no competitor has the whole set.
3. **"Work without Git" mode** — removes the entry barrier Nimbalyst trips on.
4. **Client/server split → remote workspaces (WSL/SSH)** — designed in from
   day one; no competitor has it.
5. **Checkpoints** — the `feature-checkpoints` work targets opcode's
   most-liked capability.

The features exist. The missing piece is a shell that surfaces them.

## Sources

Conductor: conductor.build, docs.conductor.build/workflow, conductor.build/changelog,
news.ycombinator.com/item?id=45520043, thenewstack.io review.
Vibe Kanban: github.com/BloopAI/vibe-kanban, vibekanban.com,
news.ycombinator.com/item?id=44533004, elite-ai-assisted-coding.dev review.
opcode: github.com/getAsterisk/opcode, opcode.sh, deepwiki.com/getAsterisk/claudia,
news.ycombinator.com/item?id=44933255.
Crystal: github.com/stravu/crystal (incl. CLAUDE.md).
Nimbalyst: nimbalyst.com, nimbalyst.com/features, sitepoint.com writeup.
Claude Squad: github.com/smtg-ai/claude-squad.
Sculptor: imbue.com/sculptor, github.com/imbue-ai/sculptor/blob/main/docs/interface.md.
Design trends: ui.shadcn.com/docs/theming, ui.shadcn.com/docs/components/radix/sidebar,
ui.shadcn.com/docs/changelog/2025-10-new-components, blog.logrocket.com/ux-design/linear-design,
fuselabcreative.com/ui-design-for-ai-agents, deepwiki.com/zed-industries/zed.
