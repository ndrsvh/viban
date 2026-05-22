//! viban-server: the standalone JSON-RPC/WebSocket backend for viban.
//!
//! Spawned as a sidecar by the Tauri shell in local mode, and run directly on
//! a remote host in later phases. It owns no UI and no Tauri code.

mod auth;
mod rpc;
mod ws;

use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

/// Command-line arguments.
#[derive(Debug, Parser)]
#[command(name = "viban-server", version, about = "viban JSON-RPC server")]
struct Args {
    /// TCP port to bind on 127.0.0.1. `0` lets the OS pick a free port.
    #[arg(long, default_value_t = 0)]
    port: u16,

    /// Filesystem path of the workspace this server operates on.
    #[arg(long)]
    workspace: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Logs go to stderr so the ready line on stdout stays machine-parseable.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let token = std::env::var("VIBAN_AUTH_TOKEN")
        .context("VIBAN_AUTH_TOKEN environment variable must be set")?;

    let db_dir = args.workspace.join(".viban");
    std::fs::create_dir_all(&db_dir).context("failed to create .viban directory")?;
    let db = viban_core::db::Db::open(&db_dir.join("viban.db"))
        .await
        .context("failed to open the database")?;
    db.ensure_default_board(&args.workspace.to_string_lossy())
        .await
        .context("failed to ensure the default board")?;

    // Keep the project repo clean: viban's worktrees and database live under
    // `.viban/`, which should never be committed to the user's project.
    if viban_core::git::is_git_repo(&args.workspace).await {
        if let Err(err) = viban_core::git::ensure_gitignored(&args.workspace, ".viban/").await {
            tracing::warn!(%err, "failed to update .gitignore");
        }
    }

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", args.port))
        .await
        .with_context(|| format!("failed to bind 127.0.0.1:{}", args.port))?;
    let port = listener.local_addr()?.port();

    // First stdout line: the bootstrap handshake the Tauri shell waits for.
    {
        let mut stdout = std::io::stdout().lock();
        writeln!(
            stdout,
            "{}",
            serde_json::json!({ "ready": true, "port": port })
        )?;
        stdout.flush()?;
    }

    tracing::info!(port, workspace = %args.workspace.display(), "viban-server listening");

    ws::serve(listener, token, args.workspace, db).await
}
