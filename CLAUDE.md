# viban — Project Specification

## What this is

A desktop application for orchestrating Claude Code CLI sessions through a visual workspace. Users plan work on a Kanban board, each task spawns its own Claude Code session in an isolated git worktree, and changes are reviewed visually before being merged back.

The audience is solo developers and small teams doing agentic development — people who already use Claude Code but want planning, parallel execution, and review surfaces that the terminal can't provide.

**Long-term goal: remote workspaces.** The product should let users open projects that live in WSL (on Windows), on an SSH-accessible remote host, or in a dev container — with the UI running locally and the heavy lifting (agent processes, git, file watching, DB) running where the code lives. Similar in spirit to VS Code Remote-WSL and JetBrains Gateway. This shapes architecture from day 1 (client/server split, headless server binary), but actual remote transports land in Phase 6+. MVP runs local-only.

## Locked tech decisions

Do not propose alternatives to these. They were chosen after evaluating Electron, Flutter, and GPUI. The reasoning is captured in `docs/decisions/`. If you think a decision is wrong, write an ADR proposing the change — don't silently rewrite.

- **Desktop framework**: Tauri 2 (stable since Oct 2024)
- **Frontend**: React 18 + TypeScript (strict mode) + Vite
- **State management**: Zustand
- **Drag-and-drop**: `@dnd-kit`
- **Code editor / diff view**: CodeMirror 6 (NOT Monaco — too heavy)
- **Terminal emulator** (for Phase 5+): xterm.js + `portable-pty` (Rust crate)
- **Database**: SQLite via `rusqlite` with the `bundled` feature
- **Async runtime**: tokio
- **Git**: shell out to `git` CLI. Do NOT use `git2-rs` — it's painful cross-platform.
- **Styling**: Tailwind CSS, kept flat (no heavy gradients/blur/shadows — they break or stutter on WebKitGTK)
- **Component library**: shadcn/ui (Radix Primitives under the hood, copy-paste source model, no runtime dependency)
- **Class utilities**: `clsx` + `tailwind-merge`, exposed via a `cn()` helper in `src/lib/utils.ts` (standard shadcn pattern)
- **Variants**: `class-variance-authority` (CVA) for component variant APIs
- **Icons**: lucide-react (canonical icon set for shadcn)
- **Process management for agents**: `tokio::process::Command` + `tauri::ipc::Channel` for streaming

## Target platforms

macOS, Linux, Windows from day 1. CI matrix must run on all three for every PR. iOS / Android are explicitly out of scope for now.

## Architecture

Three-tier, structured as client/server from day 1 even when running locally. This enables Phase 6 (WSL) and Phase 7+ (SSH, dev containers) without ripping out the foundation.

```
┌──────────────────────────────────────────────┐
│  Tauri shell (src-tauri)                     │
│  Window, OS chrome, sidecar lifecycle        │
│  WebSocket client (JSON-RPC)                 │
│  Thin #[tauri::command] proxies              │
├──────────────────────────────────────────────┤
│  React frontend (src/)                       │
│  UI, state, calls tauri::invoke()            │
└──────────────────────────────────────────────┘
                    │
                    │ JSON-RPC 2.0 over WebSocket
                    │ Local:  127.0.0.1:<random>
                    │ WSL:    localhost:<port> (auto-forwarded)
                    │ SSH:    tunneled through ssh
                    ▼
┌──────────────────────────────────────────────┐
│  viban-server (separate binary)          │
│  WebSocket listener + JSON-RPC router        │
│  Spawns claude subprocesses                  │
│  Git operations (shell out)                  │
│  SQLite (rusqlite bundled)                   │
│  File watching                               │
│  Depends on: viban-core                  │
├──────────────────────────────────────────────┤
│  viban-core (library crate)              │
│  Domain types (Task, Session, AgentEvent...) │
│  Business logic, transport-agnostic          │
│  NO Tauri, NO WebSocket, NO net code         │
└──────────────────────────────────────────────┘
```

