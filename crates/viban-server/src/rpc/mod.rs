//! Hand-rolled JSON-RPC 2.0 request handling.
//!
//! Methods are namespaced `<area>.<action>`. A running agent streams its
//! output as `events.update` notifications, and every session and message is
//! persisted to SQLite so conversations survive a restart.
//!
//! This module owns the transport-facing pieces — request/response framing,
//! the error type, dispatch — while the handlers live in per-area submodules
//! (`sessions`, `tasks`, `attempts`, `review`). Handlers return
//! `Result<Value, RpcError>`; any `anyhow::Error` from `viban-core` converts
//! into an internal `RpcError` via `?` (see the `From` impl below).

mod attempts;
mod review;
mod sessions;
mod tasks;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex};

use viban_core::agents::ClaudeSession;
use viban_core::db::Db;

/// Running agent sessions for one connection, keyed by viban session id.
pub type SessionRegistry = Arc<Mutex<HashMap<String, ClaudeSession>>>;

/// Shared state for method handlers.
pub struct Context {
    /// The user's project folder. viban never writes into it.
    pub workspace: PathBuf,
    /// viban's own data directory for this project (database, worktrees).
    pub data_dir: PathBuf,
    pub db: Db,
}

#[derive(Debug, Deserialize)]
struct Request {
    id: Value,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct Response {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
struct RpcError {
    code: i32,
    message: String,
}

impl RpcError {
    fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    fn method_not_found(method: &str) -> Self {
        Self::new(-32601, format!("method not found: {method}"))
    }
    fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(-32602, message)
    }
    fn internal(message: impl Into<String>) -> Self {
        Self::new(-32603, message)
    }
}

/// Any `anyhow` error from the core crate becomes an internal JSON-RPC error,
/// carrying the full context chain (`{:#}`) as the message. This lets handlers
/// use `?` directly on `viban-core` calls instead of mapping every error.
impl From<anyhow::Error> for RpcError {
    fn from(err: anyhow::Error) -> Self {
        RpcError::internal(format!("{err:#}"))
    }
}

/// Parses a raw JSON-RPC request, dispatches it, and returns the serialized
/// response.
pub async fn handle(
    raw: &str,
    ctx: &Context,
    registry: &SessionRegistry,
    outbound: &mpsc::UnboundedSender<String>,
) -> String {
    let request: Request = match serde_json::from_str(raw) {
        Ok(request) => request,
        Err(err) => {
            return serialize(Response {
                jsonrpc: "2.0",
                id: Value::Null,
                result: None,
                error: Some(RpcError::new(-32700, format!("parse error: {err}"))),
            });
        }
    };

    let response = match dispatch(&request.method, request.params, ctx, registry, outbound).await {
        Ok(result) => Response {
            jsonrpc: "2.0",
            id: request.id,
            result: Some(result),
            error: None,
        },
        Err(error) => Response {
            jsonrpc: "2.0",
            id: request.id,
            result: None,
            error: Some(error),
        },
    };
    serialize(response)
}

async fn dispatch(
    method: &str,
    params: Value,
    ctx: &Context,
    registry: &SessionRegistry,
    outbound: &mpsc::UnboundedSender<String>,
) -> Result<Value, RpcError> {
    match method {
        "server.health" => Ok(json!({
            "status": "ok",
            "version": viban_core::VERSION,
            "workspace": ctx.workspace.display().to_string(),
        })),
        "agents.spawn" => sessions::spawn(params, ctx, registry, outbound).await,
        "sessions.send_message" => sessions::send_message(params, ctx, registry, outbound).await,
        "sessions.list" => sessions::list(ctx).await,
        "sessions.get" => sessions::get(params, ctx).await,
        "boards.get" => tasks::get_board(ctx).await,
        "tasks.create" => tasks::create(params, ctx).await,
        "tasks.update" => tasks::update(params, ctx).await,
        "tasks.delete" => tasks::delete(params, ctx, registry).await,
        "tasks.reorder" => tasks::reorder(params, ctx).await,
        "tasks.start_session" => attempts::start_session(params, ctx).await,
        "attempts.create" => attempts::create(params, ctx).await,
        "attempts.list" => attempts::list(params, ctx).await,
        "attempts.activate" => attempts::activate(params, ctx).await,
        "git.diff" => review::diff(params, ctx).await,
        "git.commit" => review::commit(params, ctx).await,
        "git.restore" => review::restore(params, ctx).await,
        "git.merge" => review::merge(params, ctx).await,
        other => Err(RpcError::method_not_found(other)),
    }
}

/// Milliseconds since the Unix epoch — the timestamp on every persisted row.
fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

/// Extracts a required string parameter, erroring with `invalid_params`.
fn str_param<'a>(params: &'a Value, key: &str) -> Result<&'a str, RpcError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::invalid_params(format!("missing or non-string '{key}'")))
}

fn serialize(response: Response) -> String {
    serde_json::to_string(&response).unwrap_or_else(|_| {
        r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"internal error"}}"#
            .to_string()
    })
}

#[cfg(test)]
mod test_support {
    //! Shared fixtures for the handler unit tests in each submodule.

    use serde_json::json;
    use tempfile::TempDir;
    use viban_core::db::Db;

    use super::{tasks, Context};

    /// An in-memory `Context` with the default board. The returned `TempDir`s
    /// back `workspace` / `data_dir` and must be kept alive for the test.
    pub(super) async fn context() -> (Context, TempDir, TempDir) {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let data_dir = tempfile::tempdir().expect("data tempdir");
        let db = Db::open_in_memory().await.expect("in-memory db");
        db.ensure_default_board(&workspace.path().to_string_lossy())
            .await
            .expect("default board");
        let ctx = Context {
            workspace: workspace.path().to_path_buf(),
            data_dir: data_dir.path().to_path_buf(),
            db,
        };
        (ctx, workspace, data_dir)
    }

    /// Creates a task in the board's first column, returning its id.
    pub(super) async fn task(ctx: &Context, title: &str) -> String {
        let board = tasks::get_board(ctx).await.expect("get_board");
        let column_id = board["columns"][0]["id"]
            .as_str()
            .expect("a column id")
            .to_string();
        let created = tasks::create(json!({ "column_id": column_id, "title": title }), ctx)
            .await
            .expect("create task");
        created["task"]["id"]
            .as_str()
            .expect("a task id")
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::RpcError;

    #[test]
    fn anyhow_errors_convert_to_internal_rpc_errors() {
        let err: RpcError = anyhow::anyhow!("disk on fire").into();
        assert_eq!(err.code, -32603);
        assert!(err.message.contains("disk on fire"));
    }

    #[test]
    fn converted_errors_keep_the_anyhow_context_chain() {
        let err: RpcError = anyhow::anyhow!("root cause")
            .context("while doing the thing")
            .into();
        // `{:#}` flattens the whole chain into the message.
        assert!(err.message.contains("while doing the thing"));
        assert!(err.message.contains("root cause"));
    }

    #[test]
    fn str_param_rejects_a_missing_key() {
        let params = serde_json::json!({ "present": "yes" });
        let err = super::str_param(&params, "absent").expect_err("missing key errors");
        assert_eq!(err.code, -32602);
    }
}
