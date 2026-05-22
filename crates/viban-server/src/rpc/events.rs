//! Server→client push notifications.
//!
//! The server sends JSON-RPC `events.update` notifications, each tagged with a
//! `topic` string and a JSON `payload`. The client subscribes by topic. An
//! agent's output uses its session id as the topic; other features (live task
//! status, …) pick their own topic names, so the transport is not tied to
//! sessions.

use serde::Serialize;
use serde_json::json;
use tokio::sync::mpsc;

/// A handle for pushing notifications to one connected client.
///
/// Cloneable, so the per-connection `Context` and any task it spawns (the
/// agent event pump, file watchers, …) can all emit through the same socket.
#[derive(Clone)]
pub struct EventSink {
    outbound: mpsc::UnboundedSender<String>,
}

impl EventSink {
    /// Wraps the connection's outbound queue. The same queue also carries
    /// JSON-RPC responses — both share the single write half.
    pub fn new(outbound: mpsc::UnboundedSender<String>) -> Self {
        Self { outbound }
    }

    /// Pushes an `events.update` notification on `topic` to the client.
    /// A no-op once the connection's write half is gone.
    pub fn emit(&self, topic: &str, payload: impl Serialize) {
        let payload = match serde_json::to_value(payload) {
            Ok(value) => value,
            Err(err) => {
                tracing::warn!(%err, topic, "failed to serialize an event payload");
                return;
            }
        };
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "events.update",
            "params": { "topic": topic, "payload": payload },
        });
        let _ = self.outbound.send(notification.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::EventSink;
    use serde_json::Value;

    #[test]
    fn emit_frames_a_topic_and_payload_notification() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let sink = EventSink::new(tx);
        sink.emit("tasks", serde_json::json!({ "id": "t1", "status": "done" }));

        let raw = rx.try_recv().expect("a notification was queued");
        let value: Value = serde_json::from_str(&raw).expect("valid json");
        assert_eq!(value["method"], "events.update");
        assert_eq!(value["params"]["topic"], "tasks");
        assert_eq!(value["params"]["payload"]["status"], "done");
    }

    #[test]
    fn emit_is_a_silent_noop_once_the_receiver_is_gone() {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        drop(rx);
        let sink = EventSink::new(tx);
        // Must not panic even though nothing is listening.
        sink.emit("tasks", serde_json::json!({ "ok": true }));
    }
}
