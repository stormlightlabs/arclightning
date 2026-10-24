//! The `arcl-mcp` binary over stdio, as an MCP client drives it: one capture
//! taken to completion with tools alone, the limits on what a tool may do,
//! and the errors invalid transitions return.

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};
use tempfile::TempDir;

use arcl_cli::cli::ColorChoice;
use arcl_cli::output::{OutputMode, Renderer};
use arcl_core::domain::{CaptureId, TaskAction, TaskId};
use arcl_mcp::error::ToolError;
use arcl_repo::workspace::ProjectConfig;
use arcl_store::{CaptureTaskPromotion, Database, StorageError};

/// The tools that change no records. Every other tool writes.
const READ_TOOLS: &[&str] = &["context", "explain", "list", "ready", "show"];

/// An MCP client talking to an `arcl-mcp` child process over stdio.
struct Client {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    output_schemas: BTreeMap<String, Value>,
}

impl Client {
    /// Start the server in `directory`, initialize the session and read the
    /// output schema of every tool.
    fn start(directory: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_arcl-mcp"))
            .current_dir(directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the server starts");
        let stdin = child.stdin.take();
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        let mut client = Self { child, stdin, stdout, next_id: 0, output_schemas: BTreeMap::new() };
        let initialized = client.request(
            "initialize",
            json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}}),
        );
        assert_eq!(initialized["result"]["serverInfo"]["name"], json!("arcl-mcp"));
        client.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        for tool in client.tools() {
            let name = tool["name"].as_str().expect("tool name").to_owned();
            client.output_schemas.insert(name, tool["outputSchema"].clone());
        }
        client
    }

    /// The listed tools.
    fn tools(&mut self) -> Vec<Value> {
        let listed = self.request("tools/list", json!({}));
        listed["result"]["tools"].as_array().expect("a tool list").clone()
    }

    /// Call `name` and return its structured content, failing on an error
    /// result or on content its output schema does not allow.
    fn ok(&mut self, name: &str, arguments: Value) -> Value {
        let result = self.call(name, arguments);
        assert_ne!(result["isError"], json!(true), "{name} failed: {result}");
        let content = result["structuredContent"].clone();
        let schema = &self.output_schemas[name];
        let validator = jsonschema::validator_for(schema).expect("output schema compiles");
        let errors = validator
            .iter_errors(&content)
            .map(|error| error.to_string())
            .collect::<Vec<_>>();
        assert!(errors.is_empty(), "{name}: {errors:?}\n{content}");
        content
    }

    /// Call `name`, expect an error result and return its failure envelope.
    fn fail(&mut self, name: &str, arguments: Value) -> Value {
        let result = self.call(name, arguments);
        assert_eq!(result["isError"], json!(true), "{name} succeeded: {result}");
        result["structuredContent"].clone()
    }

    /// Call `name` and return the JSON-RPC result.
    fn call(&mut self, name: &str, arguments: Value) -> Value {
        let response = self.request("tools/call", json!({"name": name, "arguments": arguments}));
        response["result"].clone()
    }

    /// Send a request and return the response with the same ID.
    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let mut line = String::new();
            let read = self.stdout.read_line(&mut line).expect("read response");
            assert_ne!(read, 0, "the server closed stdout");
            let message: Value = serde_json::from_str(&line).expect("a JSON-RPC message");
            if message["id"] == json!(id) {
                return message;
            }
        }
    }

    fn send(&mut self, message: Value) {
        let stdin = self.stdin.as_mut().expect("stdin is open");
        writeln!(stdin, "{message}").expect("write request");
    }

    /// Close standard input and check that the server exits cleanly.
    fn finish(mut self) {
        drop(self.stdin.take());
        assert!(self.child.wait().expect("the server exits").success());
    }
}

