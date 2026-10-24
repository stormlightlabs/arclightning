//! The MCP tools against real projects: the tool list and its schemas, the
//! agent loop, pagination, failures, the workspace sync and stdio.

use std::collections::BTreeSet;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

use arcl_mcp::params::Parameters;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use arcl_core::domain::{TaskId, TaskPriority, TaskStatus};
use arcl_mcp::{error::ToolError, server::ArclServer, tools::*};
use arcl_repo::workspace::{ProjectConfig, WorkspaceManifest, encode_manifest};
use arcl_store::{Database, PlanningTaskUpdate};

const TOOLS: &[(&str, u64)] = &[
    ("capture_create", 1),
    ("context", 2),
    ("explain", 2),
    ("list", 1),
    ("note_create", 1),
    ("plan_create", 1),
    ("ready", 1),
    ("show", 2),
    ("spec_create", 1),
    ("task_complete", 2),
    ("task_create", 1),
    ("task_handoff", 1),
    ("task_start", 1),
    ("task_unpark", 1),
    ("task_update", 1),
];

#[test]
fn tools_publish_versions_and_schemas() {
    let project = project(false);
    let server = ArclServer::new(project.path().to_owned());
    let tools = server.tools();

    let listed = tools
        .iter()
        .map(|tool| {
            let version = tool
                .meta
                .as_ref()
                .and_then(|meta| meta.0.get("arcl/format_version"))
                .and_then(Value::as_u64);
            (tool.name.to_string(), version)
        })
        .collect::<BTreeSet<_>>();
    let expected = TOOLS
        .iter()
        .map(|(name, version)| ((*name).to_owned(), Some(*version)))
        .collect::<BTreeSet<_>>();
    assert_eq!(listed, expected);

    for tool in &tools {
        let output = tool.output_schema.as_ref().expect("every tool has an output schema");
        assert_eq!(output["required"], json!(["format_version", "data"]), "{}", tool.name);
        assert_eq!(tool.input_schema["additionalProperties"], json!(false), "{}", tool.name);
    }

    let start = tools
        .iter()
        .find(|tool| tool.name == "task_start")
        .expect("task_start is listed");
    assert_eq!(
        start.input_schema["properties"],
        json!({ "task_id": start.input_schema["properties"]["task_id"] })
    );

    let list = tools.iter().find(|tool| tool.name == "list").expect("list is listed");
    assert_eq!(list.input_schema["properties"]["limit"]["minimum"], json!(1));
    assert_eq!(list.input_schema["properties"]["limit"]["maximum"], json!(MAX_LIMIT));
}

