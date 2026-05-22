//! viban-core: pure domain logic shared by the server and (for type
//! re-exports) the Tauri shell. Must not depend on tauri or any transport
//! crate (tokio-tungstenite, WebSocket, JSON-RPC).

/// Crate version string, sourced from Cargo at compile time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod agents;
pub mod db;
pub mod types;

pub use agents::AgentEvent;
pub use types::{Board, Column, Message, Session, Task};

/// A fresh random identifier (UUID v4) — used for session, message, and task ids.
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// The current time as Unix epoch milliseconds.
pub fn now_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}
