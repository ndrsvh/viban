//! End-to-end test of the Phase 4 worktree flow: spawns the real
//! `viban-server` binary against a temporary git repository and drives it over
//! a WebSocket, asserting that `tasks.start_session` creates an isolated git
//! worktree + branch and `tasks.delete` tears them down.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::Command;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Runs `git args` in `dir`, panicking on failure.
fn git(dir: &Path, args: &[&str]) {
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
fn git_output(dir: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git should run");
    String::from_utf8(output.stdout).expect("git output is utf-8")
}

/// One JSON-RPC round-trip: sends a request and returns its `result`,
/// skipping any notifications that arrive in between.
async fn call(ws: &mut Ws, id: i64, method: &str, params: Value) -> Value {
    let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
    ws.send(Message::text(request.to_string()))
        .await
        .expect("send request");
    loop {
        let message = ws
            .next()
            .await
            .expect("a response")
            .expect("websocket is healthy");
        let Message::Text(text) = message else {
            continue;
        };
        let value: Value = serde_json::from_str(&text).expect("response is json");
        if value.get("id").and_then(Value::as_i64) != Some(id) {
            continue; // a notification or an unrelated response
        }
        if let Some(error) = value.get("error") {
            panic!("rpc error from {method}: {error}");
        }
        return value.get("result").cloned().unwrap_or(Value::Null);
    }
}

#[tokio::test]
async fn worktree_lifecycle() {
    // A real git repo with one commit — `git worktree add` needs a HEAD.
    let repo = tempfile::tempdir().expect("create temp dir");
    let root = repo.path();
    git(root, &["init"]);
    git(root, &["config", "user.email", "test@viban.dev"]);
    git(root, &["config", "user.name", "viban test"]);
    std::fs::write(root.join("README.md"), "viban test repo\n").expect("write README");
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "initial"]);

    // Spawn the real server binary against the repo.
    let token = "test-token-0123456789abcdef";
    let mut server = Command::new(env!("CARGO_BIN_EXE_viban-server"))
        .args(["--port", "0", "--workspace", &root.to_string_lossy()])
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

    // Connect and authenticate with the shared token.
    let (mut ws, _) = connect_async(format!("ws://127.0.0.1:{port}"))
        .await
        .expect("websocket connects");
    ws.send(Message::text(token)).await.expect("send token");

    // The default board exists with its four columns.
    let board = call(&mut ws, 1, "boards.get", Value::Null).await;
    let columns = board["columns"].as_array().expect("columns array");
    assert_eq!(columns.len(), 4, "default board has four columns");
    let column_id = columns[0]["id"].as_str().expect("a column id").to_string();

    // Create a task.
    let created = call(
        &mut ws,
        2,
        "tasks.create",
        json!({ "column_id": column_id, "title": "Add login flow" }),
    )
    .await;
    let task_id = created["task"]["id"]
        .as_str()
        .expect("a task id")
        .to_string();

    // Start a session: this must create the worktree and branch.
    let started = call(
        &mut ws,
        3,
        "tasks.start_session",
        json!({ "task_id": task_id }),
    )
    .await;
    let session_id = started["session_id"]
        .as_str()
        .expect("start_session returns a session id")
        .to_string();

    // The worktree directory exists on disk.
    let worktree = root.join(".viban").join("worktrees").join(&task_id);
    assert!(worktree.is_dir(), "worktree directory was created");

    // The task now carries a slugified branch name.
    let board = call(&mut ws, 4, "boards.get", Value::Null).await;
    let task = board["tasks"]
        .as_array()
        .expect("tasks array")
        .iter()
        .find(|task| task["id"].as_str() == Some(&task_id))
        .expect("the created task");
    let branch = task["branch"]
        .as_str()
        .expect("the task has a branch")
        .to_string();
    assert!(
        branch.starts_with("viban/add-login-flow-"),
        "branch name is slugified: {branch}"
    );

    // git itself knows about the worktree and the branch.
    let worktrees = git_output(root, &["worktree", "list"]);
    assert!(
        worktrees.contains(task_id.as_str()),
        "git lists the new worktree:\n{worktrees}"
    );
    let branches = git_output(root, &["branch", "--list", &branch]);
    assert!(!branches.trim().is_empty(), "git lists the new branch");

    // `start_session` is idempotent — a second call returns the same session.
    let again = call(
        &mut ws,
        5,
        "tasks.start_session",
        json!({ "task_id": task_id }),
    )
    .await;
    assert_eq!(
        again["session_id"].as_str(),
        Some(session_id.as_str()),
        "a second start returns the existing session"
    );

    // The server appended `.viban/` to the project's .gitignore.
    let gitignore = std::fs::read_to_string(root.join(".gitignore")).unwrap_or_default();
    assert!(
        gitignore.lines().any(|line| line.trim() == ".viban/"),
        ".viban/ is gitignored:\n{gitignore}"
    );

    // Delete the task: the worktree and branch must be torn down.
    call(&mut ws, 6, "tasks.delete", json!({ "task_id": task_id })).await;
    assert!(!worktree.exists(), "worktree directory was removed");
    let branches = git_output(root, &["branch", "--list", &branch]);
    assert!(
        branches.trim().is_empty(),
        "branch was deleted, git still lists: {branches:?}"
    );

    // Closing the socket makes the local-mode server exit.
    ws.close(None).await.ok();
    let _ = tokio::time::timeout(Duration::from_secs(10), server.wait()).await;
}