#[tokio::test]
async fn an_agent_claims_hands_off_and_completes_work() {
    let project = project(false);
    let server = ArclServer::new(project.path().to_owned());

    let first = create_task(&server, "First", &[]).await;
    let second = create_task(&server, "Second", &[&first]).await;

    let ready = server
        .ready(Parameters::from(ReadyInput::default()), CancellationToken::new())
        .await
        .expect("ready")
        .0;
    assert_eq!(ready.format_version, 1);
    assert_eq!(ids(&ready.data.tasks), vec![first.clone()]);

    let refused = server
        .task_start(
            Parameters::from(TaskInput { task_id: second.clone() }),
            CancellationToken::new(),
        )
        .await
        .err()
        .expect("a blocked task does not start");
    assert_eq!((refused.code(), refused.exit_code()), ("invalid_request", 3));

    let explained = server
        .explain(
            Parameters::from(TaskInput { task_id: second.clone() }),
            CancellationToken::new(),
        )
        .await
        .expect("explain")
        .0;
    assert_eq!(explained.format_version, 2);
    let reason = serde_json::to_value(&explained.data.readiness.reasons[0]).expect("reason serializes");
    assert_eq!(
        (reason["kind"].clone(), reason["blocker"].clone()),
        (json!("direct_blocker"), json!(first))
    );

    let started = server
        .task_start(
            Parameters::from(TaskInput { task_id: first.clone() }),
            CancellationToken::new(),
        )
        .await
        .expect("start")
        .0;
    assert_eq!(
        (started.data.action.as_str(), started.data.task.status),
        ("started", TaskStatus::InProgress)
    );

    let handed_off = server
        .task_handoff(
            Parameters::from(TaskHandoffInput { task_id: first.clone(), note: "Half done.".to_owned() }),
            CancellationToken::new(),
        )
        .await
        .expect("handoff")
        .0;
    assert_eq!(handed_off.data.task.status, TaskStatus::Parked);

    server
        .task_unpark(
            Parameters::from(TaskInput { task_id: first.clone() }),
            CancellationToken::new(),
        )
        .await
        .expect("unpark");
    server
        .task_start(
            Parameters::from(TaskInput { task_id: first.clone() }),
            CancellationToken::new(),
        )
        .await
        .expect("start again");
    let context = server
        .context(
            Parameters::from(TaskInput { task_id: first.clone() }),
            CancellationToken::new(),
        )
        .await
        .expect("context")
        .0;
    assert_eq!(
        (context.format_version, context.data.task.handoff.as_str()),
        (2, "Half done.")
    );

    let completed = server
        .task_complete(
            Parameters::from(TaskCompleteInput {
                task_id: first.clone(),
                evidence: Some("Tests pass.".to_owned()),
                open_subtasks: OpenSubtasks::Refuse,
            }),
            CancellationToken::new(),
        )
        .await
        .expect("complete")
        .0;
    assert_eq!(completed.format_version, 2);
    let data = serde_json::to_value(&completed.data).expect("completion serializes");
    assert_eq!(data["task"]["evidence"], json!("Tests pass."));
    assert_eq!(data["unblocked"], json!([second]));

    let shown = server
        .show(
            Parameters::from(RecordInput { id: second.clone() }),
            CancellationToken::new(),
        )
        .await
        .expect("show task")
        .0;
    assert_eq!(shown.format_version, 2);
    let captured = server
        .capture_create(
            Parameters::from(CaptureCreateInput { title: "Later".to_owned(), body: String::new() }),
            CancellationToken::new(),
        )
        .await
        .expect("capture")
        .0;
    let shown = server
        .show(
            Parameters::from(RecordInput { id: captured.data.capture.id.to_string() }),
            CancellationToken::new(),
        )
        .await
        .expect("show capture")
        .0;
    assert_eq!(shown.format_version, 1);
    assert_eq!(
        serde_json::to_value(&shown.data).expect("serializes")["action"],
        json!("shown")
    );
}

