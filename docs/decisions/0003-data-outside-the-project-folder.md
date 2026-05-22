# ADR-0003: Store viban's data outside the project folder

- Status: Accepted
- Date: 2026-05-22

## Context

viban kept all of its per-project data inside the project folder under
`.viban/`: the SQLite database at `<project>/.viban/viban.db` and the git
worktrees at `<project>/.viban/worktrees/<id>`. `.viban/` was appended to the
project's `.gitignore` to keep the repo clean.

This breaks when the project lives in a cloud-synced folder (OneDrive,
Dropbox, iCloud Drive). The sync client continuously locks files to upload
them; SQLite then cannot open the database and the server crashes on every
start:

```
failed to run migrations: database is locked (Error code 5)
```

This is not a SQLite shortcoming — any file-backed store in a synced folder
hits the same lock. The worktrees are also harmed: an agent's in-progress
edits and build artifacts get synced to the cloud, which is slow and risky.
The root cause is that viban's *own* data lives inside the user's project.

## Decision

viban stores **nothing** inside the project folder. All per-project data —
the database and the worktrees — moves to the OS application-data directory.

- The base data directory is the OS local data dir
  (`%LOCALAPPDATA%` on Windows, `~/.local/share` on Linux, `~/Library/
  Application Support` on macOS), under `viban/`. `viban-server` also accepts
  a `--data-dir` argument that overrides the base (used by tests, and
  available for remote transports).
- Each project gets its own subdirectory `projects/<key>/`, where `<key>` is
  a stable hash of the canonicalized workspace path, so distinct projects
  never share data.
- The database lives at `<base>/projects/<key>/viban.db`; worktrees at
  `<base>/projects/<key>/worktrees/<attempt-id>`.
- The `Context` carries this `data_dir` alongside `workspace`.
- `git worktree add` accepts an absolute path anywhere, so worktrees work
  unchanged from their new location; the worktree metadata git keeps lives in
  the repo's own `.git/`, which is already ignored.
- Because nothing viban-related is written into the project, the
  `.gitignore` entry for `.viban/` is no longer added (`prepare_repo` and the
  server startup drop that step).
- The local data dir is deliberately the non-roaming one on Windows so it is
  not itself cloud-synced.

For remote transports (Phase 6 WSL onward) the server runs where the code
lives and resolves its own local data dir there, keeping data next to the
server.

The database stays **SQLite** — it was never the problem, and it remains the
locked choice from the spec.

## Consequences

- A project in a cloud-synced folder works: the database is never locked, and
  agent worktrees are not synced to the cloud.
- The project folder stays pristine — viban adds nothing to it, not even a
  `.gitignore` line.
- Pre-existing `<project>/.viban/` directories from older builds are no longer
  read; their data (test tasks, stale worktrees) is effectively reset and the
  directory can be deleted by hand. No automatic migration is provided at this
  pre-1.0 stage.
- `viban-server` gains a `--data-dir` argument and a dependency on the `dirs`
  crate.
