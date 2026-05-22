# ADR-0002: Multiple attempts per task

- Status: Accepted
- Date: 2026-05-22

## Context

A task currently owns exactly one agent run: the `tasks` row carries
`session_id`, `worktree_path`, and `branch`, and `tasks.start_session` is
idempotent — once a task has a session it always returns that one. There is no
way to re-run a task and keep the previous result for comparison. The
worktree-per-task architecture gives isolation but not iteration.

Peer tools (vibe-kanban) let a task have multiple **attempts**, each in its own
worktree and branch, so the user can re-run, compare, and pick the best one.
viban's worktree model already isolates by directory; what is missing is the
data model for more than one live attempt per task.

## Decision

Introduce an `attempts` table. Each attempt is one agent run of a task, with
its own session, worktree, and branch.

```
attempts(
  id            TEXT PRIMARY KEY,
  task_id       TEXT NOT NULL REFERENCES tasks(id),
  session_id    TEXT,
  worktree_path TEXT,
  branch        TEXT,
  created_at    INTEGER NOT NULL
)
```

`tasks` keeps `session_id` / `worktree_path` / `branch` — they denote the
task's **active** attempt: the one shown on the card and operated on by
`git.diff` / `git.commit` / `git.restore` / `git.merge`. This denormalization
is deliberate — the existing task-centric RPC surface keeps working unchanged,
acting on whatever attempt the task currently points at.

- `tasks.start_session` creates the **first** attempt: a worktree + branch +
  session, an `attempts` row, and points the task's fields at it.
- `attempts.create` starts an **additional** attempt: a fresh worktree +
  branch + session and a new `attempts` row, then repoints the task's active
  fields. Earlier attempts' worktrees and branches are left intact, so
  attempts coexist.
- `attempts.activate` repoints the task's active fields at an existing
  attempt, so the user can switch which attempt the review/merge surface acts
  on.
- `attempts.list` returns a task's attempts, newest first.

Worktree paths and branch names are keyed by **attempt id**, not task id
(`.viban/worktrees/<attempt-id>`, `viban/<slug>-<attempt-id-fragment>`), so
multiple attempts of one task never collide.

`MIGRATION_4` adds the table and backfills one attempt row for every task that
already has a session, copying that task's `session_id` / `worktree_path` /
`branch` verbatim.

No per-attempt status column is added: an attempt's state is derived (it is
active if the task points at it; committed/merged state is already visible via
the task's column). A status field can be added by a later ADR if needed.

This denormalized model is chosen over full normalization (dropping the
columns from `tasks` and threading `attempt_id` through every `git.*` method)
because it reuses the entire existing RPC and frontend surface, ships far less
code, and delivers the same user-visible capability.

## Consequences

- A task can have any number of attempts; their worktrees and branches coexist
  on disk and can be reviewed and merged independently by switching the active
  attempt.
- `git.diff` / `git.commit` / `git.restore` / `git.merge` are unchanged — they
  keep acting on the task's active attempt.
- The `tasks` columns are denormalized copies of the active attempt's fields;
  any code that writes an attempt must keep both in sync. This is the accepted
  cost of the smaller change.
- Deleting a task must tear down every attempt's worktree and branch, not just
  the active one.
- The model is reversible: a later ADR can normalize fully with no
  user-facing change.