Rules:
- **viban-core** is a pure library. Knows nothing about Tauri, transport, or being remote. Testable standalone with `cargo test -p viban-core`.
- **viban-server** is a standalone binary. Runs independently: `viban-server --port 7400 --workspace /path`. This is what ships into WSL or onto a remote host in Phase 6+.
- **src-tauri** is the UI host. Spawns viban-server as a sidecar in local mode, connects to a remote viban-server in remote mode. Same protocol both ways.
- For high-frequency streams (agent stdout → UI), the server emits JSON-RPC notifications. src-tauri forwards them through `tauri::ipc::Channel<T>` to the React side.
- Errors at the WebSocket boundary serialize as JSON-RPC error objects. Errors at the Tauri command boundary serialize as `String` (Tauri requirement).

## Server protocol

JSON-RPC 2.0 over WebSocket. Same family as LSP and MCP — pick what the ecosystem already knows.

### Method naming

Namespaced as `<area>.<action>`:
- `tasks.create`, `tasks.update`, `tasks.list`, `tasks.delete`, `tasks.reorder`
- `sessions.create`, `sessions.resume`, `sessions.send_message`, `sessions.cancel`
- `git.worktree_create`, `git.worktree_remove`, `git.diff`, `git.commit`
- `agents.spawn`, `agents.cancel`, `agents.status`
- `boards.create`, `boards.get`, `boards.list`

### Streaming events

Long-running operations (agent runs, file watch, live task status) emit
JSON-RPC `events.update` notifications, each tagged with a **topic** string
and a JSON **payload**. The client subscribes by topic — the transport is not
tied to sessions, so any feature can push on its own topic.

1. Client calls `agents.spawn` → server returns `{ session_id }`
2. Server emits `events.update` notifications with `{ topic, payload }`. For an
   agent run the topic is the session id and the payload is an `AgentEvent`
3. Client calls `agents.cancel` → server tears down, stops emitting

Server-side, the per-connection `Context` carries an `EventSink`; any handler
or spawned task can call `events.emit(topic, payload)` to push a notification.

### Bootstrapping (local mode)

On Tauri shell startup:
1. Generate a 32-byte random auth token
2. Pick a free port (bind `127.0.0.1:0`, read assigned port) OR let server pick with `--port 0`
3. Spawn `viban-server` as Tauri sidecar with `--port <n> --workspace <path>` and env `VIBAN_AUTH_TOKEN=<token>`
4. Wait for server to print `{"ready":true,"port":<n>}` on first stdout line
5. Open WebSocket to `127.0.0.1:<n>`, send token as first message, server validates

### Auth

Local mode: token-based handshake described above. Prevents other processes on the same machine from connecting.
Remote mode (Phase 6+): same token over the secure transport (WSL is trusted localhost, SSH provides its own encryption, TLS for anything else).

## Project structure

Cargo workspace at the root with three Rust crates plus the Tauri shell. The frontend lives in `src/` as usual.

