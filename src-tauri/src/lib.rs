//! viban desktop shell — the Tauri application host.
//!
//! This crate is a thin UI host: it spawns the `viban-server` sidecar and
//! proxies JSON-RPC calls to it. No domain logic lives here.

mod client;
mod commands;
mod project;
mod sidecar;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager, RunEvent};
use tauri_plugin_shell::process::CommandChild;
use tokio::sync::{oneshot, Mutex, Notify};

use client::Client;

/// Shared application state: the JSON-RPC client, the sidecar handle, the
/// selected project, and a flag that tells the supervisor to stop restarting
/// once the app is exiting.
#[derive(Default)]
pub struct AppState {
    client: Mutex<Option<Arc<Client>>>,
    sidecar: Mutex<Option<CommandChild>>,
    shutting_down: AtomicBool,
    /// Filesystem path of the open project, or `None` when none is selected.
    project: Mutex<Option<String>>,
    /// Pulsed when the project changes (or the app exits) to wake the
    /// sidecar supervisor.
    project_changed: Notify,
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

/// The next restart delay after a failed/lost sidecar: double the current
/// one, capped at `MAX_BACKOFF`.
fn next_backoff(current: Duration) -> Duration {
    (current * 2).min(MAX_BACKOFF)
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
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::current_project,
            commands::open_project,
            commands::server_health,
            commands::open_session,
            commands::close_session,
            commands::spawn_session,
            commands::send_message,
            commands::start_session,
            commands::create_attempt,
            commands::list_attempts,
            commands::activate_attempt,
            commands::list_sessions,
            commands::get_session,
            commands::get_board,
            commands::create_task,
            commands::update_task,
            commands::delete_task,
            commands::reorder_tasks,
            commands::git_diff,
            commands::git_commit,
            commands::git_restore,
            commands::git_merge
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
                let state = app.state::<AppState>();
                state.shutting_down.store(true, Ordering::Relaxed);
                // Wake the supervisor so it observes the shutdown flag and
                // stops, even when it is idle waiting for a project.
                state.project_changed.notify_one();
                let child = state
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

/// Keeps a `viban-server` running for the open project: connects, waits for
/// the process to die or the project to change, and restarts it with
/// exponential backoff until the app shuts down. With no project selected the
/// supervisor idles until one is opened.
async fn supervise_sidecar(app: AppHandle) {
    // Restore the project remembered from the last launch, if any.
    if let Some(path) = project::load(&app) {
        *app.state::<AppState>().project.lock().await = Some(path);
    }

    let mut backoff = INITIAL_BACKOFF;
    loop {
        let state = app.state::<AppState>();
        if state.shutting_down.load(Ordering::Relaxed) {
            return;
        }

        let project = state.project.lock().await.clone();
        let Some(project) = project else {
            // No project: idle until one is opened (or the app exits).
            state.project_changed.notified().await;
            backoff = INITIAL_BACKOFF;
            continue;
        };

        match start_sidecar(&app, &project).await {
            Ok(exited) => {
                backoff = INITIAL_BACKOFF;
                // Run until the sidecar dies or the project is switched.
                let switched = tokio::select! {
                    _ = exited => false,
                    _ = state.project_changed.notified() => true,
                };

                *state.client.lock().await = None;
                let child = state.sidecar.lock().await.take();
                if let Some(child) = child {
                    if let Err(err) = child.kill() {
                        tracing::warn!(%err, "failed to kill viban-server");
                    }
                }

                // The exit handler pulses `project_changed` too; when the app
                // is shutting down, stop quietly instead of reporting a
                // restart that will never happen.
                if state.shutting_down.load(Ordering::Relaxed) {
                    return;
                }
                if switched {
                    tracing::info!("project changed; restarting viban-server");
                    continue;
                }
                tracing::warn!("viban-server connection lost");
            }
            Err(err) => tracing::error!(%err, "failed to start viban-server"),
        }

        if state.shutting_down.load(Ordering::Relaxed) {
            return;
        }
        tokio::time::sleep(backoff).await;
        backoff = next_backoff(backoff);
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

#[cfg(test)]
mod tests {
    use super::{next_backoff, INITIAL_BACKOFF, MAX_BACKOFF};
    use std::time::Duration;

    #[test]
    fn backoff_doubles_each_step() {
        let first = next_backoff(INITIAL_BACKOFF);
        assert_eq!(first, Duration::from_secs(1));
        assert_eq!(next_backoff(first), Duration::from_secs(2));
    }

    #[test]
    fn backoff_is_capped_at_the_maximum() {
        let mut backoff = INITIAL_BACKOFF;
        for _ in 0..20 {
            backoff = next_backoff(backoff);
        }
        assert_eq!(backoff, MAX_BACKOFF, "backoff settles at the cap");
        assert_eq!(
            next_backoff(MAX_BACKOFF),
            MAX_BACKOFF,
            "and never exceeds it"
        );
    }
}