#[test]
fn a_capture_goes_to_completion_through_mcp_tools() {
    let project = project();
    let mut client = Client::start(project.path());

    let captured = client.ok(
        "capture_create",
        json!({"title": "Retry the flaky lock test", "body": "It fails one run in twenty."}),
    );
    let capture = captured["data"]["capture"]["id"]
        .as_str()
        .expect("capture ID")
        .to_owned();

    let promoted = client.ok("capture_promote", json!({"capture_id": capture, "target": "task"}));
    assert_eq!(promoted["format_version"], json!(1));
    let task = promoted["data"]["task"]["id"].as_str().expect("task ID").to_owned();
    assert_eq!(promoted["data"]["capture"]["status"], json!("promoted"));
    assert_eq!(
        promoted["data"]["promotion"]["target"],
        json!({"kind": "task", "id": task})
    );
    assert_eq!(
        (&promoted["data"]["task"]["title"], &promoted["data"]["task"]["body"]),
        (
            &json!("Retry the flaky lock test"),
            &json!("It fails one run in twenty.")
        )
    );

    assert_eq!(ready_ids(&mut client), vec![task.clone()]);

    let started = client.ok("task_start", json!({"task_id": task}));
    assert_eq!(
        (&started["data"]["action"], &started["data"]["task"]["status"]),
        (&json!("started"), &json!("in_progress"))
    );
    let context = client.ok("context", json!({"task_id": task}));
    assert_eq!(
        (&context["format_version"], &context["data"]["task"]["id"]),
        (&json!(2), &json!(task))
    );

    let handed_off = client.ok(
        "task_handoff",
        json!({"task_id": task, "note": "The retry loop is in; the timeout is next."}),
    );
    assert_eq!(handed_off["data"]["task"]["status"], json!("parked"));
    assert!(ready_ids(&mut client).is_empty(), "a parked task is not ready");

    let unparked = client.ok("task_unpark", json!({"task_id": task}));
    assert_eq!(unparked["data"]["task"]["status"], json!("pending"));
    assert_eq!(ready_ids(&mut client), vec![task.clone()]);
    client.ok("task_start", json!({"task_id": task}));
    let resumed = client.ok("context", json!({"task_id": task}));
    assert_eq!(
        resumed["data"]["task"]["handoff"],
        json!("The retry loop is in; the timeout is next.")
    );

    let completed = client.ok(
        "task_complete",
        json!({"task_id": task, "evidence": "The test passed 200 runs in a row."}),
    );
    assert_eq!(completed["format_version"], json!(2));
    assert_eq!(
        (
            &completed["data"]["task"]["status"],
            &completed["data"]["task"]["evidence"]
        ),
        (&json!("completed"), &json!("The test passed 200 runs in a row."))
    );

    assert!(ready_ids(&mut client).is_empty());
    let shown = client.ok("show", json!({"id": capture}));
    assert_eq!(shown["data"]["capture"]["status"], json!("promoted"));
    client.finish();
}

#[test]
fn tools_stay_inside_their_authority() {
    let project = project();
    let mut client = Client::start(project.path());

    for tool in client.tools() {
        let name = tool["name"].as_str().expect("tool name");
        let read_only = READ_TOOLS.contains(&name);
        assert_eq!(
            tool["annotations"]["readOnlyHint"],
            json!(read_only),
            "{name}'s read-only hint"
        );
    }

    let blocker = create_task(&mut client, json!({"title": "Blocker"}));
    let blocked = create_task(&mut client, json!({"title": "Blocked", "blocked_by": [blocker]}));

    // No argument lets a tool start a task that is not ready.
    let forced = client.fail("task_start", json!({"task_id": blocked, "force": true}));
    assert_eq!(code(&forced), ("invalid_request", 3));
    assert!(message(&forced).contains("unknown field `force`"), "{forced}");
    let refused = client.fail("task_start", json!({"task_id": blocked}));
    assert_eq!(code(&refused), ("invalid_request", 3));
    let explained = client.ok("explain", json!({"task_id": blocked}));
    assert_eq!(explained["data"]["task"]["status"], json!("pending"));

    // Completion leaves open subtasks alone unless the call says otherwise.
    let parent = create_task(&mut client, json!({"title": "Parent"}));
    let child = create_task(&mut client, json!({"title": "Child", "parent": parent}));
    let open = client.fail("task_complete", json!({"task_id": parent}));
    assert_eq!(code(&open), ("invalid_request", 3));
    let child_view = client.ok("show", json!({"id": child}));
    assert_eq!(child_view["data"]["task"]["status"], json!("pending"));

    // Task placement applies only to a task.
    let captured = client.ok("capture_create", json!({"title": "A note to keep"}));
    let capture = captured["data"]["capture"]["id"].clone();
    let misplaced = client.fail(
        "capture_promote",
        json!({"capture_id": capture, "target": "note", "parent": parent}),
    );
    assert_eq!(code(&misplaced), ("invalid_request", 3));
    assert!(
        message(&misplaced).contains("only when promoting to a task"),
        "{misplaced}"
    );

    // A tool the server does not list cannot be called.
    let unknown = client.request("tools/call", json!({"name": "task_delete", "arguments": {}}));
    assert!(unknown["result"].is_null(), "{unknown}");
    assert!(unknown["error"]["code"].is_i64(), "{unknown}");
    client.finish();
}