```
viban/
├── Cargo.toml                         # workspace manifest
├── crates/
│   ├── viban-core/                # pure logic library — no Tauri, no transport
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── types.rs               # Task, Session, AgentEvent, Board, ...
│   │   │   ├── agents/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── claude_code.rs     # subprocess wrapper, spawn config
│   │   │   │   └── stream.rs          # NDJSON parser
│   │   │   ├── git/
│   │   │   │   ├── mod.rs
│   │   │   │   └── worktree.rs
│   │   │   ├── tasks/
│   │   │   ├── sessions/
│   │   │   └── db/
│   │   │       ├── mod.rs
│   │   │       ├── schema.rs
│   │   │       └── migrations.rs
│   │   └── Cargo.toml
│   └── viban-server/              # binary: exposes core over JSON-RPC/WebSocket
│       ├── src/
│       │   ├── main.rs                # entry: arg parsing, start server, print ready
│       │   ├── rpc.rs                 # JSON-RPC method routing
│       │   ├── ws.rs                  # WebSocket transport
│       │   └── auth.rs                # token check
│       └── Cargo.toml
├── src-tauri/                         # Tauri shell + frontend host
│   ├── src/
│   │   ├── main.rs
│   │   ├── lib.rs
│   │   ├── sidecar.rs                 # spawn/manage viban-server
│   │   ├── client.rs                  # JSON-RPC client over WebSocket
│   │   └── commands/                  # thin proxies, NO business logic
│   │       ├── mod.rs
│   │       ├── agents.rs
│   │       ├── tasks.rs
│   │       ├── sessions.rs
│   │       └── git.rs
│   ├── binaries/                      # built viban-server lands here
│   │   └── viban-server-<target-triple>{.exe}
│   ├── Cargo.toml
│   └── tauri.conf.json                # externalBin: ["binaries/viban-server"]
├── src/                               # React frontend (same as before)
│   ├── main.tsx
│   ├── App.tsx
│   ├── components/
│   │   ├── ui/                        # shadcn components
│   │   └── ...
│   ├── pages/
│   ├── stores/
│   ├── hooks/
│   ├── lib/
│   │   └── utils.ts                   # cn() helper
│   └── types/                         # mirrors of viban-core types
├── docs/
│   └── decisions/                     # ADRs
├── .github/workflows/
│   └── ci.yml                         # builds server first, then tauri app
├── package.json
├── tsconfig.json
└── README.md
```

