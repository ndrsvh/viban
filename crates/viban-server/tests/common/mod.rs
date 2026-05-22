//! Shared harness for viban-server integration tests: spawns the real server
//! binary against a temporary git repository and speaks JSON-RPC over a
//! WebSocket. Each test file pulls in only the helpers it needs, so unused
//! ones are expected.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// A running viban-server with an authenticated client connection. The server
/// process is killed when this value is dropped.
pub struct TestServer {
    /// The temporary git repository the server operates on.
    pub root: tempfile::TempDir,
    server: Child,
    ws: Ws,
    next_id: i64,
}

impl TestServer {
    /// Spawns the server against a fresh temp git repo and authenticates.
    pub async fn start() -> Self {
        Self::start_inner(true).await
    }

    /// Spawns the server against a plain (non-git) temp folder.
    pub async fn start_without_git() -> Self {
        Self::start_inner(false).await
    }

    async fn start_inner(with_git: bool) -> Self {
        let root = tempfile::tempdir().expect("create temp dir");
        if with_git {
            init_git_repo(root.path());
        }

        let token = "integration-test-token";
        let mut server = Command::new(env!("CARGO_BIN_EXE_viban-server"))
            .args(["--port", "0", "--workspace", &root.path().to_string_lossy()])
            .env("VIBAN_AUTH_TOKEN", token)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("spawn viban-server");

        // The first stdout line is the `{"ready":true,"port":N}` handshake.
        let stdout = server.stdout.take().expect("server stdout");
        let mut lines = BufReader::new(stdout).lines();
        let ready = tokio::time::timeout(Duration::from_secs(30), lines.next_line())
            .await
            .expect("server is ready in time")
            .expect("read server stdout")
            .expect("a ready line");
        let ready: Value = serde_json::from_str(&ready).expect("ready line is json");
        let port = ready["port"].as_u64().expect("a port number");

        let (mut ws, _) = connect_async(format!("ws://127.0.0.1:{port}"))
            .await
            .expect("websocket connects");
        ws.send(Message::text(token)).await.expect("send token");

        Self {
            root,
            server,
            ws,
            next_id: 1,
        }
    }

    /// The workspace path the server runs against.
    pub fn workspace(&self) -> &Path {
        self.root.path()
    }

    /// Sends a JSON-RPC request and returns the full response object,
    /// skipping any notifications that arrive in between.
    pub async fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.ws
            .send(Message::text(request.to_string()))
            .await
            .expect("send request");
        loop {
            let message = self
                .ws
                .next()
                .await
                .expect("a response")
                .expect("websocket is healthy");
            let Message::Text(text) = message else {
                continue;
            };
            let value: Value = serde_json::from_str(&text).expect("response is json");
            if value.get("id").and_then(Value::as_i64) == Some(id) {
                return value;
            }
        }
    }

    /// Sends a request that must succeed, returning its `result`.
    pub async fn call(&mut self, method: &str, params: Value) -> Value {
        let response = self.request(method, params).await;
        if let Some(error) = response.get("error") {
            panic!("unexpected rpc error from {method}: {error}");
        }
        response.get("result").cloned().unwrap_or(Value::Null)
    }

    /// Sends a raw (possibly malformed) frame and returns the next response.
    pub async fn send_raw(&mut self, raw: &str) -> Value {
        self.ws
            .send(Message::text(raw.to_string()))
            .await
            .expect("send raw frame");
        loop {
            let message = self
                .ws
                .next()
                .await
                .expect("a response")
                .expect("websocket is healthy");
            if let Message::Text(text) = message {
                return serde_json::from_str(&text).expect("response is json");
            }
        }
    }

    /// Sends a request that must fail, returning its `error` object.
    pub async fn call_expecting_error(&mut self, method: &str, params: Value) -> Value {
        let response = self.request(method, params).await;
        response
            .get("error")
            .cloned()
            .unwrap_or_else(|| panic!("expected an error from {method}, got {response}"))
    }

    /// The id of the first column on the default board.
    pub async fn first_column_id(&mut self) -> String {
        let board = self.call("boards.get", Value::Null).await;
        board["columns"][0]["id"]
            .as_str()
            .expect("a column id")
            .to_string()
    }
}

/// Runs `git args` in `dir`, panicking on failure.
pub fn run_git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("git should run");
    assert!(status.success(), "git {args:?} failed");
}

/// Captures the stdout of `git args` run in `dir`.
pub fn git_output(dir: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git should run");
    String::from_utf8(output.stdout).expect("git output is utf-8")
}

/// Creates a git repo with a single commit at `dir`.
pub fn init_git_repo(dir: &Path) {
    run_git(dir, &["init"]);
    run_git(dir, &["config", "user.email", "test@viban.dev"]);
    run_git(dir, &["config", "user.name", "viban test"]);
    run_git(dir, &["config", "core.autocrlf", "false"]);
    std::fs::write(dir.join("README.md"), "viban test repo\n").expect("write README");
    run_git(dir, &["add", "."]);
    run_git(dir, &["commit", "-m", "initial"]);
}

/// The on-disk worktree path the server uses for `task_id`.
pub fn worktree_path(root: &Path, task_id: &str) -> PathBuf {
    root.join(".viban").join("worktrees").join(task_id)
}