#[test]
fn invalid_transitions_carry_the_application_error() {
    let project = project();
    let mut client = Client::start(project.path());
    let database = || Database::open(project.path().join(".arcl/arcl.db")).expect("open database");

    let pending = create_task(&mut client, json!({"title": "Pending"}));
    let done = create_task(&mut client, json!({"title": "Done"}));
    client.ok("task_start", json!({"task_id": done}));
    client.ok("task_complete", json!({"task_id": done, "evidence": "Shipped."}));
    let pending_id = TaskId::parse(&pending).expect("task ID");
    let done_id = TaskId::parse(&done).expect("task ID");

    let failed = client.fail("task_unpark", json!({"task_id": pending}));
    let expected = database().transition_planning_task(pending_id, TaskAction::Unpark, false);
    assert_same_failure(&failed, expected.map(drop), ("invalid_request", 3));

    let failed = client.fail("task_handoff", json!({"task_id": pending, "note": "Not started."}));
    let expected = database().handoff_planning_task(pending_id, "Not started.".to_owned());
    assert_same_failure(&failed, expected.map(drop), ("invalid_request", 3));

    let failed = client.fail("task_start", json!({"task_id": done}));
    let expected = database().start_planning_task(done_id, false);
    assert_same_failure(&failed, expected.map(drop), ("invalid_request", 3));

    let failed = client.fail("task_unpark", json!({"task_id": done}));
    let expected = database().transition_planning_task(done_id, TaskAction::Unpark, false);
    assert_same_failure(&failed, expected.map(drop), ("invalid_request", 3));

    let failed = client.fail("task_handoff", json!({"task_id": done, "note": "Reopen it."}));
    let expected = database().handoff_planning_task(done_id, "Reopen it.".to_owned());
    assert_same_failure(&failed, expected.map(drop), ("invalid_request", 3));

    let captured = client.ok("capture_create", json!({"title": "Promote once"}));
    let capture = captured["data"]["capture"]["id"]
        .as_str()
        .expect("capture ID")
        .to_owned();
    let capture_id = CaptureId::parse(&capture).expect("capture ID");
    let first = client.ok("capture_promote", json!({"capture_id": capture, "target": "task"}));
    let again = client.ok("capture_promote", json!({"capture_id": capture, "target": "task"}));
    assert_eq!(again["data"]["task"]["id"], first["data"]["task"]["id"]);
    let failed = client.fail("capture_promote", json!({"capture_id": capture, "target": "note"}));
    let expected = database().promote_capture_to_note(capture_id, "Promote once".to_owned(), String::new());
    assert_same_failure(&failed, expected.map(drop), ("invalid_request", 3));

    let missing = "arcl-c-01M5QPZZKW6X9JPSGZ4VWRAMCY".to_owned();
    let failed = client.fail("capture_promote", json!({"capture_id": missing, "target": "task"}));
    let expected = Err(StorageError::CaptureNotFound { id: missing });
    assert_same_failure(&failed, expected, ("not_found", 5));
    client.finish();
}

