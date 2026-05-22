//! Integration tests for the attempts RPC surface: a task may have several
//! agent attempts, each in its own worktree, and the active one can be
//! switched.

mod common;

use serde_json::json;

use common::TestServer;

/// Creates a task and starts its first attempt, returning the task id.
async fn task_with_attempt(server: &mut TestServer) -> String {
    let column_id = server.first_column_id().await;
    let created = server
        .call(
            "tasks.create",
            json!({ "column_id": column_id, "title": "Iterate" }),
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

#[tokio::test]
async fn start_session_records_an_attempt() {
    let mut server = TestServer::start().await;
    let task_id = task_with_attempt(&mut server).await;

    let result = server
        .call("attempts.list", json!({ "task_id": task_id }))
        .await;
    let attempts = result["attempts"].as_array().expect("attempts array");
    assert_eq!(attempts.len(), 1, "the first session records one attempt");
    assert!(attempts[0]["session_id"].as_str().is_some());
    assert!(attempts[0]["worktree_path"].as_str().is_some());
}

#[tokio::test]
async fn attempts_create_makes_a_second_isolated_attempt() {
    let mut server = TestServer::start().await;
    let task_id = task_with_attempt(&mut server).await;
    let first_worktree = server.task_worktree(&task_id).await;

    server
        .call("attempts.create", json!({ "task_id": task_id }))
        .await;

    let result = server
        .call("attempts.list", json!({ "task_id": task_id }))
        .await;
    let attempts = result["attempts"].as_array().expect("attempts array");
    assert_eq!(attempts.len(), 2, "the task now has two attempts");

    let second_worktree = server.task_worktree(&task_id).await;
    assert_ne!(
        first_worktree, second_worktree,
        "each attempt has its own worktree"
    );
    assert!(
        first_worktree.is_dir(),
        "the first attempt's worktree is still on disk"
    );
    assert!(
        second_worktree.is_dir(),
        "the second attempt's worktree exists"
    );

    let branches: Vec<&str> = attempts
        .iter()
        .filter_map(|attempt| attempt["branch"].as_str())
        .collect();
    assert_eq!(branches.len(), 2);
    assert_ne!(
        branches[0], branches[1],
        "the attempts have distinct branches"
    );
}

#[tokio::test]
async fn attempts_activate_repoints_the_task() {
    let mut server = TestServer::start().await;
    let task_id = task_with_attempt(&mut server).await;

    let listed = server
        .call("attempts.list", json!({ "task_id": task_id }))
        .await;
    let first_attempt_id = listed["attempts"][0]["id"]
        .as_str()
        .expect("an attempt id")
        .to_string();
    let first_session = listed["attempts"][0]["session_id"]
        .as_str()
        .expect("a session id")
        .to_string();

    // A second attempt becomes the active one.
    server
        .call("attempts.create", json!({ "task_id": task_id }))
        .await;
    let active = server.task(&task_id).await;
    assert_ne!(
        active["session_id"].as_str(),
        Some(first_session.as_str()),
        "the new attempt is active"
    );

    // Activating the first attempt repoints the task back at it.
    server
        .call(
            "attempts.activate",
            json!({ "attempt_id": first_attempt_id }),
        )
        .await;
    let task = server.task(&task_id).await;
    assert_eq!(task["session_id"].as_str(), Some(first_session.as_str()));
}

#[tokio::test]
async fn deleting_a_task_removes_every_attempt_worktree() {
    let mut server = TestServer::start().await;
    let task_id = task_with_attempt(&mut server).await;
    let first = server.task_worktree(&task_id).await;
    server
        .call("attempts.create", json!({ "task_id": task_id }))
        .await;
    let second = server.task_worktree(&task_id).await;
    assert!(first.is_dir() && second.is_dir());

    server
        .call("tasks.delete", json!({ "task_id": task_id }))
        .await;
    assert!(!first.exists(), "the first attempt's worktree was removed");
    assert!(
        !second.exists(),
        "the second attempt's worktree was removed"
    );
}
