//! Worktree checkpoints — save points the user can roll back to.
//!
//! A checkpoint is a real commit on the task's branch capturing everything
//! in the worktree (tracked and untracked). Restoring resets the worktree to
//! that commit.

use std::path::Path;

use anyhow::Result;

use super::run_git;

/// Commits the worktree's full current state and returns the new commit's
/// SHA. `--allow-empty`, so a checkpoint always succeeds even with no changes
/// since the last one.
pub async fn create_checkpoint(worktree: &Path, label: &str) -> Result<String> {
    run_git(worktree, &["add", "-A"]).await?;
    run_git(
        worktree,
        &[
            "commit",
            "--allow-empty",
            "-m",
            &format!("viban checkpoint: {label}"),
        ],
    )
    .await?;
    let sha = run_git(worktree, &["rev-parse", "HEAD"]).await?;
    Ok(sha.trim().to_string())
}

/// Resets the worktree to `commit` — rewinding tracked files and removing any
/// untracked files created since. Destructive: discards everything after the
/// checkpoint.
pub async fn restore_checkpoint(worktree: &Path, commit: &str) -> Result<()> {
    run_git(worktree, &["reset", "--hard", commit]).await?;
    run_git(worktree, &["clean", "-fd"]).await?;
    Ok(())
}
