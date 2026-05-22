//! viban-server: the standalone JSON-RPC/WebSocket backend for viban.
//!
//! Spawned as a sidecar by the Tauri shell in local mode, and run directly on
//! a remote host in later phases. It owns no UI and no Tauri code.

mod auth;
mod paths;
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

    /// Base directory for viban's own data (database, worktrees). Defaults to
    /// the OS local data directory. viban never writes into the workspace.
    #[arg(long)]
    data_dir: Option<PathBuf>,
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

    // viban's data lives outside the workspace, so a cloud-synced project
    // folder cannot lock the database (ADR-0003).
    let data_dir = paths::project_data_dir(args.data_dir.as_deref(), &args.workspace)
        .context("failed to resolve the data directory")?;
    std::fs::create_dir_all(&data_dir).context("failed to create the data directory")?;
    let db = viban_core::db::Db::open(&data_dir.join("viban.db"))
        .await
        .context("failed to open the database")?;
    db.ensure_default_board(&args.workspace.to_string_lossy())
        .await
        .context("failed to ensure the default board")?;

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

    tracing::info!(
        port,
        workspace = %args.workspace.display(),
        data_dir = %data_dir.display(),
        "viban-server listening"
    );

    ws::serve(listener, token, args.workspace, data_dir, db).await
}