Dependency graph (enforced by Cargo.toml):
- `viban-core` → no internal deps
- `viban-server` → `viban-core`
- `src-tauri` → `viban-core` (for types only) — does NOT depend on `viban-server` (it's a client)

## Claude Code CLI integration

This is the core of the product. Get it right before building everything around it.

### Invocation modes

**Bidirectional streaming session** (primary mode for the chat UI):
```bash
claude \
  --input-format stream-json \
  --output-format stream-json \
  --verbose \
  --permission-mode acceptEdits
```
Writes newline-delimited JSON to stdin, reads NDJSON from stdout. Multi-turn.

**One-shot mode** (for short batch tasks like commit message generation):
```bash
claude -p "<prompt>" \
  --output-format stream-json \
  --verbose \
  --allowedTools "Read,Edit,Bash"
```

### Session resumption

Claude Code returns a `session_id` in the initial JSON event. Persist it. To resume:
```bash
claude --resume <session_id>
```

### Permission modes

Three values matter for `--permission-mode`:
- `default` — prompts per action (only useful interactively)
- `acceptEdits` — auto-accept file edits, but not bash commands. Sensible default.
- `bypassPermissions` — accept everything. Risky. Opt-in per task, never default.

The UI exposes this per task. App default is `acceptEdits`.

### Tool allowlist

Use `--allowedTools` to constrain capability:
- Read-only analysis: `"Read,Grep,Glob"`
- Standard coding task: `"Read,Edit,Write,Bash,Grep,Glob"`
- Refactor without shell: omit `Bash`

### Spawning from Rust (canonical shape)

```rust
use tokio::process::Command;
use std::process::Stdio;

let mut child = Command::new("claude")
    .args([
        "--input-format", "stream-json",
        "--output-format", "stream-json",
        "--verbose",
        "--permission-mode", "acceptEdits",
    ])
    .current_dir(&worktree_path)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true)
    .spawn()?;
```

Read stdout via `BufReader::lines().next_line()` in a loop, parse each line as JSON, forward to frontend via `Channel<AgentEvent>`. Don't load whole lines into memory — use streaming.

On Windows, also set `CREATE_NO_WINDOW` to avoid a console flash:
```rust
#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(windows)]
cmd.creation_flags(CREATE_NO_WINDOW);
```

## MVP phases

Do these in order. Do not jump ahead. Each phase ends in a working, committable state. Open a PR per phase; review before merging.

### Phase 1 — Skeleton + Claude Code subprocess

**Goal**: open the app, type a prompt, see the streaming response from Claude Code in a chat view.

- [ ] `npm create tauri-app@latest` with React-TS + Vite template
- [ ] Restructure into Cargo workspace: create `crates/viban-core` and `crates/viban-server`, make `src-tauri` a workspace member. Update root `Cargo.toml` with `[workspace]` and members list.
- [ ] Configure Tailwind CSS (flat config, no fancy plugins)
- [ ] Initialize shadcn/ui: `npx shadcn@latest init` (choose: TypeScript, Tailwind, CSS variables for theming, `@/` alias)
- [ ] Install initial shadcn components for Phase 1: `button`, `input`, `textarea`, `scroll-area`, `dialog`, `dropdown-menu`, `tooltip`, `separator`, `card`
- [ ] GitHub Actions CI matrix: `macos-latest`, `ubuntu-22.04`, `windows-latest`. CI builds `viban-server` first with proper target-triple suffix, copies into `src-tauri/binaries/`, then runs `cargo tauri build`.
- [ ] `viban-core`: `AgentEvent` enum, `Session` struct, `spawn_claude(workspace, opts)` function using `tokio::process`, NDJSON parser. Pure logic, no Tauri.
- [ ] `viban-server`: WebSocket listener via `tokio-tungstenite`, JSON-RPC router (consider `jsonrpsee` or hand-rolled — start simple). Methods: `agents.spawn`. Notification: `events.update`. CLI args: `--port`, `--workspace`. Auth via `VIBAN_AUTH_TOKEN` env.
- [ ] `viban-server`: on startup, print `{"ready":true,"port":<n>}` to stdout as the first line, then continue logging via `tracing` to stderr.
- [ ] `src-tauri`: register `viban-server` as `externalBin` in `tauri.conf.json`. Add capability for sidecar shell exec.
- [ ] `src-tauri`: on `setup()`, generate auth token, spawn sidecar, wait for ready line, open WebSocket, store the JSON-RPC client in app state.
- [ ] `src-tauri`: `#[tauri::command] spawn_session(...)` — thin proxy that calls `agents.spawn` over WebSocket, returns `subscription_id`. Stream `events.update` notifications through `Channel<AgentEvent>` to the frontend.
- [ ] Frontend: server connection status line (connecting / ok / error) backed by a `server.health` JSON-RPC round-trip — proves the sidecar pipe end to end before the chat UI exists
- [ ] Frontend: minimal chat view (textarea + scrolling message list) using shadcn `ScrollArea` + `Textarea` + `Button`
- [ ] Frontend: receive `Channel<AgentEvent>`, render assistant text incrementally
- [ ] Handle subprocess deaths: if claude dies, surface error; if viban-server dies, attempt restart with backoff, surface state in UI
- [ ] Smoke test on all three OSes locally before pushing

**Done when**: CI green on all 3 OSes. User can launch the app, send "hello", see streaming response. viban-server starts and stops cleanly with the Tauri shell. Force-killing viban-server while the app is running results in graceful UI degradation + restart attempt, not a crash.

### Phase 2 — Session persistence

**Goal**: sessions survive app restart, can be listed and resumed.

- [ ] SQLite schema:
  - `sessions(id TEXT PK, claude_session_id TEXT NULL, title TEXT, created_at INT, project_path TEXT)`
  - `messages(id TEXT PK, session_id TEXT FK, role TEXT, content TEXT, created_at INT, raw_json TEXT)`
- [ ] Manual migration runner — single `migrations` table tracking applied version
- [ ] Persist every agent event to `messages` table
- [ ] Capture `session_id` from the Claude Code init event, store on `sessions`
- [ ] Sidebar listing sessions, click → load message history
- [ ] On resume, spawn `claude --resume <claude_session_id>`
- [ ] Title generation: first user prompt truncated to ~50 chars

**Done when**: close app, reopen, sessions are still there, can resume one and continue the conversation.

### Phase 3 — Kanban board + tasks

**Goal**: sessions belong to tasks. Tasks live on a board.

- [ ] DB:
  - `boards(id, name, project_path)`
  - `columns(id, board_id, name, position)`
  - `tasks(id, column_id, title, description, position, session_id NULL)`
- [ ] Default columns on new board: Backlog → In Progress → Review → Done
- [ ] Kanban UI with `@dnd-kit/sortable` (vertical column lists, horizontal column row)
- [ ] CRUD: create / edit / delete tasks
- [ ] Drag between columns + reorder within column, persist on drop
- [ ] "Start session" button on task → creates session, links it
- [ ] Click task with session → opens chat view scoped to that task

**Done when**: can plan tasks visually, drag them around, start an agent session from a task, leave and come back.

### Phase 4 — Git worktree per task

**Goal**: each task runs its agent in an isolated worktree so parallel sessions don't collide.

- [ ] On board creation: user picks **any** folder as the project root. It need
  not be a git repository — viban initializes git on demand (`git init` + an
  initial commit, after a confirmation dialog) the first time a task needs a
  worktree.
- [ ] On "start session" for a task:
  - if the project folder is not yet a git repo with a commit, ask the user to
    confirm initializing it
  - run `git worktree add <worktree> -b viban/<task-slug>`, where `<worktree>`
    is in viban's data directory, **not** the project folder (see ADR-0003)
  - spawn `claude` with `current_dir` = worktree path
- [ ] On task moved to Done: optional "merge & cleanup" — merge branch into base, then `git worktree remove`
- [ ] On task cancelled / deleted: `git worktree remove --force` + delete branch
- [ ] UI: show branch name + worktree path on the task card

**Done when**: two tasks can have agents running in parallel without seeing each other's edits.

### Phase 5 — Diff review

**Goal**: after an agent finishes, see what changed in its worktree, accept or reject.

- [ ] Detect agent-done signal from stream-json
- [ ] Run `git diff` in the worktree, parse into per-file hunks
- [ ] UI: file tree of changed files + CodeMirror 6 merge view per file
- [ ] Accept all → `git add . && git commit -m "<task title>"`, move task to Review
- [ ] Reject all → `git restore .`, move task back to In Progress
- [ ] Per-file accept (only if it stays simple): selective `git restore <file>` for rejected files

**Done when**: full workflow — plan → spawn → review → commit — works end to end on a real project.

### Phase 6 — WSL remote workspaces (Windows)

**Goal**: on Windows, open a project that lives inside a WSL2 distro. Agents, git, and file watching happen in WSL where the code is; UI runs on the Windows host. This is the simplest non-local target — WSL2 auto-forwards localhost between Windows and the distro, so no SSH or port forwarding logic is needed.

- [ ] UI: "Open in WSL" action in the project picker. List distros via `wsl.exe -l -q --running`, plus an option to start a stopped one.
- [ ] WSL filesystem picker: browse `\\wsl$\<distro>\...` from Windows for selecting the workspace path. Convert to POSIX path before sending to server.
- [ ] Server installation in distro: check for `viban-server` on PATH inside the distro. If missing, offer two paths to the user:
  - install via `cargo install viban-server` (requires Rust toolchain in distro)
  - download a pre-built static Linux binary from project releases and copy into `~/.local/bin` in the distro
- [ ] Spawn: `wsl.exe -d <distro> --cd <workspace> -- viban-server --port 0 --workspace .`
- [ ] Parse `{"ready":true,"port":<n>}` from stdout, connect WebSocket to `localhost:<n>` (Windows side — WSL2 forwards automatically)
- [ ] Same auth token flow as local mode (token passed via env)
- [ ] UI badge showing "Remote: WSL/<distro>" in the workspace header so the user always knows where the code is running
- [ ] Path normalization: agent operations may return POSIX paths from the server; the UI must display them correctly but never try to access them directly from Windows
- [ ] Handle WSL distro stop / suspend: reconnect with exponential backoff, show "reconnecting" state, allow manual retry
- [ ] git worktrees are created inside WSL filesystem (`/home/<user>/.viban/worktrees/<task-id>`), NOT under `\\wsl$` mount from Windows — performance is dramatically better when git operations stay within the WSL filesystem

**Done when**: a project in WSL behaves identically to a local project. Tasks run, agents spawn, diffs render, git commits land on the WSL filesystem. Killing the distro and restarting it recovers cleanly.

### Phase 7+ — additional remote transports (post-MVP, not specified here)

Designed-for but out of scope for this doc:
- SSH remote (`ssh user@host viban-server --port 0 ...`, tunnel port back)
- Dev containers (docker exec into a running container)
- Cloud dev environments (Coder, Gitpod-style)

All these share the same `viban-server` binary and JSON-RPC protocol. Only the spawn/connect strategy differs per transport. Each gets its own ADR before implementation.

## Post-MVP improvements

Incremental features added after the MVP (Phases 1–5). Each is built TDD-first
and ships as its own PR.

### Merge from board

A task card with a branch shows a **Merge** button. Confirming it runs
`git.merge`: the task's branch is merged into the project's current branch,
the worktree and branch are removed, the task's `worktree_path` / `branch`
fields are cleared, and the task moves to the Done column. A conflicting
merge is aborted and surfaces an error, leaving the worktree intact so the
user can resolve it manually. (This is the "merge & cleanup" deferred from
Phase 4.)

### AI-generated commit messages

When a review is accepted (`git.commit`), the commit message is generated from
the worktree's diff by a one-shot `claude -p` call rather than always being
the task title. The task title is the fallback when the CLI is unavailable or
there are no changes, so commits always succeed offline.

### Data outside the project folder

viban stores **nothing** inside the user's project folder. The SQLite
database and the git worktrees live in the OS local data directory
(`%LOCALAPPDATA%\viban` / `~/.local/share/viban`), under `projects/<key>/`
keyed per project. `viban-server` resolves this itself and accepts a
`--data-dir` override. This keeps a cloud-synced project (OneDrive, Dropbox)
from locking the database, and keeps agent worktrees off the sync. Design:
`docs/decisions/0003-data-outside-the-project-folder.md`.

### Multiple attempts per task

A task can be run by the agent more than once. Each run is an **attempt** with
its own git worktree, branch, and session; attempts coexist so they can be
compared. The task's `session_id` / `worktree_path` / `branch` point at the
**active** attempt, which is what the card and the review/merge surface act
on. `tasks.start_session` records the first attempt; `attempts.create` starts
another (a card with a session shows a **New attempt** button);
`attempts.activate` switches the active attempt (the **DiffView** shows a
selector when a task has more than one). Worktrees are keyed by attempt id.
Design and rationale: `docs/decisions/0002-multiple-attempts-per-task.md`.

### Work without Git

A worktree needs the project folder to be **its own** git repository — being
merely a subdirectory of an outer repo does not count, since a worktree would
then branch off that outer repo. `git.is_repo_root` (compare `git rev-parse
--show-toplevel` against the folder) makes this distinction; `prepare_repo`
initializes a *dedicated* repo when the folder only sits inside an ancestor
one.

When the folder is not its own repository, `tasks.start_session` returns
`needs_git_init` and the UI shows a dialog with three choices: **Initialize
git** (`init_git: true` — make a repo, then use worktrees as usual), **Work
without Git** (`without_git: true`), or cancel. In no-git mode the agent runs
directly in the project folder: the attempt carries no `worktree_path` or
`branch`, and diff review / merge — which need a worktree — do not apply.
`attempts.create` follows the task's mode automatically (worktree when the
project is a ready repository, otherwise a plain in-folder session).

