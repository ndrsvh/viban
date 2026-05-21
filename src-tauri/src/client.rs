//! JSON-RPC 2.0 client over a WebSocket connection to viban-server.
//!
//! A background read loop demultiplexes the socket: framed responses (which
//! carry an `id`) complete the matching pending call; `events.update`
//! notifications are routed to the subscription named in their params.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Stream = WebSocketStream<MaybeTlsStream<TcpStream>>;
type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>>;
type Subscriptions = Arc<Mutex<HashMap<String, mpsc::UnboundedSender<Value>>>>;

/// A connected JSON-RPC client. Calls and subscriptions may be in flight
/// concurrently — the read loop fans incoming messages out by id.
pub struct Client {
    sink: Mutex<SplitSink<Stream, Message>>,
    pending: Pending,
    subscriptions: Subscriptions,
    next_id: AtomicU64,
}

impl Client {
    /// Connects to viban-server on `127.0.0.1:<port>`, completes the token
    /// handshake, and starts the background read loop.
    pub async fn connect(port: u16, token: &str) -> Result<Self> {
        let url = format!("ws://127.0.0.1:{port}");
        let (mut stream, _) = tokio_tungstenite::connect_async(url.as_str())
            .await
            .with_context(|| format!("failed to connect to {url}"))?;
        stream
            .send(Message::text(token.to_string()))
            .await
            .context("failed to send auth token")?;

        let (sink, read) = stream.split();
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let subscriptions: Subscriptions = Arc::new(Mutex::new(HashMap::new()));

        tokio::spawn(read_loop(
            read,
            Arc::clone(&pending),
            Arc::clone(&subscriptions),
        ));

        Ok(Self {
            sink: Mutex::new(sink),
            pending,
            subscriptions,
            next_id: AtomicU64::new(1),
        })
    }

    /// Issues a JSON-RPC call and awaits its result.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);

        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        if let Err(err) = self
            .sink
            .lock()
            .await
            .send(Message::text(request.to_string()))
            .await
        {
            self.pending.lock().await.remove(&id);
            return Err(err).context("failed to send JSON-RPC request");
        }

        rx.await.context("response channel dropped")?
    }

    /// Registers a subscription and returns the receiver for its
    /// `events.update` notifications.
    pub async fn subscribe(&self, subscription_id: &str) -> mpsc::UnboundedReceiver<Value> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.subscriptions
            .lock()
            .await
            .insert(subscription_id.to_string(), tx);
        rx
    }
}

/// Reads the socket until it closes, routing each message to its waiting
/// caller or subscription.
async fn read_loop(mut read: SplitStream<Stream>, pending: Pending, subscriptions: Subscriptions) {
    while let Some(message) = read.next().await {
        let text = match message {
            Ok(Message::Text(text)) => text,
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(_) => continue,
        };
        let value: Value = match serde_json::from_str(text.as_str()) {
            Ok(value) => value,
            Err(err) => {
                tracing::warn!(%err, "discarding unparseable server message");
                continue;
            }
        };

        if let Some(id) = value.get("id").and_then(Value::as_u64) {
            if let Some(tx) = pending.lock().await.remove(&id) {
                let _ = tx.send(extract_result(&value));
            }
        } else if value.get("method").and_then(Value::as_str) == Some("events.update") {
            let sub_id = value
                .pointer("/params/subscription_id")
                .and_then(Value::as_str);
            let event = value.pointer("/params/event");
            if let (Some(sub_id), Some(event)) = (sub_id, event) {
                if let Some(tx) = subscriptions.lock().await.get(sub_id) {
                    let _ = tx.send(event.clone());
                }
            }
        }
    }

    // Connection gone — fail every in-flight call so callers don't hang.
    for (_, tx) in pending.lock().await.drain() {
        let _ = tx.send(Err(anyhow!("connection to viban-server closed")));
    }
}

fn extract_result(value: &Value) -> Result<Value> {
    if let Some(error) = value.get("error") {
        bail!("server error: {error}");
    }
    Ok(value.get("result").cloned().unwrap_or(Value::Null))
}
