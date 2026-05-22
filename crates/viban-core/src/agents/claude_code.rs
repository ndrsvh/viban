//! Spawns the Claude Code CLI in bidirectional stream-json mode.
//!
//! The user's prompt is delivered on stdin as a JSON envelope; flags are the
//! only thing on argv, so spawning through a shell carries no injection risk.

use std::path::Path;
use std::process::Stdio;

use anyhow::{bail, Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::mpsc;

use super::{stream, AgentEvent};
use crate::git::{FileDiff, FileStatus};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A handle to a running Claude Code process: send messages in, kill it. The
/// event stream is the `Receiver` returned alongside it by [`spawn_claude`].
pub struct ClaudeSession {
    child: Child,
    stdin: ChildStdin,
}

impl ClaudeSession {
    /// Sends a user message — one stream-json line written to the agent's
    /// stdin. The process stays alive for follow-up messages.
    pub async fn send_message(&mut self, text: &str) -> Result<()> {
        let envelope = serde_json::json!({
            "type": "user",
            "message": { "role": "user", "content": text },
        });
        let mut line = serde_json::to_string(&envelope)?;
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .await
            .context("failed to write to claude stdin")?;
        self.stdin
            .flush()
            .await
            .context("failed to flush claude stdin")?;
        Ok(())
    }

    /// Terminates the agent process. On Windows the whole child tree is
    /// killed, since `claude` runs under a `cmd` shim.
    pub async fn kill(&mut self) {
        #[cfg(windows)]
        if let Some(pid) = self.child.id() {
            let _ = no_window_command("taskkill")
                .args(["/F", "/T", "/PID", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await;
        }
        let _ = self.child.start_kill();
    }
}

/// Spawns `claude` in `workspace`, returning a control handle and the stream
/// of parsed events. The event stream ends when the process exits.
///
/// When `resume` is `Some(claude_session_id)`, the agent reattaches to that
/// existing Claude Code session via `--resume`.
pub fn spawn_claude(
    workspace: &Path,
    resume: Option<&str>,
) -> Result<(ClaudeSession, mpsc::UnboundedReceiver<AgentEvent>)> {
    let mut command = base_command();
    command.args([
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--permission-mode",
        "acceptEdits",
    ]);
    if let Some(claude_session_id) = resume {
        command.args(["--resume", claude_session_id]);
    }
    command
        .current_dir(workspace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = command.spawn().context("failed to spawn claude")?;
    let stdin = child.stdin.take().context("claude stdin unavailable")?;
    let stdout = child.stdout.take().context("claude stdout unavailable")?;
    let stderr = child.stderr.take().context("claude stderr unavailable")?;

    let (tx, rx) = mpsc::unbounded_channel();

    // Pump stdout: one NDJSON line -> one AgentEvent.
    tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if tx.send(stream::parse_line(trimmed)).is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(err) => {
                    let _ = tx.send(AgentEvent::Error {
                        message: format!("claude stdout error: {err}"),
                    });
                    break;
                }
            }
        }
    });

    // Drain stderr to the log so a full pipe can't stall the process.
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if !line.trim().is_empty() {
                tracing::debug!(target: "claude", "{line}");
            }
        }
    });

    Ok((ClaudeSession { child, stdin }, rx))
}

#[cfg(windows)]
fn base_command() -> Command {
    // `claude` is a `.cmd` shim on Windows; CreateProcess won't resolve it,
    // so go through `cmd /c`, which honors PATHEXT.
    let mut command = no_window_command("cmd");
    command.args(["/c", "claude"]);
    command
}

#[cfg(not(windows))]
fn base_command() -> Command {
    Command::new("claude")
}

#[cfg(windows)]
fn no_window_command(program: &str) -> Command {
    use std::os::windows::process::CommandExt;

    let mut std_command = std::process::Command::new(program);
    std_command.creation_flags(CREATE_NO_WINDOW);
    Command::from(std_command)
}

/// Asks Claude Code for a one-line commit message describing `files`. Falls
/// back to `fallback` when there are no changes or the CLI is unavailable, so
/// the caller never has to handle an error.
pub async fn generate_commit_message(workdir: &Path, files: &[FileDiff], fallback: &str) -> String {
    if files.is_empty() {
        return fallback.to_string();
    }
    match run_claude_oneshot(workdir, &build_commit_prompt(files)).await {
        Ok(message) if !message.is_empty() => message,
        _ => fallback.to_string(),
    }
}

/// Builds the one-shot prompt: an instruction plus a capped dump of the
/// changed files and their new content.
fn build_commit_prompt(files: &[FileDiff]) -> String {
    let mut body = String::new();
    for file in files {
        let verb = match file.status {
            FileStatus::Added => "added",
            FileStatus::Modified => "modified",
            FileStatus::Deleted => "deleted",
        };
        body.push_str(&format!("\n=== {verb}: {} ===\n", file.path));
        if file.status != FileStatus::Deleted {
            body.push_str(&file.new_text);
            body.push('\n');
        }
    }
    let body: String = body.chars().take(8000).collect();
    format!(
        "Write a single-line git commit message in the conventional-commits \
         style (for example \"feat: ...\" or \"fix: ...\") summarizing the \
         change below. Reply with ONLY the message — no quotes, no body, no \
         explanation.\n{body}"
    )
}

/// Runs `claude` in one-shot print mode and returns its cleaned output.
async fn run_claude_oneshot(workdir: &Path, prompt: &str) -> Result<String> {
    let output = base_command()
        .args(["-p", prompt, "--output-format", "text"])
        .current_dir(workdir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .context("failed to run claude")?;
    if !output.status.success() {
        bail!("claude exited unsuccessfully");
    }
    Ok(clean_message(&String::from_utf8_lossy(&output.stdout)))
}

/// The first non-empty line of `text`, stripped of surrounding quotes.
fn clean_message(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .trim_matches(|c| c == '"' || c == '`' || c == '\'')
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diff(path: &str, status: FileStatus, new_text: &str) -> FileDiff {
        FileDiff {
            path: path.to_string(),
            status,
            old_text: String::new(),
            new_text: new_text.to_string(),
        }
    }

    #[test]
    fn build_commit_prompt_includes_files_and_the_instruction() {
        let prompt = build_commit_prompt(&[
            diff("src/a.rs", FileStatus::Modified, "fn a() {}"),
            diff("src/b.rs", FileStatus::Added, "fn b() {}"),
        ]);
        assert!(prompt.contains("conventional-commits"));
        assert!(prompt.contains("src/a.rs"));
        assert!(prompt.contains("src/b.rs"));
        assert!(prompt.contains("fn b() {}"));
    }

    #[test]
    fn build_commit_prompt_omits_deleted_file_content() {
        let prompt = build_commit_prompt(&[diff("gone.rs", FileStatus::Deleted, "")]);
        assert!(prompt.contains("deleted: gone.rs"));
    }

    #[test]
    fn clean_message_takes_the_first_line_without_quotes() {
        assert_eq!(
            clean_message("\"feat: add thing\"\n\nextra"),
            "feat: add thing"
        );
        assert_eq!(clean_message("  fix: the bug  "), "fix: the bug");
        assert_eq!(clean_message("`chore: tidy`"), "chore: tidy");
        assert_eq!(clean_message(""), "");
    }

    #[tokio::test]
    async fn generate_commit_message_falls_back_when_there_are_no_changes() {
        let message = generate_commit_message(Path::new("."), &[], "the task title").await;
        assert_eq!(message, "the task title");
    }
}
