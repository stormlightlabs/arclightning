//! The MCP tools against real projects: the tool list and its schemas, the
//! agent loop, pagination, failures, the workspace sync and stdio.

use std::collections::BTreeSet;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use rmcp::handler::server::wrapper::Parameters;
use serde_json::{Value, json};
use tempfile::TempDir;

use arcl_core::domain::{TaskPriority, TaskStatus};
use arcl_mcp::{error::ToolError, server::ArclServer, tools::*};
use arcl_repo::workspace::{ProjectConfig, WorkspaceManifest, encode_manifest};
use arcl_store::Database;

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

    let ready = server.ready(Parameters(ReadyInput::default())).await.expect("ready").0;
    assert_eq!(ready.format_version, 1);
    assert_eq!(ids(&ready.data.tasks), vec![first.clone()]);

    let refused = server
        .task_start(Parameters(TaskInput { task_id: second.clone() }))
        .await
        .err()
        .expect("a blocked task does not start");
    assert_eq!((refused.code(), refused.exit_code()), ("invalid_request", 3));

    let explained = server
        .explain(Parameters(TaskInput { task_id: second.clone() }))
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
        .task_start(Parameters(TaskInput { task_id: first.clone() }))
        .await
        .expect("start")
        .0;
    assert_eq!(
        (started.data.action.as_str(), started.data.task.status),
        ("started", TaskStatus::InProgress)
    );

    let handed_off = server
        .task_handoff(Parameters(TaskHandoffInput {
            task_id: first.clone(),
            note: "Half done.".to_owned(),
        }))
        .await
        .expect("handoff")
        .0;
    assert_eq!(handed_off.data.task.status, TaskStatus::Parked);

    server
        .task_unpark(Parameters(TaskInput { task_id: first.clone() }))
        .await
        .expect("unpark");
    server
        .task_start(Parameters(TaskInput { task_id: first.clone() }))
        .await
        .expect("start again");
    let context = server
        .context(Parameters(TaskInput { task_id: first.clone() }))
        .await
        .expect("context")
        .0;
    assert_eq!(
        (context.format_version, context.data.task.handoff.as_str()),
        (2, "Half done.")
    );

    let completed = server
        .task_complete(Parameters(TaskCompleteInput {
            task_id: first.clone(),
            evidence: Some("Tests pass.".to_owned()),
            open_subtasks: OpenSubtasks::Refuse,
        }))
        .await
        .expect("complete")
        .0;
    assert_eq!(completed.format_version, 2);
    let data = serde_json::to_value(&completed.data).expect("completion serializes");
    assert_eq!(data["task"]["evidence"], json!("Tests pass."));
    assert_eq!(data["unblocked"], json!([second]));

    let shown = server
        .show(Parameters(RecordInput { id: second.clone() }))
        .await
        .expect("show task")
        .0;
    assert_eq!(shown.format_version, 2);
    let captured = server
        .capture_create(Parameters(CaptureCreateInput {
            title: "Later".to_owned(),
            body: String::new(),
        }))
        .await
        .expect("capture")
        .0;
    let shown = server
        .show(Parameters(RecordInput { id: captured.data.capture.id.to_string() }))
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
            .list(Parameters(ListInput {
                kind: Some(RecordKind::Task),
                limit: Some(2),
                cursor,
                ..ListInput::default()
            }))
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
        .ready(Parameters(ReadyInput { limit: Some(3), ..ReadyInput::default() }))
        .await
        .expect("ready")
        .0;
    let rest = server
        .ready(Parameters(ReadyInput {
            limit: Some(3),
            cursor: first.data.next_cursor,
            ..ReadyInput::default()
        }))
        .await
        .expect("ready page two")
        .0;
    assert_eq!(
        (first.data.tasks.len(), rest.data.tasks.len(), rest.data.next_cursor),
        (3, 2, None)
    );

    for limit in [0, MAX_LIMIT + 1] {
        let error = server
            .list(Parameters(ListInput { limit: Some(limit), ..ListInput::default() }))
            .await
            .err()
            .expect("limit out of range");
        assert_eq!(error.code(), "invalid_request");
    }
    let error = server
        .list(Parameters(ListInput {
            kind: Some(RecordKind::Capture),
            status: vec![RecordStatus::Pending],
            ..ListInput::default()
        }))
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
        .context(Parameters(TaskInput {
            task_id: "arcl-t-01M5QPZZKW6X9JPSGZ4VWRAMCY".to_owned(),
        }))
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
        .show(Parameters(RecordInput { id: "arcl-t-nope".to_owned() }))
        .await
        .err()
        .expect("invalid ID");
    assert_eq!((invalid.code(), invalid.exit_code()), ("invalid_request", 3));

    let empty = TempDir::new().expect("temp dir");
    let uninitialized = ArclServer::new(empty.path().to_owned())
        .ready(Parameters(ReadyInput::default()))
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
        .task_update(Parameters(TaskUpdateInput {
            task_id: id.clone(),
            title: None,
            body: None,
            priority: Some(TaskPriority::High),
            position: None,
        }))
        .await
        .expect("update")
        .0;
    assert!(updated.warnings.is_empty());
    let content = fs::read_to_string(&file).expect("the update is exported");
    assert!(content.contains("priority = \"high\""));
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

async fn create_task(server: &ArclServer, title: &str, blocked_by: &[&str]) -> String {
    let created = server
        .task_create(Parameters(TaskCreateInput {
            title: title.to_owned(),
            body: String::new(),
            spec: None,
            plan: None,
            phase: None,
            parent: None,
            priority: TaskPriority::Normal,
            position: 0,
            blocked_by: blocked_by.iter().map(|id| (*id).to_owned()).collect(),
        }))
        .await
        .unwrap_or_else(|error: ToolError| panic!("create {title}: {error}"))
        .0;
    assert_eq!((created.format_version, created.data.action.as_str()), (1, "created"));
    created.data.task.id.to_string()
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
