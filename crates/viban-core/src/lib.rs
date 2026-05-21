//! viban-core: pure domain logic shared by the server and (for type
//! re-exports) the Tauri shell. Must not depend on tauri or any transport
//! crate (tokio-tungstenite, WebSocket, JSON-RPC).

/// Crate version string, sourced from Cargo at compile time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod agents;
pub mod db;
pub mod types;

pub use agents::AgentEvent;
pub use types::{Message, Session};

/// A fresh random identifier (UUID v4), used for session and message ids.
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
