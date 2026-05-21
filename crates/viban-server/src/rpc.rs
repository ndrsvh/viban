//! Hand-rolled JSON-RPC 2.0 request handling.
//!
//! Methods are namespaced `<area>.<action>`. Only `server.health` exists so
//! far; the task/session/git/agent areas land in later phases.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Shared, read-only state available to every method handler.
pub struct Context {
    pub workspace: PathBuf,
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

/// Parses a raw JSON-RPC request and returns the serialized response.
pub fn handle(raw: &str, ctx: &Context) -> String {
    let request: Request = match serde_json::from_str(raw) {
        Ok(req) => req,
        Err(err) => {
            return serialize(Response {
                jsonrpc: "2.0",
                id: Value::Null,
                result: None,
                error: Some(RpcError {
                    code: -32700,
                    message: format!("parse error: {err}"),
                }),
            });
        }
    };

    let response = match dispatch(&request.method, &request.params, ctx) {
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

fn dispatch(method: &str, _params: &Value, ctx: &Context) -> Result<Value, RpcError> {
    match method {
        "server.health" => Ok(serde_json::json!({
            "status": "ok",
            "version": viban_core::VERSION,
            "workspace": ctx.workspace.display().to_string(),
        })),
        other => Err(RpcError {
            code: -32601,
            message: format!("method not found: {other}"),
        }),
    }
}

fn serialize(response: Response) -> String {
    serde_json::to_string(&response).unwrap_or_else(|_| {
        r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"internal error"}}"#
            .to_string()
    })
}