### Live task status

Each task card shows the live state of its agent so the board is a real
dashboard — at a glance you see what is working, what is finished, and what
broke, without opening every chat.

`AgentStatus` (in `viban-core`) has three states, derived from the agent's
event stream:

- **`running`** — the agent is processing (an amber, pulsing dot).
- **`done`** — the agent finished its turn; ready for the user (a green dot).
- **`failed`** — the last turn errored, or the agent itself errored (a red
  dot).

A task with no session, or whose agent has not emitted anything yet, shows no
dot. Status is **live, not persisted** — it is held in memory per connection
and reflects only currently/recently running agents; a fresh server starts
with everything blank.

The server tracks status in the per-connection `Context` (keyed by task id).
The agent event pump maps `AgentEvent`s to transitions and pushes a
`TaskStatusUpdate { task_id, status }` on the **`tasks`** notification topic
(see "Streaming events"). `boards.get` includes a `statuses` map so a fresh
board load is accurate. The board view subscribes to the `tasks` topic for
the duration it is shown; the store applies updates, and a finished or failed
agent also raises a toast.

OS-level notifications (reaching the user when the window is unfocused) are a
deliberate later addition — the in-app dot and toast come first.

### Session-to-file linkage

A session's chat view lists the files its agent has edited — its footprint —
so you can see what it changed without reading every message. This works even
in "Work without Git" mode, where there is no worktree to `git diff`.

