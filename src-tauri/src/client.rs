//! JSON-RPC 2.0 client over a WebSocket connection to viban-server.

use anyhow::{anyhow, bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Stream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// A connected JSON-RPC client. The connection is serialized behind a mutex —
/// one request is in flight at a time, which is sufficient until streaming
/// notifications are introduced.
pub struct Client {
    stream: Mutex<Stream>,
    next_id: AtomicU64,
}

impl Client {
    /// Connects to viban-server on `127.0.0.1:<port>` and completes the token
    /// handshake by sending the token as the first message.
    pub async fn connect(port: u16, token: &str) -> Result<Self> {
        let url = format!("ws://127.0.0.1:{port}");
        let (mut stream, _) = tokio_tungstenite::connect_async(url.as_str())
            .await
            .with_context(|| format!("failed to connect to {url}"))?;
        stream
            .send(Message::text(token.to_string()))
            .await
            .context("failed to send auth token")?;
        Ok(Self {
            stream: Mutex::new(stream),
            next_id: AtomicU64::new(1),
        })
    }

    /// Issues a JSON-RPC call and returns its `result` value.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let mut stream = self.stream.lock().await;
        stream
            .send(Message::text(request.to_string()))
            .await
            .context("failed to send JSON-RPC request")?;

        while let Some(msg) = stream.next().await {
            match msg.context("websocket error")? {
                Message::Text(text) => {
                    let response: Value =
                        serde_json::from_str(text.as_str()).context("invalid JSON-RPC response")?;
                    if response.get("id").and_then(Value::as_u64) != Some(id) {
                        continue;
                    }
                    if let Some(error) = response.get("error") {
                        bail!("server error: {error}");
                    }
                    return Ok(response.get("result").cloned().unwrap_or(Value::Null));
                }
                Message::Close(_) => bail!("connection closed by server"),
                _ => continue,
            }
        }
        Err(anyhow!("connection closed before a response arrived"))
    }
}
