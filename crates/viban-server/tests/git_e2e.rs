//! Integration tests for the diff-review RPC surface: `git.diff`,
//! `git.commit`, and `git.restore` against a task's worktree.

mod common;

use serde_json::{json, Value};

use common::{worktree_path, TestServer};

/// Creates a task, starts its session (and worktree), returns the task id.
async fn task_with_worktree(server: &mut TestServer) -> String {
    let column_id = server.first_column_id().await;
    let created = server
        .call(
            "tasks.create",
            json!({ "column_id": column_id, "title": "Review me" }),
        )
        .await;
    let task_id = created["task"]["id"]
        .as_str()
        .expect("a task id")
        .to_string();
    server
        .call("tasks.start_session", json!({ "task_id": task_id }))
        .await;
    task_id
}

/// The id of the board column named `name`.
async fn column_id(server: &mut TestServer, name: &str) -> String {
    let board = server.call("boards.get", Value::Null).await;
    board["columns"]
        .as_array()
        .expect("columns")
        .iter()
        .find(|column| column["name"] == name)
        .unwrap_or_else(|| panic!("no column named {name}"))["id"]
        .as_str()
        .expect("a column id")
        .to_string()
}

#[tokio::test]
async fn git_diff_lists_pending_worktree_changes() {
    let mut server = TestServer::start().await;
    let task_id = task_with_worktree(&mut server).await;

    let worktree = worktree_path(server.workspace(), &task_id);
    std::fs::write(worktree.join("hello.txt"), "agent output\n").expect("write");

    let result = server.call("git.diff", json!({ "task_id": task_id })).await;
    let files = result["files"].as_array().expect("files array");
    assert_eq!(files.len(), 1, "one file changed");
    assert_eq!(files[0]["path"], "hello.txt");
    assert_eq!(files[0]["status"], "added");
    assert_eq!(files[0]["new_text"], "agent output\n");
}

#[tokio::test]
async fn git_commit_commits_changes_and_moves_the_task_to_review() {
    let mut server = TestServer::start().await;
    let task_id = task_with_worktree(&mut server).await;
    let worktree = worktree_path(server.workspace(), &task_id);
    std::fs::write(worktree.join("hello.txt"), "agent output\n").expect("write");

    server
        .call("git.commit", json!({ "task_id": task_id }))
        .await;

    // Nothing is pending after the commit.
    let result = server.call("git.diff", json!({ "task_id": task_id })).await;
    assert_eq!(result["files"].as_array().expect("files").len(), 0);

    // The task has moved into the Review column.
    let review = column_id(&mut server, "Review").await;
    let board = server.call("boards.get", Value::Null).await;
    let task = board["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .find(|task| task["id"].as_str() == Some(&task_id))
        .expect("the task");
    assert_eq!(task["column_id"].as_str(), Some(review.as_str()));
}

#[tokio::test]
async fn git_restore_discards_changes_and_moves_the_task_to_in_progress() {
    let mut server = TestServer::start().await;
    let task_id = task_with_worktree(&mut server).await;
    let worktree = worktree_path(server.workspace(), &task_id);
    std::fs::write(worktree.join("junk.txt"), "discard me\n").expect("write");

    server
        .call("git.restore", json!({ "task_id": task_id }))
        .await;

    let result = server.call("git.diff", json!({ "task_id": task_id })).await;
    assert_eq!(result["files"].as_array().expect("files").len(), 0);
    assert!(
        !worktree.join("junk.txt").exists(),
        "the untracked file was removed"
    );

    let in_progress = column_id(&mut server, "In Progress").await;
    let board = server.call("boards.get", Value::Null).await;
    let task = board["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .find(|task| task["id"].as_str() == Some(&task_id))
        .expect("the task");
    assert_eq!(task["column_id"].as_str(), Some(in_progress.as_str()));
}

#[tokio::test]
async fn git_diff_on_a_task_without_a_worktree_errors() {
    let mut server = TestServer::start().await;
    let board_column = server.first_column_id().await;
    let created = server
        .call(
            "tasks.create",
            json!({ "column_id": board_column, "title": "no session yet" }),
        )
        .await;
    let task_id = created["task"]["id"]
        .as_str()
        .expect("a task id")
        .to_string();

    let error = server
        .call_expecting_error("git.diff", json!({ "task_id": task_id }))
        .await;
    assert_eq!(error["code"], -32602);
}
