//! viban desktop shell — the Tauri application host.
//!
//! This crate is a thin UI host: it spawns the `viban-server` sidecar and
//! proxies JSON-RPC calls to it. No domain logic lives here.

mod client;
mod commands;
mod sidecar;

use std::sync::Arc;

use tauri::{Manager, RunEvent};
use tauri_plugin_shell::process::CommandChild;
use tokio::sync::Mutex;

use client::Client;

/// Shared application state: the JSON-RPC client and the sidecar handle.
#[derive(Default)]
pub struct AppState {
    client: Mutex<Option<Arc<Client>>>,
    sidecar: Mutex<Option<CommandChild>>,
}

impl AppState {
    /// Returns the JSON-RPC client once the sidecar handshake has completed.
    pub async fn client(&self) -> Option<Arc<Client>> {
        self.client.lock().await.clone()
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::server_health,
            commands::spawn_session,
            commands::send_message
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(err) = connect_sidecar(handle).await {
                    tracing::error!(%err, "failed to start viban-server");
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building viban")
        .run(|app, event| {
            if let RunEvent::ExitRequested { .. } = event {
                let child = app
                    .state::<AppState>()
                    .sidecar
                    .try_lock()
                    .ok()
                    .and_then(|mut guard| guard.take());
                if let Some(child) = child {
                    if let Err(err) = child.kill() {
                        tracing::warn!(%err, "failed to kill viban-server");
                    }
                }
            }
        });
}

/// Spawns viban-server, connects the JSON-RPC client, and publishes both into
/// shared state. The sidecar handle is stored before connecting so it is
/// always reachable for kill-on-exit.
async fn connect_sidecar(app: tauri::AppHandle) -> anyhow::Result<()> {
    let workspace = std::env::current_dir()?.to_string_lossy().into_owned();

    let sidecar = sidecar::spawn(&app, &workspace).await?;
    let port = sidecar.port;
    let state = app.state::<AppState>();
    *state.sidecar.lock().await = Some(sidecar.child);
    tracing::info!(port, "viban-server ready");

    let client = Client::connect(port, &sidecar.token).await?;
    *state.client.lock().await = Some(Arc::new(client));
    tracing::info!("connected to viban-server");
    Ok(())
}
