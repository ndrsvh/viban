//! Running a one-off shell command in a worktree and streaming its output.
//!
//! Used by the diff-review surface to verify an agent's changes — run the
//! tests, a linter, a build — before they are accepted or merged.

use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use ts_rs::TS;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Which standard stream a line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src/types/generated/")]
pub enum OutputStream {
    Stdout,
    Stderr,
}

/// One event from a streamed command run.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src/types/generated/")]
pub enum CommandOutput {
    /// A line written to stdout or stderr (without its trailing newline).
    Line { stream: OutputStream, text: String },
    /// The process exited. `code` is its status, or `None` if it was
    /// terminated by a signal.
    Exited { code: Option<i32> },
}

/// Runs `command` through the platform shell in `workdir`, streaming each
/// output line and the final exit code over the returned channel. The channel
/// closes once `Exited` has been sent.
pub fn run_command(
    workdir: &Path,
    command: &str,
) -> Result<mpsc::UnboundedReceiver<CommandOutput>> {
    let mut child = shell_command(command)
        .current_dir(workdir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("failed to spawn the command")?;
    let stdout = child.stdout.take().context("command has no stdout")?;
    let stderr = child.stderr.take().context("command has no stderr")?;

    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let out = tokio::spawn(pump(stdout, OutputStream::Stdout, tx.clone()));
        let err = tokio::spawn(pump(stderr, OutputStream::Stderr, tx.clone()));
        let _ = out.await;
        let _ = err.await;
        let code = child.wait().await.ok().and_then(|status| status.code());
        let _ = tx.send(CommandOutput::Exited { code });
    });
    Ok(rx)
}

/// Forwards every line of `reader` as a `Line` event until it closes.
async fn pump<R>(reader: R, stream: OutputStream, tx: mpsc::UnboundedSender<CommandOutput>)
where
    R: AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(text)) = lines.next_line().await {
        if tx.send(CommandOutput::Line { stream, text }).is_err() {
            break;
        }
    }
}

#[cfg(windows)]
fn shell_command(command: &str) -> Command {
    use std::os::windows::process::CommandExt;

    let mut std_command = std::process::Command::new("cmd");
    std_command.args(["/C", command]);
    std_command.creation_flags(CREATE_NO_WINDOW);
    Command::from(std_command)
}

#[cfg(not(windows))]
fn shell_command(command: &str) -> Command {
    let mut builder = Command::new("sh");
    builder.args(["-c", command]);
    builder
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn run_command_streams_output_and_a_zero_exit() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut rx = run_command(dir.path(), "echo hello").expect("spawn");

        let mut lines = Vec::new();
        let mut code = None;
        while let Some(event) = rx.recv().await {
            match event {
                CommandOutput::Line { text, .. } => lines.push(text),
                CommandOutput::Exited { code: status } => code = Some(status),
            }
        }
        assert!(
            lines.iter().any(|line| line.contains("hello")),
            "stdout was streamed: {lines:?}"
        );
        assert_eq!(code, Some(Some(0)), "a clean run exits 0");
    }

    #[tokio::test]
    async fn run_command_reports_a_nonzero_exit() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut rx = run_command(dir.path(), "exit 3").expect("spawn");

        let mut code = None;
        while let Some(event) = rx.recv().await {
            if let CommandOutput::Exited { code: status } = event {
                code = Some(status);
            }
        }
        assert_eq!(code, Some(Some(3)), "the failing exit code is reported");
    }
}
