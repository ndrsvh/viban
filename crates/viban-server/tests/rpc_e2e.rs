//! Integration tests for the JSON-RPC surface that does not need a live
//! `claude` binary: server health, board and task CRUD, session listing, and
//! error handling. Each test drives the real server binary over a WebSocket.

mod common;

use serde_json::{json, Value};

use common::TestServer;

#[tokio::test]
async fn server_health_reports_ok() {
    let mut server = TestServer::start().await;
    let health = server.call("server.health", Value::Null).await;
    assert_eq!(health["status"], "ok");
    assert!(health["version"].as_str().is_some(), "version is reported");
    assert!(
        health["workspace"].as_str().is_some(),
        "workspace is reported"
    );
}

#[tokio::test]
async fn a_connection_with_the_wrong_token_is_closed() {
    let server = TestServer::start().await;
    assert!(
        server.connection_rejected("not-the-real-token").await,
        "the server must close a mis-authenticated connection",
    );
}

#[tokio::test]
async fn board_starts_with_four_columns_and_no_tasks() {
    let mut server = TestServer::start().await;
    let board = server.call("boards.get", Value::Null).await;
    let columns = board["columns"].as_array().expect("columns array");
    let names: Vec<&str> = columns
        .iter()
        .filter_map(|column| column["name"].as_str())
        .collect();
    assert_eq!(names, ["Backlog", "In Progress", "Review", "Done"]);
    assert_eq!(
        board["tasks"].as_array().expect("tasks array").len(),
        0,
        "a fresh board has no tasks"
    );
}

#[tokio::test]
async fn tasks_create_update_and_delete() {
    let mut server = TestServer::start().await;
    let column_id = server.first_column_id().await;

    let created = server
        .call(
            "tasks.create",
            json!({ "column_id": column_id, "title": "First", "description": "desc" }),
        )
        .await;
    let task_id = created["task"]["id"]
        .as_str()
        .expect("a task id")
        .to_string();
    assert_eq!(created["task"]["title"], "First");
    assert_eq!(
        created["task"]["position"], 0,
        "first task is at position 0"
    );

    // A second task in the same column lands at the next position.
    let second = server
        .call(
            "tasks.create",
            json!({ "column_id": column_id, "title": "Second" }),
        )
        .await;
    assert_eq!(second["task"]["position"], 1);

    // Update mutates only the supplied fields.
    let updated = server
        .call(
            "tasks.update",
            json!({ "task_id": task_id, "title": "Renamed", "description": "new" }),
        )
        .await;
    assert_eq!(updated["task"]["title"], "Renamed");
    assert_eq!(updated["task"]["description"], "new");

    // Delete removes the task from the board.
    server
        .call("tasks.delete", json!({ "task_id": task_id }))
        .await;
    let board = server.call("boards.get", Value::Null).await;
    let remaining: Vec<&str> = board["tasks"]
        .as_array()
        .expect("tasks array")
        .iter()
        .filter_map(|task| task["id"].as_str())
        .collect();
    assert!(!remaining.contains(&task_id.as_str()), "task was deleted");
}

#[tokio::test]
async fn tasks_reorder_moves_a_task_between_columns() {
    let mut server = TestServer::start().await;
    let board = server.call("boards.get", Value::Null).await;
    let backlog = board["columns"][0]["id"]
        .as_str()
        .expect("backlog id")
        .to_string();
    let in_progress = board["columns"][1]["id"]
        .as_str()
        .expect("in progress id")
        .to_string();

    let created = server
        .call(
            "tasks.create",
            json!({ "column_id": backlog, "title": "A" }),
        )
        .await;
    let task_id = created["task"]["id"]
        .as_str()
        .expect("a task id")
        .to_string();

    server
        .call(
            "tasks.reorder",
            json!({ "column_id": in_progress, "task_ids": [task_id] }),
        )
        .await;

    let board = server.call("boards.get", Value::Null).await;
    let task = board["tasks"]
        .as_array()
        .expect("tasks array")
        .iter()
        .find(|task| task["id"].as_str() == Some(&task_id))
        .expect("the task");
    assert_eq!(
        task["column_id"].as_str(),
        Some(in_progress.as_str()),
        "the task moved to the In Progress column"
    );
}

#[tokio::test]
async fn sessions_list_is_empty_for_a_fresh_workspace() {
    let mut server = TestServer::start().await;
    let result = server.call("sessions.list", Value::Null).await;
    assert_eq!(
        result["sessions"].as_array().expect("sessions array").len(),
        0
    );
}

#[tokio::test]
async fn an_unknown_method_returns_method_not_found() {
    let mut server = TestServer::start().await;
    let error = server
        .call_expecting_error("does.not_exist", Value::Null)
        .await;
    assert_eq!(error["code"], -32601, "method-not-found error code");
}

#[tokio::test]
async fn missing_required_params_return_invalid_params() {
    let mut server = TestServer::start().await;
    // tasks.create without the required column_id.
    let error = server
        .call_expecting_error("tasks.create", json!({ "title": "x" }))
        .await;
    assert_eq!(error["code"], -32602, "invalid-params error code");
}

#[tokio::test]
async fn operations_on_unknown_ids_return_errors() {
    let mut server = TestServer::start().await;

    let error = server
        .call_expecting_error("sessions.get", json!({ "session_id": "nope" }))
        .await;
    assert_eq!(error["code"], -32602);

    let error = server
        .call_expecting_error("tasks.update", json!({ "task_id": "nope", "title": "x" }))
        .await;
    assert_eq!(error["code"], -32602);

    let error = server
        .call_expecting_error("tasks.start_session", json!({ "task_id": "nope" }))
        .await;
    assert_eq!(error["code"], -32602);
}

#[tokio::test]
async fn malformed_json_returns_a_structured_parse_error() {
    let mut server = TestServer::start().await;
    // A frame that is not valid JSON still gets a structured error back
    // rather than dropping the connection.
    let response = server.send_raw("this is not json").await;
    assert_eq!(response["error"]["code"], -32700, "parse-error code");
    assert!(response["id"].is_null(), "a parse error has a null id");
}
