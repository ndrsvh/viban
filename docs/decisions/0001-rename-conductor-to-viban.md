# ADR-0001: Rename the project from "Conductor" to "viban"

- Status: Accepted
- Date: 2026-05-22

## Context

The project specification (`CLAUDE.md`) was authored under the working name
"Conductor". The spec flagged this explicitly: *"Working name. Rename anywhere
in this doc once you pick a final one."*

The repository, the Cargo workspace, all three crates (`viban-core`,
`viban-server`, `viban-app`), the Tauri bundle identifier (`com.viban.app`),
the product name, and the npm package were all scaffolded in commit `b794f2f`
under the name **viban**. The spec prose was the only remaining artifact
carrying the old name, leaving documentation inconsistent with the code.

## Decision

The project's name is **viban**. Every occurrence of "Conductor" / "conductor"
in `CLAUDE.md` is updated to "viban", and the provisional "Working name" note
is removed.

Concrete renames applied to the spec:

- `conductor-core` -> `viban-core`
- `conductor-server` -> `viban-server`
- `CONDUCTOR_AUTH_TOKEN` -> `VIBAN_AUTH_TOKEN`
- worktree directory `.conductor/worktrees/` -> `.viban/worktrees/`
- branch prefix `conductor/<task-slug>` -> `viban/<task-slug>`
- the project root in the structure tree `conductor/` -> `viban/`

The scaffolded code already used these `viban-*` names, so this ADR records
no code change beyond the spec — it brings the documentation in line with
what `b794f2f` shipped.

## Consequences

- Documentation matches the code; there is no parallel naming to reconcile.
- `VIBAN_AUTH_TOKEN` and the `.viban/worktrees/` directory name are fixed
  before any code reads them, so no later migration is needed.
- This decision is final unless a future ADR supersedes it.
