//! viban-core: pure domain logic shared by the server and (for type
//! re-exports) the Tauri shell. Must not depend on tauri or any transport
//! crate (tokio-tungstenite, WebSocket, JSON-RPC).

/// Crate version string, sourced from Cargo at compile time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod agents;

pub use agents::AgentEvent;