The server watches the agent event stream for file-editing tool calls
(`Edit` / `Write` / `MultiEdit` carry `file_path`, `NotebookEdit` carries
`notebook_path`; read-only tools are ignored) and records each distinct path
in a `session_files` table — durable, so the footprint survives a restart.
`sessions.get` returns the recorded `files`. While a chat is open the view
also grows the list live from the `tool_use` events it already receives, so
no extra subscription is needed.

## Coding conventions

### Rust

- **Crate boundaries are sacred.** `viban-core` must never import `tauri`, `tokio-tungstenite`, or anything transport-related. `viban-server` is the only place WebSocket and JSON-RPC live. `src-tauri` is the only place Tauri lives. If you find yourself wanting to import the wrong thing into the wrong crate, the abstraction is wrong — fix the abstraction, don't smuggle the import.
- **Wire types live in viban-core.** Domain types (`Task`, `Session`, `AgentEvent`, etc.) carry `Serialize + Deserialize` derives from day 1, even though `viban-core` doesn't itself do serialization. This avoids parallel type hierarchies in the protocol layer.
- Errors: `anyhow::Result` internally, convert to `String` at every `#[tauri::command]` boundary
- Async: every command is `async fn`. Never `block_on` inside a command.
- Locks: `tokio::sync::{Mutex, RwLock}`. Never `std::sync` in async code paths.
- IPC types: every struct crossing IPC derives `Serialize, Deserialize, Clone, Debug`
- One Tauri command per logical operation, one file per command group
- No `.unwrap()` / `.expect()` outside tests and main setup
- Logging: `tracing` crate, not `println!` / `eprintln!`
- Format: `rustfmt` default. Lint: `cargo clippy -- -D warnings`.