#[tokio::test]
async fn lists_page_with_bounded_limits() {
    let project = project(false);
    let server = ArclServer::new(project.path().to_owned());
    let mut created = Vec::new();
    for title in ["A", "B", "C", "D", "E"] {
        created.push(create_task(&server, title, &[]).await);
    }

    let mut listed = Vec::new();
    let mut cursor = None;
    loop {
        let page = server
            .list(
                Parameters::from(ListInput {
                    kind: Some(RecordKind::Task),
                    limit: Some(2),
                    cursor,
                    ..ListInput::default()
                }),
                CancellationToken::new(),
            )
            .await
            .expect("list")
            .0;
        assert!(page.data.records.len() <= 2);
        listed.extend(page.data.records.into_iter().map(|record| record.id));
        cursor = page.data.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    created.sort();
    assert_eq!(listed, created);

    let first = server
        .ready(
            Parameters::from(ReadyInput { limit: Some(3), ..ReadyInput::default() }),
            CancellationToken::new(),
        )
        .await
        .expect("ready")
        .0;
    let rest = server
        .ready(
            Parameters::from(ReadyInput { limit: Some(3), cursor: first.data.next_cursor, ..ReadyInput::default() }),
            CancellationToken::new(),
        )
        .await
        .expect("ready page two")
        .0;
    assert_eq!(
        (first.data.tasks.len(), rest.data.tasks.len(), rest.data.next_cursor),
        (3, 2, None)
    );

    for limit in [0, MAX_LIMIT + 1] {
        let error = server
            .list(
                Parameters::from(ListInput { limit: Some(limit), ..ListInput::default() }),
                CancellationToken::new(),
            )
            .await
            .err()
            .expect("limit out of range");
        assert_eq!(error.code(), "invalid_request");
    }
    let error = server
        .list(
            Parameters::from(ListInput {
                kind: Some(RecordKind::Capture),
                status: vec![RecordStatus::Pending],
                ..ListInput::default()
            }),
            CancellationToken::new(),
        )
        .await
        .err()
        .expect("pending is not a capture status");
    assert_eq!(error.code(), "invalid_request");
}

#[tokio::test]
async fn failures_use_the_cli_error_envelope() {
    let project = project(false);
    let server = ArclServer::new(project.path().to_owned());

    let missing = server
        .context(
            Parameters::from(TaskInput { task_id: "arcl-t-01M5QPZZKW6X9JPSGZ4VWRAMCY".to_owned() }),
            CancellationToken::new(),
        )
        .await
        .err()
        .expect("no such task");
    assert_eq!(
        missing.envelope(),
        json!({
            "format_version": 1,
            "error": {
                "code": "not_found",
                "message": "planning task `arcl-t-01M5QPZZKW6X9JPSGZ4VWRAMCY` was not found",
                "exit_code": 5,
            },
        })
    );

    let invalid = server
        .show(
            Parameters::from(RecordInput { id: "arcl-t-nope".to_owned() }),
            CancellationToken::new(),
        )
        .await
        .err()
        .expect("invalid ID");
    assert_eq!((invalid.code(), invalid.exit_code()), ("invalid_request", 3));

    let empty = TempDir::new().expect("temp dir");
    let uninitialized = ArclServer::new(empty.path().to_owned())
        .ready(Parameters::from(ReadyInput::default()), CancellationToken::new())
        .await
        .err()
        .expect("no project");
    assert_eq!(uninitialized.code(), "invalid_request");
    assert!(uninitialized.to_string().contains("run `arcl init` first"));
}

#[tokio::test]
async fn writes_sync_the_workspace() {
    let project = project(true);
    let server = ArclServer::new(project.path().to_owned());

    let id = create_task(&server, "Synced", &[]).await;

    let file = project.path().join(".arcl/workspace/tasks").join(format!("{id}.md"));
    let content = fs::read_to_string(&file).expect("the write is exported to the workspace");
    assert!(content.contains("title = \"Synced\""));

    let updated = server
        .task_update(
            Parameters::from(TaskUpdateInput {
                task_id: id.clone(),
                title: None,
                body: None,
                priority: Some(TaskPriority::High),
                position: None,
            }),
            CancellationToken::new(),
        )
        .await
        .expect("update")
        .0;
    assert!(updated.warnings.is_empty());
    let content = fs::read_to_string(&file).expect("the update is exported");
    assert!(content.contains("priority = \"high\""));
}

#[tokio::test]
async fn bad_arguments_fail_with_the_failure_envelope() {
    let project = project(false);
    let server = ArclServer::new(project.path().to_owned());

    let unknown = server
        .task_start(
            Parameters::from_arguments(arguments(json!({"force": true}))),
            CancellationToken::new(),
        )
        .await
        .err()
        .expect("an unknown field fails");
    assert_eq!((unknown.code(), unknown.exit_code()), ("invalid_request", 3));
    assert!(unknown.to_string().contains("unknown field `force`"), "{unknown}");

    let wrong_type = server
        .list(
            Parameters::from_arguments(arguments(json!({"limit": "5"}))),
            CancellationToken::new(),
        )
        .await
        .err()
        .expect("a string limit fails");
    assert_eq!(wrong_type.code(), "invalid_request");

    let missing = server
        .explain(Parameters::from_arguments(None), CancellationToken::new())
        .await
        .err()
        .expect("a missing field fails");
    assert_eq!(missing.envelope()["error"]["exit_code"], json!(3));
}

#[tokio::test]
async fn ready_pages_by_ready_order_key() {
    let project = project(false);
    let server = ArclServer::new(project.path().to_owned());
    let mut created = Vec::new();
    for title in ["A", "B", "C"] {
        created.push(create_task(&server, title, &[]).await);
    }
    created.sort();

    let first = server
        .ready(
            Parameters::from(ReadyInput { limit: Some(1), ..ReadyInput::default() }),
            CancellationToken::new(),
        )
        .await
        .expect("ready")
        .0;
    assert_eq!(ids(&first.data.tasks), vec![created[0].clone()]);
    server
        .task_start(
            Parameters::from(TaskInput { task_id: created[0].clone() }),
            CancellationToken::new(),
        )
        .await
        .expect("start A");

    let second = server
        .ready(
            Parameters::from(ReadyInput { limit: Some(1), cursor: first.data.next_cursor, ..ReadyInput::default() }),
            CancellationToken::new(),
        )
        .await
        .expect("ready page two")
        .0;
    assert_eq!(ids(&second.data.tasks), vec![created[1].clone()]);
}

#[tokio::test]
async fn malformed_cursors_are_rejected() {
    let project = project(false);
    let server = ArclServer::new(project.path().to_owned());
    let task = create_task(&server, "A", &[]).await;

    for cursor in [
        "nocolon".to_owned(),
        "zzz:x".to_owned(),
        "task:".to_owned(),
        "task:arcl-t-nope".to_owned(),
        format!("note:{task}"),
    ] {
        let error = server
            .list(
                Parameters::from(ListInput { cursor: Some(cursor.clone()), ..ListInput::default() }),
                CancellationToken::new(),
            )
            .await
            .err()
            .unwrap_or_else(|| panic!("list cursor `{cursor}` is rejected"));
        assert_eq!(error.code(), "invalid_request", "{cursor}");
    }
    let accepted = server
        .list(
            Parameters::from(ListInput { cursor: Some(format!("task:{task}")), ..ListInput::default() }),
            CancellationToken::new(),
        )
        .await
        .expect("a cursor `list` returns is accepted")
        .0;
    assert!(accepted.data.records.is_empty());

    for cursor in [
        "1".to_owned(),
        format!("urgent:0:{task}"),
        format!("normal:x:{task}"),
        "normal:0:arcl-t-nope".to_owned(),
    ] {
        let error = server
            .ready(
                Parameters::from(ReadyInput { cursor: Some(cursor.clone()), ..ReadyInput::default() }),
                CancellationToken::new(),
            )
            .await
            .err()
            .unwrap_or_else(|| panic!("ready cursor `{cursor}` is rejected"));
        assert_eq!(error.code(), "invalid_request", "{cursor}");
    }
}

#[tokio::test]
async fn outputs_match_their_output_schemas() {
    let project = project(false);
    let server = ArclServer::new(project.path().to_owned());
    let tools = server.tools();
    let check = |name: &str, output: Value| {
        let tool = tools.iter().find(|tool| tool.name == name).expect("tool is listed");
        let schema = Value::Object((**tool.output_schema.as_ref().expect("output schema")).clone());
        let validator = jsonschema::validator_for(&schema).expect("output schema compiles");
        let errors = validator
            .iter_errors(&output)
            .map(|error| error.to_string())
            .collect::<Vec<_>>();
        assert!(errors.is_empty(), "{name}: {errors:?}\n{output}");
    };

    let first = create_task(&server, "First", &[]).await;
    let second = create_task(&server, "Second", &[&first]).await;

    let ready = server
        .ready(
            Parameters::from(ReadyInput { limit: Some(1), ..ReadyInput::default() }),
            CancellationToken::new(),
        )
        .await
        .expect("ready")
        .0;
    check("ready", to_json(&ready));
    let list = server
        .list(Parameters::from(ListInput::default()), CancellationToken::new())
        .await
        .expect("list")
        .0;
    check("list", to_json(&list));
    let explain = server
        .explain(
            Parameters::from(TaskInput { task_id: second.clone() }),
            CancellationToken::new(),
        )
        .await
        .expect("explain")
        .0;
    check("explain", to_json(&explain));
    let started = server
        .task_start(
            Parameters::from(TaskInput { task_id: first.clone() }),
            CancellationToken::new(),
        )
        .await
        .expect("start")
        .0;
    check("task_start", to_json(&started));
    let context = server
        .context(
            Parameters::from(TaskInput { task_id: first.clone() }),
            CancellationToken::new(),
        )
        .await
        .expect("context")
        .0;
    check("context", to_json(&context));
    let completed = server
        .task_complete(
            Parameters::from(TaskCompleteInput {
                task_id: first,
                evidence: Some("Done.".to_owned()),
                open_subtasks: OpenSubtasks::Refuse,
            }),
            CancellationToken::new(),
        )
        .await
        .expect("complete")
        .0;
    check("task_complete", to_json(&completed));
    let shown = server
        .show(Parameters::from(RecordInput { id: second }), CancellationToken::new())
        .await
        .expect("show")
        .0;
    check("show", to_json(&shown));
}

#[tokio::test]
async fn a_failed_sync_warns_reads_and_refuses_writes() {
    let project = project(true);
    let server = ArclServer::new(project.path().to_owned());
    let id = create_task(&server, "Synced", &[]).await;
    fs::write(project.path().join(".arcl/workspace/manifest.toml"), "not = [valid").expect("break the manifest");

    let ready = server
        .ready(Parameters::from(ReadyInput::default()), CancellationToken::new())
        .await
        .expect("ready")
        .0;
    assert_eq!(ids(&ready.data.tasks), vec![id]);
    assert_eq!(ready.warnings.len(), 1);
    let warning = serde_json::to_value(&ready.warnings[0]).expect("warning serializes");
    assert_eq!(warning["code"], json!("workspace_sync"));
    assert!(
        warning["message"]
            .as_str()
            .is_some_and(|message| message.contains("shows the database as it is")),
        "{warning}"
    );

    let refused = server
        .capture_create(
            Parameters::from(CaptureCreateInput { title: "Refused".to_owned(), body: String::new() }),
            CancellationToken::new(),
        )
        .await
        .err()
        .expect("a write does not run without a sync");
    assert!(
        refused
            .to_string()
            .contains("the workspace must sync before this change"),
        "{refused}"
    );
    assert!(!refused.to_string().contains("workspace resolve"), "{refused}");
    let database = Database::open(project.path().join(".arcl/arcl.db")).expect("open database");
    assert!(database.captures().expect("captures").is_empty());
}

#[tokio::test]
async fn a_divergent_sync_names_workspace_resolve() {
    let project = project(true);
    let server = ArclServer::new(project.path().to_owned());
    let id = create_task(&server, "Synced", &[]).await;

    let file = project.path().join(".arcl/workspace/tasks").join(format!("{id}.md"));
    let content = fs::read_to_string(&file).expect("read task file");
    fs::write(
        &file,
        content.replace("title = \"Synced\"", "title = \"From the workspace\""),
    )
    .expect("edit the workspace");
    let mut database = Database::open(project.path().join(".arcl/arcl.db")).expect("open database");
    database
        .update_planning_task(
            TaskId::parse(&id).expect("task ID"),
            PlanningTaskUpdate { title: Some("From the database".to_owned()), ..PlanningTaskUpdate::default() },
        )
        .expect("edit the database");
    drop(database);

    let refused = server
        .task_start(Parameters::from(TaskInput { task_id: id }), CancellationToken::new())
        .await
        .err()
        .expect("a divergent sync stops the write");
    assert_eq!((refused.code(), refused.exit_code()), ("conflict", 4));
    assert!(refused.to_string().contains("arcl workspace resolve"), "{refused}");
}

#[tokio::test]
async fn a_held_sync_lock_fails_after_the_wait() {
    let project = project(true);
    let server = ArclServer::new(project.path().to_owned()).with_lock_wait(Duration::from_millis(200));
    let lock = hold_sync_lock(project.path());

    let refused = server
        .capture_create(
            Parameters::from(CaptureCreateInput { title: "Waited".to_owned(), body: String::new() }),
            CancellationToken::new(),
        )
        .await
        .err()
        .expect("the write gives up on the lock");
    assert_eq!((refused.code(), refused.exit_code()), ("application_error", 1));
    assert!(refused.to_string().contains("arcl.db-sync.lock"), "{refused}");

    let read = server
        .ready(Parameters::from(ReadyInput::default()), CancellationToken::new())
        .await
        .err()
        .expect("the read gives up on the lock");
    assert_eq!(read.code(), "application_error");

    drop(lock);
    let database = Database::open(project.path().join(".arcl/arcl.db")).expect("open database");
    assert!(database.captures().expect("captures").is_empty());
}

#[tokio::test]
async fn a_cancelled_write_does_not_run() {
    let project = project(true);
    let server = ArclServer::new(project.path().to_owned());
    let lock = hold_sync_lock(project.path());

    // The client cancels the request while it waits for the lock.
    let cancelled = CancellationToken::new();
    let call = {
        let server = server.clone();
        let cancelled = cancelled.clone();
        tokio::spawn(async move {
            server
                .capture_create(
                    Parameters::from(CaptureCreateInput { title: "Cancelled".to_owned(), body: String::new() }),
                    cancelled,
                )
                .await
        })
    };
    // The client drops the call, as a client-side timeout does.
    let dropped = {
        let server = server.clone();
        tokio::spawn(async move {
            server
                .capture_create(
                    Parameters::from(CaptureCreateInput { title: "Dropped".to_owned(), body: String::new() }),
                    CancellationToken::new(),
                )
                .await
        })
    };
    // Both calls are now waiting for the lock.
    tokio::time::sleep(Duration::from_millis(100)).await;
    cancelled.cancel();
    let error = call
        .await
        .expect("the call ends")
        .err()
        .expect("a cancelled call fails");
    assert!(error.to_string().contains("cancelled"), "{error}");
    dropped.abort();
    assert!(dropped.await.err().is_some_and(|error| error.is_cancelled()));

    drop(lock);
    tokio::time::sleep(Duration::from_millis(200)).await;
    let database = Database::open(project.path().join(".arcl/arcl.db")).expect("open database");
    assert!(database.captures().expect("captures").is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn an_invalid_workspace_path_is_not_a_sync_refusal() {
    let project = project(true);
    let outside = TempDir::new().expect("temp dir");
    let workspace = project.path().join(".arcl/workspace");
    fs::remove_dir_all(&workspace).expect("remove workspace");
    std::os::unix::fs::symlink(outside.path(), &workspace).expect("symlink the workspace");
    let server = ArclServer::new(project.path().to_owned());

    let refused = server
        .capture_create(
            Parameters::from(CaptureCreateInput { title: "Refused".to_owned(), body: String::new() }),
            CancellationToken::new(),
        )
        .await
        .err()
        .expect("an invalid workspace path stops the write");
    assert_eq!((refused.code(), refused.exit_code()), ("invalid_request", 3));
    assert!(
        refused
            .to_string()
            .starts_with("invalid project or record: project configuration"),
        "{refused}"
    );
    assert!(!refused.to_string().contains("must sync"), "{refused}");
}

#[test]
fn stdio_serves_tool_calls() {
    let project = project(false);
    let mut child = Command::new(env!("CARGO_BIN_EXE_arcl-mcp"))
        .current_dir(project.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the server starts");
    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut request = |message: Value| writeln!(stdin, "{message}").expect("write request");

    request(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}}}));
    assert_eq!(
        response(&mut stdout, 1)["result"]["serverInfo"]["name"],
        json!("arcl-mcp")
    );
    request(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    request(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
        "name": "capture_create", "arguments": {"title": "From stdio"}}}));
    let created = response(&mut stdout, 2);
    assert_eq!(created["result"]["structuredContent"]["format_version"], json!(1));
    assert_eq!(
        created["result"]["structuredContent"]["data"]["capture"]["title"],
        json!("From stdio")
    );

    request(json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {
        "name": "task_start", "arguments": {"task_id": "arcl-t-01M5QPZZKW6X9JPSGZ4VWRAMCY"}}}));
    let failed = response(&mut stdout, 3);
    assert_eq!(failed["result"]["isError"], json!(true));
    assert_eq!(
        failed["result"]["structuredContent"]["error"]["code"],
        json!("not_found")
    );

    request(json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {
        "name": "task_start", "arguments": {"task_id": "arcl-t-01M5QPZZKW6X9JPSGZ4VWRAMCY", "force": true}}}));
    let rejected = response(&mut stdout, 4);
    assert_eq!(rejected["result"]["isError"], json!(true));
    assert_eq!(
        rejected["result"]["structuredContent"]["error"]["code"],
        json!("invalid_request")
    );

    drop(stdin);
    assert!(child.wait().expect("the server exits").success());
}

