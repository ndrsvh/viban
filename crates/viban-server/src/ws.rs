//! WebSocket transport: accepts connections, runs the auth handshake, then
//! serves JSON-RPC requests for the lifetime of each connection.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;

use crate::{auth, rpc};

/// Accept loop. Each connection is handled on its own task and never brings
/// the listener down.
pub async fn serve(listener: TcpListener, token: String, workspace: PathBuf) -> Result<()> {
    let ctx = Arc::new(rpc::Context { workspace });
    let token = Arc::new(token);

    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(pair) => pair,
            Err(err) => {
                tracing::warn!(%err, "accept failed");
                continue;
            }
        };
        let ctx = Arc::clone(&ctx);
        let token = Arc::clone(&token);
        tokio::spawn(async move {
            if let Err(err) = handle_connection(stream, peer, &token, &ctx).await {
                tracing::warn!(%peer, %err, "connection closed with error");
            }
        });
    }
}

async fn handle_connection(
    stream: TcpStream,
    peer: SocketAddr,
    token: &str,
    ctx: &rpc::Context,
) -> Result<()> {
    let mut ws = tokio_tungstenite::accept_async(stream).await?;
    tracing::debug!(%peer, "websocket connected");

    if !auth::authenticate(&mut ws, token).await? {
        tracing::warn!(%peer, "authentication failed");
        ws.close(None).await.ok();
        return Ok(());
    }
    tracing::debug!(%peer, "authenticated");

    while let Some(msg) = ws.next().await {
        match msg? {
            Message::Text(text) => {
                let response = rpc::handle(text.as_str(), ctx);
                ws.send(Message::text(response)).await?;
            }
            Message::Ping(payload) => ws.send(Message::Pong(payload)).await?,
            Message::Close(_) => break,
            _ => {}
        }
    }
    Ok(())
}
