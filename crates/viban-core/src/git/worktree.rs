//! Git worktree management — the per-task isolated checkouts.

use std::path::Path;

use anyhow::Result;

use super::run_git;

/// Creates a worktree at `path` on a new branch `branch`, off the repo's HEAD.
pub async fn worktree_add(repo: &Path, path: &Path, branch: &str) -> Result<()> {
    let path = path.to_string_lossy();
    run_git(repo, &["worktree", "add", path.as_ref(), "-b", branch]).await?;
    Ok(())
}

/// Removes a worktree. With `force`, removes it even with uncommitted changes.
pub async fn worktree_remove(repo: &Path, path: &Path, force: bool) -> Result<()> {
    let path = path.to_string_lossy();
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(path.as_ref());
    run_git(repo, &args).await?;
    Ok(())
}

/// Force-deletes a branch.
pub async fn branch_delete(repo: &Path, branch: &str) -> Result<()> {
    run_git(repo, &["branch", "-D", branch]).await?;
    Ok(())
}
