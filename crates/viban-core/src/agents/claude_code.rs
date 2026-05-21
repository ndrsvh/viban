//! Spawns the Claude Code CLI in bidirectional stream-json mode.
//!
//! The user's prompt is delivered on stdin as a JSON envelope; flags are the
//! only thing on argv, so spawning through a shell carries no injection risk.

use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::mpsc;

use super::{stream, AgentEvent};

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
