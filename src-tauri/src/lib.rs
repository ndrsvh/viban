//! viban desktop shell — the Tauri application host.
//!
//! This crate is a thin UI host: it spawns the `viban-server` sidecar and
//! proxies JSON-RPC calls to it. No domain logic lives here.

mod client;
mod commands;
mod sidecar;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager, RunEvent};
use tauri_plugin_shell::process::CommandChild;
use tokio::sync::{oneshot, Mutex};

use client::Client;

/// Shared application state: the JSON-RPC client, the sidecar handle, and a
/// flag that tells the supervisor to stop restarting once the app is exiting.
#[derive(Default)]
pub struct AppState {
    client: Mutex<Option<Arc<Client>>>,
    sidecar: Mutex<Option<CommandChild>>,
    shutting_down: AtomicBool,
}

impl AppState {
    /// Returns the JSON-RPC client once the sidecar handshake has completed.
    pub async fn client(&self) -> Option<Arc<Client>> {
        self.client.lock().await.clone()
    }
}

/// Backoff bounds for sidecar restarts.
const INITIAL_BACKOFF: Duration = Duration::from_millis(500);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

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
            tauri::async_runtime::spawn(supervise_sidecar(handle));
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building viban")
        .run(|app, event| {
            if let RunEvent::ExitRequested { .. } = event {
                app.state::<AppState>()
                    .shutting_down
                    .store(true, Ordering::Relaxed);
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

/// Keeps a `viban-server` running: connects, waits for it to die, and restarts
/// it with exponential backoff until the app shuts down.
async fn supervise_sidecar(app: AppHandle) {
    let workspace = match std::env::current_dir() {
        Ok(dir) => dir.to_string_lossy().into_owned(),
        Err(err) => {
            tracing::error!(%err, "cannot determine workspace directory");
            return;
        }
    };

    let mut backoff = INITIAL_BACKOFF;
    loop {
        if app
            .state::<AppState>()
            .shutting_down
            .load(Ordering::Relaxed)
        {
            return;
        }

        match start_sidecar(&app, &workspace).await {
            Ok(exited) => {
                backoff = INITIAL_BACKOFF;
                // Block until the sidecar process dies.
                let _ = exited.await;
                tracing::warn!("viban-server connection lost");
                *app.state::<AppState>().client.lock().await = None;
            }
            Err(err) => tracing::error!(%err, "failed to start viban-server"),
        }

        if app
            .state::<AppState>()
            .shutting_down
            .load(Ordering::Relaxed)
        {
            return;
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

/// Spawns viban-server, connects the JSON-RPC client, and publishes both into
/// shared state. Returns a signal that resolves when the sidecar dies.
async fn start_sidecar(app: &AppHandle, workspace: &str) -> anyhow::Result<oneshot::Receiver<()>> {
    let sidecar = sidecar::spawn(app, workspace).await?;
    let port = sidecar.port;

    // Store the handle before connecting so it is always reachable for
    // kill-on-exit.
    *app.state::<AppState>().sidecar.lock().await = Some(sidecar.child);

    let client = Client::connect(port, &sidecar.token).await?;
    *app.state::<AppState>().client.lock().await = Some(Arc::new(client));
    tracing::info!(port, "connected to viban-server");
    Ok(sidecar.exited)
}
