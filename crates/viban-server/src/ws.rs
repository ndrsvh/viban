//! WebSocket transport: accepts the connection, runs the auth handshake, then
//! serves JSON-RPC — multiplexing request responses and event notifications
//! onto a single outbound queue feeding the write half.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::Message;
use viban_core::db::Db;

use crate::auth;
use crate::rpc::{self, EventSink};

/// Accept loop. Each connection is handled on its own task, with its own
/// `Context` carrying a per-connection session registry and event sink.
pub async fn serve(
    listener: TcpListener,
    token: String,
    workspace: PathBuf,
    data_dir: PathBuf,
    db: Db,
) -> Result<()> {
    let token = Arc::new(token);

    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(pair) => pair,
            Err(err) => {
                tracing::warn!(%err, "accept failed");
                continue;
            }
        };
        let token = Arc::clone(&token);
        let workspace = workspace.clone();
        let data_dir = data_dir.clone();
        let db = db.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_connection(stream, peer, &token, workspace, data_dir, db).await
            {
                tracing::warn!(%peer, %err, "connection closed with error");
            }
        });
    }
}

async fn handle_connection(
    stream: TcpStream,
    peer: SocketAddr,
    token: &str,
    workspace: PathBuf,
    data_dir: PathBuf,
    db: Db,
) -> Result<()> {
    let mut ws = tokio_tungstenite::accept_async(stream).await?;
    tracing::debug!(%peer, "websocket connected");

    if !auth::authenticate(&mut ws, token).await? {
        tracing::warn!(%peer, "authentication failed");
        ws.close(None).await.ok();
        return Ok(());
    }
    tracing::debug!(%peer, "authenticated");

    let (mut sink, mut incoming) = ws.split();
    let (outbound, mut outbound_rx) = mpsc::unbounded_channel::<String>();

    // One task owns the write half; responses and notifications share it.
    tokio::spawn(async move {
        while let Some(text) = outbound_rx.recv().await {
            if sink.send(Message::text(text)).await.is_err() {
                break;
            }
        }
    });

    let ctx = rpc::Context {
        workspace,
        data_dir,
        db,
        registry: Arc::new(Mutex::new(HashMap::new())),
        events: EventSink::new(outbound.clone()),
        statuses: Arc::new(Mutex::new(HashMap::new())),
    };

    while let Some(message) = incoming.next().await {
        match message {
            Ok(Message::Text(text)) => {
                let response = rpc::handle(text.as_str(), &ctx).await;
                if outbound.send(response).is_err() {
                    break;
                }
            }
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(_) => {}
        }
    }

    // The UI client is gone. Kill every agent so no `claude` process is
    // orphaned, then exit — in local mode the server's lifetime equals the
    // client's.
    tracing::info!(%peer, "client disconnected, shutting down");
    let sessions: Vec<_> = ctx
        .registry
        .lock()
        .await
        .drain()
        .map(|(_, session)| session)
        .collect();
    for mut session in sessions {
        session.kill().await;
    }
    std::process::exit(0)
}
