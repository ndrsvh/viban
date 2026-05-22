//! Token handshake: the first WebSocket message a client sends must equal the
//! shared secret passed to the server via the VIBAN_AUTH_TOKEN env var.

use anyhow::Result;
use futures_util::StreamExt;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

/// Reads the first message and checks it against `token`. Returns whether the
/// client is authorized.
pub async fn authenticate<S>(ws: &mut WebSocketStream<S>, token: &str) -> Result<bool>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    match ws.next().await {
        Some(Ok(Message::Text(text))) => {
            Ok(constant_time_eq(text.as_str().as_bytes(), token.as_bytes()))
        }
        Some(Ok(_)) => Ok(false),
        Some(Err(err)) => Err(err.into()),
        None => Ok(false),
    }
}

/// Compares two byte slices without an early-out, so a timing side channel
/// cannot reveal how much of the token matched.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::constant_time_eq;

    #[test]
    fn identical_slices_match() {
        assert!(constant_time_eq(b"secret-token", b"secret-token"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn slices_of_equal_length_but_different_content_do_not_match() {
        assert!(!constant_time_eq(b"secret-token", b"secret-tokeX"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"Xbc", b"abc"));
    }

    #[test]
    fn slices_of_different_lengths_do_not_match() {
        assert!(!constant_time_eq(b"short", b"longer-value"));
        assert!(!constant_time_eq(b"", b"x"));
        assert!(!constant_time_eq(b"prefix", b"prefix-extra"));
    }
}