#[test]
fn capture_promote_checks_run_in_the_cli_order() {
    let project = project();
    let mut client = Client::start(project.path());
    let database = || Database::open(project.path().join(".arcl/arcl.db")).expect("open database");
    let parent = create_task(&mut client, json!({"title": "Parent"}));

    // A missing capture is reported before misplaced task options.
    let missing = "arcl-c-01M5QPZZKW6X9JPSGZ4VWRAMCY".to_owned();
    let failed = client.fail(
        "capture_promote",
        json!({"capture_id": missing, "target": "note", "parent": parent}),
    );
    let expected = Err(StorageError::CaptureNotFound { id: missing });
    assert_same_failure(&failed, expected, ("not_found", 5));

    // A blank title is the store's to refuse, after the capture is found.
    let captured = client.ok("capture_create", json!({"title": "Keep the title"}));
    let capture = captured["data"]["capture"]["id"]
        .as_str()
        .expect("capture ID")
        .to_owned();
    let capture_id = CaptureId::parse(&capture).expect("capture ID");
    let failed = client.fail(
        "capture_promote",
        json!({"capture_id": capture, "target": "note", "title": " "}),
    );
    let expected = database().promote_capture_to_note(capture_id, " ".to_owned(), String::new());
    assert_same_failure(&failed, expected.map(drop), ("invalid_request", 3));
    client.finish();
}

#[test]
fn capture_promote_data_matches_the_cli() {
    let project = project();
    let mut client = Client::start(project.path());
    let database = || Database::open(project.path().join(".arcl/arcl.db")).expect("open database");
    let renderer = Renderer::new(OutputMode::Json, ColorChoice::Never);

    for target in ["spec", "task", "note"] {
        let captured = client.ok(
            "capture_create",
            json!({"title": format!("Promote to a {target}"), "body": "Body text."}),
        );
        let capture = captured["data"]["capture"]["id"]
            .as_str()
            .expect("capture ID")
            .to_owned();
        let promoted = client.ok("capture_promote", json!({"capture_id": capture, "target": target}));

        // Promoting again to the same target returns the same records, so the
        // CLI's rendering of the store's result must match the tool's output.
        let id = CaptureId::parse(&capture).expect("capture ID");
        let title = format!("Promote to a {target}");
        let body = "Body text.".to_owned();
        let result = match target {
            "spec" => database().promote_capture_to_spec(id, title, body, String::new()),
            "task" => database().promote_capture_to_task(
                id,
                CaptureTaskPromotion {
                    spec_id: None,
                    plan_id: None,
                    phase_id: None,
                    parent_id: None,
                    title,
                    body,
                    priority: Default::default(),
                    position: 0,
                },
            ),
            _ => database().promote_capture_to_note(id, title, body),
        }
        .expect("the store returns the promotion");
        let rendered = renderer
            .render_capture_promotion(&result)
            .expect("render")
            .expect("JSON output");
        let cli: Value = serde_json::from_str(&rendered).expect("CLI JSON");
        assert_eq!(promoted["data"], cli["data"], "{target}");
        assert_eq!(promoted["format_version"], cli["format_version"], "{target}");
    }
    client.finish();
}

/// A project without the workspace in a temp directory.
fn project() -> TempDir {
    let directory = TempDir::new().expect("temp dir");
    let arcl = directory.path().join(".arcl");
    fs::create_dir_all(&arcl).expect("create .arcl");
    let mut config = ProjectConfig::default();
    config.workspace.enabled = false;
    fs::write(arcl.join("config.toml"), config.render().expect("render config")).expect("write config");
    Database::open(arcl.join("arcl.db")).expect("create database");
    directory
}

fn create_task(client: &mut Client, arguments: Value) -> String {
    let created = client.ok("task_create", arguments);
    created["data"]["task"]["id"].as_str().expect("task ID").to_owned()
}

fn ready_ids(client: &mut Client) -> Vec<String> {
    let ready = client.ok("ready", json!({}));
    ready["data"]["tasks"]
        .as_array()
        .expect("ready tasks")
        .iter()
        .map(|task| task["id"].as_str().expect("task ID").to_owned())
        .collect()
}

fn code(envelope: &Value) -> (&str, u64) {
    (
        envelope["error"]["code"].as_str().expect("error code"),
        envelope["error"]["exit_code"].as_u64().expect("exit code"),
    )
}

fn message(envelope: &Value) -> &str {
    envelope["error"]["message"].as_str().expect("error message")
}

/// Check that a tool's failure envelope is the one the application error
/// from the same call on the store maps to, with the code and exit code the
/// CLI uses.
fn assert_same_failure(envelope: &Value, expected: Result<(), StorageError>, code_and_exit: (&str, u64)) {
    let error = expected.expect_err("the store refuses the same call");
    assert_eq!(envelope, &ToolError::from(error).envelope());
    assert_eq!(code(envelope), code_and_exit, "{envelope}");
}
