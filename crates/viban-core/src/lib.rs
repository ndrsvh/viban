//! viban-core: pure domain logic shared by the server and (for type
//! re-exports) the Tauri shell. Must not depend on tauri or any transport
//! crate (tokio-tungstenite, WebSocket, JSON-RPC).

/// Crate version string, sourced from Cargo at compile time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod agents;
pub mod db;
pub mod git;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_id_is_unique_and_a_valid_uuid() {
        let a = new_id();
        let b = new_id();
        assert_ne!(a, b, "ids must be unique");
        assert_eq!(a.len(), 36, "a uuid v4 string is 36 characters");
        assert!(
            uuid::Uuid::parse_str(&a).is_ok(),
            "new_id must produce a valid uuid"
        );
    }

    #[test]
    fn now_millis_is_a_plausible_timestamp() {
        let now = now_millis();
        // Sanity bounds: after 2020-01-01 and before 2100-01-01.
        assert!(now > 1_577_836_800_000, "timestamp should be after 2020");
        assert!(now < 4_102_444_800_000, "timestamp should be before 2100");
    }

    #[test]
    fn version_is_populated_from_cargo() {
        assert!(!VERSION.is_empty(), "crate version must not be empty");
    }
}