/// A project in a temp directory, with the workspace on or off.
fn project(workspace: bool) -> TempDir {
    let directory = TempDir::new().expect("temp dir");
    let arcl = directory.path().join(".arcl");
    fs::create_dir_all(&arcl).expect("create .arcl");
    let mut config = ProjectConfig::default();
    config.workspace.enabled = workspace;
    fs::write(arcl.join("config.toml"), config.render().expect("render config")).expect("write config");
    Database::open(arcl.join("arcl.db")).expect("create database");
    if workspace {
        let root = arcl.join("workspace");
        for kind in ["captures", "releases", "specs", "plans", "phases", "tasks", "notes"] {
            fs::create_dir_all(root.join(kind)).expect("create workspace directory");
        }
        let manifest = encode_manifest(&WorkspaceManifest::default()).expect("encode manifest");
        fs::write(root.join("manifest.toml"), manifest).expect("write manifest");
    }
    directory
}

/// Hold the project's sync lock, as a running `arcl` command does, until the
/// file drops.
fn hold_sync_lock(root: &std::path::Path) -> fs::File {
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join(".arcl/arcl.db-sync.lock"))
        .expect("open the sync lock");
    lock.lock().expect("hold the sync lock");
    lock
}

async fn create_task(server: &ArclServer, title: &str, blocked_by: &[&str]) -> String {
    let created = server
        .task_create(
            Parameters::from(TaskCreateInput {
                title: title.to_owned(),
                body: String::new(),
                spec: None,
                plan: None,
                phase: None,
                parent: None,
                priority: TaskPriority::Normal,
                position: 0,
                blocked_by: blocked_by.iter().map(|id| (*id).to_owned()).collect(),
            }),
            CancellationToken::new(),
        )
        .await
        .unwrap_or_else(|error: ToolError| panic!("create {title}: {error}"))
        .0;
    assert_eq!((created.format_version, created.data.action.as_str()), (1, "created"));
    created.data.task.id.to_string()
}

fn to_json(value: &impl serde::Serialize) -> Value {
    serde_json::to_value(value).expect("serializes")
}

fn arguments(value: Value) -> Option<rmcp::model::JsonObject> {
    value.as_object().cloned()
}

fn ids(tasks: &[arcl_core::domain::PlanningTask]) -> Vec<String> {
    tasks.iter().map(|task| task.id.to_string()).collect()
}

/// Read lines until the response to request `id`.
fn response(stdout: &mut impl BufRead, id: u64) -> Value {
    loop {
        let mut line = String::new();
        assert_ne!(
            stdout.read_line(&mut line).expect("read response"),
            0,
            "the server closed stdout"
        );
        let message: Value = serde_json::from_str(&line).expect("a JSON-RPC message");
        if message["id"] == json!(id) {
            return message;
        }
    }
}