### TypeScript

- `strict: true`. No `any` — use `unknown` and narrow.
- DTOs in `src/types/` are generated from the Rust structs in `viban-core` by `ts-rs` — `cargo test` writes `src/types/generated/`, and the hand-written `src/types/*.ts` files re-export those mirrors. CI fails if the committed output is stale. To add a shared type, derive `TS` on the Rust struct (with `#[ts(export, export_to = "../../../src/types/generated/")]`) — never hand-write the TypeScript mirror.
- Functional components only. No classes.
- Side effects in hooks; shared state in Zustand stores.
- Tailwind utilities only — no global custom CSS unless absolutely needed.
- File naming: `kebab-case.tsx` for components, `useThing.ts` for hooks, `useThingStore.ts` for stores.
- Import order: react → external → `@/` → relative.
- Format: prettier default. Lint: eslint with `@typescript-eslint/strict`.
- Adding a new UI component: `npx shadcn@latest add <name>`. Files land in `src/components/ui/` and are owned by us — edit them directly when needed, don't wrap them in adapter layers.
- Composing styles: use the `cn()` helper from `src/lib/utils.ts` — never concatenate Tailwind classes with template literals.

### Git

- Conventional commits: `feat:`, `fix:`, `refactor:`, `chore:`, `docs:`, `test:`
- One concern per commit
- No `wip` / `temp` in main branch
- No commits with secrets — `.env` is in `.gitignore` from commit 0

