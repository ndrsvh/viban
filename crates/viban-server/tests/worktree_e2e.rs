//! End-to-end test of the Phase 4 worktree flow: drives the real
//! `viban-server` binary over a WebSocket and asserts that
//! `tasks.start_session` creates an isolated git worktree + branch and
//! `tasks.delete` tears them down.

mod common;

use serde_json::{json, Value};

use common::{git_output, TestServer};

#[tokio::test]
async fn worktree_lifecycle() {
    let mut server = TestServer::start().await;

    let column_id = server.first_column_id().await;
    let created = server
        .call(
            "tasks.create",
            json!({ "column_id": column_id, "title": "Add login flow" }),
        )
        .await;
    let task_id = created["task"]["id"]
        .as_str()
        .expect("a task id")
        .to_string();

    // Start a session: this must create the worktree and branch.
    let started = server
        .call("tasks.start_session", json!({ "task_id": task_id }))
        .await;
    let session_id = started["session_id"]
        .as_str()
        .expect("start_session returns a session id")
        .to_string();

    // The worktree directory exists on disk.
    let worktree = server.task_worktree(&task_id).await;
    assert!(worktree.is_dir(), "worktree directory was created");

    // The task now carries a slugified branch name.
    let board = server.call("boards.get", Value::Null).await;
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
    let worktree_name = worktree
        .file_name()
        .and_then(|name| name.to_str())
        .expect("worktree directory name");
    let worktrees = git_output(server.workspace(), &["worktree", "list"]);
    assert!(
        worktrees.contains(worktree_name),
        "git lists the new worktree:\n{worktrees}"
    );
    let branches = git_output(server.workspace(), &["branch", "--list", &branch]);
    assert!(!branches.trim().is_empty(), "git lists the new branch");

    // `start_session` is idempotent — a second call returns the same session.
    let again = server
        .call("tasks.start_session", json!({ "task_id": task_id }))
        .await;
    assert_eq!(
        again["session_id"].as_str(),
        Some(session_id.as_str()),
        "a second start returns the existing session"
    );

    // viban keeps nothing in the project folder — not even a .viban directory.
    assert!(
        !server.workspace().join(".viban").exists(),
        "the project folder stays free of viban's data"
    );

    // Delete the task: the worktree and branch must be torn down.
    server
        .call("tasks.delete", json!({ "task_id": task_id }))
        .await;
    assert!(!worktree.exists(), "worktree directory was removed");
    let branches = git_output(server.workspace(), &["branch", "--list", &branch]);
    assert!(
        branches.trim().is_empty(),
        "branch was deleted, git still lists: {branches:?}"
    );
}
