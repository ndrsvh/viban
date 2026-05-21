//! Spawns and supervises the bundled `viban-server` sidecar process.

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use tauri::AppHandle;
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;
use tokio::sync::oneshot;

/// A successfully launched sidecar: its bound port, the shared auth token, the
/// process handle (kept for kill-on-exit), and a signal that resolves once the
/// process dies.
pub struct Sidecar {
    pub port: u16,
    pub token: String,
    pub child: CommandChild,
    pub exited: oneshot::Receiver<()>,
}

#[derive(Deserialize)]
struct ReadyLine {
    ready: bool,
    port: u16,
}

/// Generates an auth token, spawns `viban-server`, and waits for its ready
/// line on stdout to learn the OS-assigned port.
pub async fn spawn(app: &AppHandle, workspace: &str) -> Result<Sidecar> {
    let token = generate_token();

    let (mut rx, child) = app
        .shell()
        .sidecar("viban-server")
        .context("viban-server sidecar is not configured")?
        .args(["--port", "0", "--workspace", workspace])
        .env("VIBAN_AUTH_TOKEN", &token)
        .spawn()
        .context("failed to spawn viban-server")?;

    let (ready_tx, ready_rx) = oneshot::channel::<Result<u16>>();
    let mut ready_tx = Some(ready_tx);
    let (exit_tx, exit_rx) = oneshot::channel::<()>();

    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(bytes) => {
                    let line = String::from_utf8_lossy(&bytes);
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    if let Some(tx) = ready_tx.take() {
                        let _ = tx.send(parse_ready(line));
                    } else {
                        tracing::debug!(line, "viban-server stdout");
                    }
                }
                CommandEvent::Stderr(bytes) => {
                    let line = String::from_utf8_lossy(&bytes);
                    let line = line.trim_end();
                    if !line.is_empty() {
                        tracing::info!(target: "viban_server", "{line}");
                    }
                }
                CommandEvent::Error(err) => {
                    tracing::error!(err, "viban-server process error");
                }
                CommandEvent::Terminated(payload) => {
                    tracing::warn!(code = ?payload.code, "viban-server terminated");
                    if let Some(tx) = ready_tx.take() {
                        let _ = tx.send(Err(anyhow!("viban-server exited before ready")));
                    }
                    break;
                }
                _ => {}
            }
        }
        let _ = exit_tx.send(());
    });

    let port = ready_rx
        .await
        .context("viban-server ready channel dropped")??;

    Ok(Sidecar {
        port,
        token,
        child,
        exited: exit_rx,
    })
}

fn parse_ready(line: &str) -> Result<u16> {
    let parsed: ReadyLine =
        serde_json::from_str(line).with_context(|| format!("unexpected ready line: {line}"))?;
    if !parsed.ready {
        bail!("viban-server reported not ready");
    }
    Ok(parsed.port)
}

/// 32 bytes of OS randomness, hex-encoded into a 64-char token.
fn generate_token() -> String {
    use std::fmt::Write as _;

    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("OS RNG must be available");
    let mut token = String::with_capacity(64);
    for byte in bytes {
        let _ = write!(token, "{byte:02x}");
    }
    token
}
