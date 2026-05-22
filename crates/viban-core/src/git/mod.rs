//! Git operations. viban shells out to the `git` CLI — never `git2-rs`.

mod worktree;

pub use worktree::{branch_delete, worktree_add, worktree_remove};

use std::path::Path;

use anyhow::{bail, Context, Result};
use tokio::process::Command;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Runs `git args` in `dir`, returning stdout. Errors carry git's stderr.
async fn run_git(dir: &Path, args: &[&str]) -> Result<String> {
    let output = git_command()
        .args(args)
        .current_dir(dir)
        .output()
        .await
        .context("failed to run git")?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Returns whether `dir` is inside a git working tree.
pub async fn is_git_repo(dir: &Path) -> bool {
    git_command()
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(dir)
        .output()
        .await
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Appends `entry` to the repo's `.gitignore` unless it is already listed.
pub async fn ensure_gitignored(repo: &Path, entry: &str) -> Result<()> {
    use tokio::io::AsyncWriteExt;

    let path = repo.join(".gitignore");
    let existing = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    if existing.lines().any(|line| line.trim() == entry) {
        return Ok(());
    }
    let prefix = if existing.is_empty() || existing.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await
        .context("failed to open .gitignore")?;
    file.write_all(format!("{prefix}{entry}\n").as_bytes())
        .await
        .context("failed to write .gitignore")?;
    Ok(())
}

/// Converts a task title into a branch-name-safe slug (lowercase ASCII
/// alphanumerics, single dashes, at most 40 chars).
pub fn slugify(title: &str) -> String {
    let mut slug = String::new();
    let mut pending_dash = false;
    for ch in title.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_dash {
                slug.push('-');
                pending_dash = false;
            }
            slug.push(ch);
        } else if !slug.is_empty() {
            pending_dash = true;
        }
    }
    let slug: String = slug.chars().take(40).collect();
    if slug.is_empty() {
        "task".to_string()
    } else {
        slug
    }
}

#[cfg(windows)]
fn git_command() -> Command {
    use std::os::windows::process::CommandExt;

    let mut std_command = std::process::Command::new("git");
    std_command.creation_flags(CREATE_NO_WINDOW);
    Command::from(std_command)
}

#[cfg(not(windows))]
fn git_command() -> Command {
    Command::new("git")
}

#[cfg(test)]
mod tests {
    use super::slugify;

    #[test]
    fn slugify_basics() {
        assert_eq!(slugify("Add login flow"), "add-login-flow");
        assert_eq!(slugify("  Fix: the bug!! "), "fix-the-bug");
        assert_eq!(slugify("***"), "task");
    }

    #[test]
    fn slugify_falls_back_for_empty_or_symbol_only_input() {
        assert_eq!(slugify(""), "task");
        assert_eq!(slugify("   "), "task");
        assert_eq!(slugify("!@#$%"), "task");
    }

    #[test]
    fn slugify_collapses_runs_of_separators_into_single_dashes() {
        assert_eq!(slugify("a---b__c  d"), "a-b-c-d");
        assert_eq!(slugify("Trailing dashes!!!"), "trailing-dashes");
    }

    #[test]
    fn slugify_lowercases_and_keeps_ascii_alphanumerics() {
        assert_eq!(slugify("MixedCase"), "mixedcase");
        assert_eq!(slugify("Hello, World 2024"), "hello-world-2024");
    }

    #[test]
    fn slugify_caps_length_at_forty_characters() {
        let slug = slugify(&"word ".repeat(40));
        assert!(slug.len() <= 40, "slug was {} chars", slug.len());
    }
}
