//! Reading and resolving a worktree's pending changes for review.

use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::run_git;

/// How a file changed relative to the worktree's HEAD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src/types/generated/")]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
}

/// One changed file in a worktree, carrying both sides so a diff can be
/// rendered without further git calls.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/types/generated/")]
pub struct FileDiff {
    pub path: String,
    pub status: FileStatus,
    /// File content at HEAD — empty for an added file.
    pub old_text: String,
    /// File content in the worktree — empty for a deleted file.
    pub new_text: String,
}

/// Lists every pending change in `worktree` against its HEAD, with the old and
/// new text of each file. Results are sorted by path.
pub async fn worktree_diff(worktree: &Path) -> Result<Vec<FileDiff>> {
    let status = run_git(worktree, &["status", "--porcelain"]).await?;

    let mut diffs = Vec::new();
    for line in status.lines() {
        if line.len() < 4 {
            continue;
        }
        let code = &line[..2];
        // Renames render as "old -> new"; the new path is what matters.
        let raw = line[3..].trim();
        let path = raw.rsplit(" -> ").next().unwrap_or(raw).to_string();
        let status = classify(code);

        let old_text = if status == FileStatus::Added {
            String::new()
        } else {
            file_at_head(worktree, &path).await
        };
        let new_text = if status == FileStatus::Deleted {
            String::new()
        } else {
            tokio::fs::read_to_string(worktree.join(&path))
                .await
                .unwrap_or_default()
        };

        diffs.push(FileDiff {
            path,
            status,
            // Normalize to LF so a repo's autocrlf setting does not turn every
            // line into a spurious diff.
            old_text: old_text.replace("\r\n", "\n"),
            new_text: new_text.replace("\r\n", "\n"),
        });
    }
    diffs.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(diffs)
}

/// Stages every change in `worktree` and commits it with `message`.
pub async fn commit_all(worktree: &Path, message: &str) -> Result<()> {
    run_git(worktree, &["add", "-A"]).await?;
    run_git(worktree, &["commit", "-m", message]).await?;
    Ok(())
}

/// Discards every pending change, returning `worktree` to its HEAD — both
/// tracked modifications and untracked files.
pub async fn discard_all(worktree: &Path) -> Result<()> {
    run_git(worktree, &["reset", "--hard", "HEAD"]).await?;
    run_git(worktree, &["clean", "-fd"]).await?;
    Ok(())
}

/// Maps a `git status --porcelain` two-letter code to a coarse status.
fn classify(code: &str) -> FileStatus {
    if code.contains('D') {
        FileStatus::Deleted
    } else if code == "??" || code.contains('A') {
        FileStatus::Added
    } else {
        FileStatus::Modified
    }
}

/// The content of `path` at the worktree's HEAD, or empty if absent there.
async fn file_at_head(worktree: &Path, path: &str) -> String {
    run_git(worktree, &["show", &format!("HEAD:{path}")])
        .await
        .unwrap_or_default()
}
