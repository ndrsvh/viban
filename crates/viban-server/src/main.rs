//! viban-server stub. Real WebSocket + JSON-RPC listener lands in the next
//! commit. For now this binary just identifies itself so that workspace
//! builds produce all three crate outputs.

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!(
        version = viban_core::VERSION,
        "viban-server stub: not yet implemented"
    );
    Ok(())
}