## Cross-platform gotchas

Things that have bitten people building similar apps. Actively guard against these:

- **WebKitGTK (Linux)**: avoid `backdrop-filter`, multi-layer box-shadows, large gradients. Test scroll perf with 100+ list items on Ubuntu 22.04.
- **Windows paths**: always use `PathBuf`, never string-concatenate paths. Beware backslashes leaking into CLI args.
- **macOS code signing**: required for distribution. Get an Apple Developer account ($99/yr) before shipping a public release. Notarization adds 2-5 min to CI.
- **Windows code signing**: SmartScreen blocks unsigned executables. Either get a code signing cert (~$200-500/yr) or accept the warning for early users.
- **Process spawning**: on Windows, child processes need `CREATE_NO_WINDOW` flag or a console flashes. Already in the canonical spawn snippet above.
- **Linux bundles**: Tauri produces .deb, .rpm, .appimage. Default distribution choice = AppImage (no install needed).

## Strict don't-do list

- ❌ Do NOT put business logic in `src-tauri` commands. They are PROXIES — they receive `invoke()`, forward to `viban-server` via JSON-RPC, return the result. Anything with a `match` on domain types, a database query, or a subprocess call belongs in `viban-core` and is reached via the server.
- ❌ Do NOT import `tauri` anywhere in `viban-core` or `viban-server`. Cargo.toml of those crates must not list it as a dependency. Enforce in CI by checking with `cargo tree`.
- ❌ Do NOT bypass the JSON-RPC layer with direct function calls from `src-tauri` to `viban-core` (even though they're in the same workspace and could technically). The boundary is the whole point — collapsing it means Phase 6 won't work.
- ❌ Do NOT switch frameworks (no Electron, no Flutter, no GPUI). Decision is final until an ADR says otherwise.
- ❌ Do NOT use Monaco. CodeMirror 6 only.
- ❌ Do NOT add a second component library (Tamagui, MUI, Chakra, Mantine, NativeBase). shadcn/ui is the only one. If a needed component is missing, build it on top of Radix Primitives following the shadcn pattern.
- ❌ Do NOT add RAG, knowledge graph, vector search, multi-agent orchestration, or MCP integration during Phase 1-5. They come AFTER MVP, in separate planning.
- ❌ Do NOT use `git2-rs`. Shell out to `git`.
- ❌ Do NOT use `app.emit()` for high-frequency streams. Use `Channel<T>`.
- ❌ Do NOT introduce build steps that don't work on all 3 OSes.
- ❌ Do NOT commit `.env`, API keys, OAuth tokens, or session credentials.
- ❌ Do NOT add a user-facing feature without first describing it in this doc.
- ❌ Do NOT use `unwrap()` / `expect()` in production code paths.
- ❌ Do NOT bypass Tailwind with arbitrary custom CSS without a documented reason.

## How to work on this project

1. Read this file fully before touching code.
2. Identify the current phase. Don't skip ahead.
3. Each task in the current phase is its own commit.
4. Iterate locally: `npm run tauri dev`.
5. Before every push: `cargo fmt`, `cargo clippy`, `npm run typecheck`, `npm run lint`.
6. If a decision needs to change, open an ADR in `docs/decisions/` first.

## Open questions for the human (answer before Phase 2)

1. **Claude Code auth**: assume `claude` command is already configured in the user's shell, or provide a config UI to set API key / OAuth in-app?
2. **Multi-project**: one app instance manages one project at a time, or multiple boards across multiple projects?
3. **Telemetry**: none, opt-in, or opt-out? (Default recommendation: none for v0.1, opt-in later.)
4. **Distribution channels**: GitHub releases only, or Homebrew / winget / Flatpak from day 1?
5. **Update mechanism**: Tauri's built-in updater with signed releases, or manual download for now?
