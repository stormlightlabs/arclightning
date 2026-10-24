//! The `arcl-mcp` binary over stdio, as an MCP client drives it: one capture
//! taken to completion with tools alone, the limits on what a tool may do,
//! and the errors invalid transitions return.

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tempfile::TempDir;

use arcl_cli::cli::ColorChoice;
use arcl_cli::output::{OutputMode, Renderer};
use arcl_core::domain::{CaptureId, PhaseId, PlanId, SpecId, TaskAction, TaskId, TaskPriority};
use arcl_mcp::error::ToolError;
use arcl_repo::workspace::{ProjectConfig, WorkspaceManifest, encode_manifest};
use arcl_store::{CapturePromotionResult, CaptureTaskPromotion, Database, StorageError};

/// The tools that change no records. Every other tool writes.
const READ_TOOLS: &[&str] = &["context", "explain", "list", "ready", "show"];

/// How long the server has to answer a request or to exit before the test
/// fails instead of hanging.
const TIMEOUT: Duration = Duration::from_secs(30);

/// An MCP client talking to an `arcl-mcp` child process over stdio.
struct Client {
    child: Child,
    stdin: Option<ChildStdin>,
    /// Lines from the server's stdout, read on their own thread so a read
    /// can time out.
    lines: Receiver<String>,
    /// Everything the server has written to stderr, for failure messages.
    stderr: Arc<Mutex<String>>,
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
            .stderr(Stdio::piped())
            .spawn()
            .expect("the server starts");
        let stdin = child.stdin.take();
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        let (sender, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in stdout.lines() {
                let Ok(line) = line else { break };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let stderr = Arc::new(Mutex::new(String::new()));
        let mut stderr_pipe = child.stderr.take().expect("stderr");
        let collected = Arc::clone(&stderr);
        thread::spawn(move || {
            let mut buffer = [0; 4096];
            while let Ok(read) = stderr_pipe.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                let text = String::from_utf8_lossy(&buffer[..read]);
                collected.lock().expect("stderr buffer").push_str(&text);
            }
        });
        let mut client = Self { child, stdin, lines, stderr, next_id: 0, output_schemas: BTreeMap::new() };
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

    /// What the server has written to stderr so far.
    fn stderr(&self) -> String {
        self.stderr.lock().expect("stderr buffer").clone()
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
        assert_ne!(
            result["isError"],
            json!(true),
            "{name} failed: {result}\nstderr:\n{}",
            self.stderr()
        );
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
        assert_eq!(
            result["isError"],
            json!(true),
            "{name} succeeded: {result}\nstderr:\n{}",
            self.stderr()
        );
        result["structuredContent"].clone()
    }

    /// Call `name` and return the JSON-RPC result.
    fn call(&mut self, name: &str, arguments: Value) -> Value {
        let response = self.request("tools/call", json!({"name": name, "arguments": arguments}));
        response["result"].clone()
    }

    /// Send a request and return the response with the same ID. Fails, with
    /// the server's stderr, when no response comes within [`TIMEOUT`].
    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let line = match self.lines.recv_timeout(left) {
                Ok(line) => line,
                Err(RecvTimeoutError::Timeout) => {
                    panic!("no response to {method} within {TIMEOUT:?}\nstderr:\n{}", self.stderr())
                }
                Err(RecvTimeoutError::Disconnected) => {
                    panic!("the server closed stdout during {method}\nstderr:\n{}", self.stderr())
                }
            };
            let message: Value = serde_json::from_str(&line)
                .unwrap_or_else(|error| panic!("not a JSON-RPC message ({error}): {line}\nstderr:\n{}", self.stderr()));
            if message["id"] == json!(id) {
                return message;
            }
        }
    }

    fn send(&mut self, message: Value) {
        let stdin = self.stdin.as_mut().expect("stdin is open");
        writeln!(stdin, "{message}").expect("write request");
    }

    /// Close standard input and check that the server exits cleanly within
    /// [`TIMEOUT`].
    fn finish(mut self) {
        drop(self.stdin.take());
        let deadline = Instant::now() + TIMEOUT;
        let status = loop {
            if let Some(status) = self.child.try_wait().expect("poll the server") {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "the server did not exit within {TIMEOUT:?}\nstderr:\n{}",
                self.stderr()
            );
            thread::sleep(Duration::from_millis(20));
        };
        assert!(
            status.success(),
            "the server exited with {status}\nstderr:\n{}",
            self.stderr()
        );
    }
}

impl Drop for Client {
    /// Stop a server a failed test left running.
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn a_capture_goes_to_completion_through_mcp_tools() {
    let project = project(false);
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
    let project = project(false);
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
    let project = project(false);
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
    let project = project(false);
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
    let project = project(false);
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

#[test]
fn capture_promote_parses_target_and_priority_where_the_cli_does() {
    let project = project(false);
    let mut client = Client::start(project.path());
    let missing = "arcl-c-01M5QPZZKW6X9JPSGZ4VWRAMCY".to_owned();
    let capture = |client: &mut Client, title: &str| {
        let captured = client.ok("capture_create", json!({"title": title}));
        captured["data"]["capture"]["id"]
            .as_str()
            .expect("capture ID")
            .to_owned()
    };

    // A missing capture is reported before a bad priority or target.
    for arguments in [
        json!({"capture_id": missing, "target": "task", "priority": "urgent"}),
        json!({"capture_id": missing, "target": "plan"}),
    ] {
        let failed = client.fail("capture_promote", arguments);
        let expected = Err(StorageError::CaptureNotFound { id: missing.clone() });
        assert_same_failure(&failed, expected, ("not_found", 5));
    }

    // A bad capture ID is reported before anything else.
    let bad_id = ToolError::from(CaptureId::parse("bogus").expect_err("not a capture ID")).envelope();
    for arguments in [
        json!({"capture_id": "bogus", "target": "task", "priority": "urgent"}),
        json!({"capture_id": "bogus", "target": "plan"}),
        json!({"capture_id": "bogus"}),
    ] {
        assert_eq!(client.fail("capture_promote", arguments), bad_id);
    }

    // Then a target must be given.
    let untargeted = client.fail("capture_promote", json!({"capture_id": missing}));
    let expected = ToolError::invalid_request("capture promotion requires a target: spec, task, or note");
    assert_eq!(untargeted, expected.envelope());
    assert_eq!(code(&untargeted), ("invalid_request", 3));

    // The priority applies only to a task, so a note ignores it.
    let note = capture(&mut client, "A note");
    let promoted = client.ok(
        "capture_promote",
        json!({"capture_id": note, "target": "note", "priority": "urgent"}),
    );
    assert_eq!(promoted["data"]["promotion"]["target"]["kind"], json!("note"));

    // The target ignores ASCII case.
    let task = capture(&mut client, "A task");
    let promoted = client.ok("capture_promote", json!({"capture_id": task, "target": "Task"}));
    assert_eq!(promoted["data"]["promotion"]["target"]["kind"], json!("task"));

    // An unknown target, then a task's priority, fail once the capture is found.
    let pending = capture(&mut client, "Not promoted");
    let unknown = client.fail("capture_promote", json!({"capture_id": pending, "target": "Plan"}));
    let expected = ToolError::invalid_request("unknown capture promotion target `plan`; use spec, task, or note");
    assert_eq!(unknown, expected.envelope());
    assert_eq!(code(&unknown), ("invalid_request", 3));
    for priority in ["urgent", "High"] {
        let failed = client.fail(
            "capture_promote",
            json!({"capture_id": pending, "target": "task", "priority": priority}),
        );
        let expected = ToolError::from(TaskPriority::parse(priority).expect_err("not a priority")).envelope();
        assert_eq!(failed, expected);
        assert_eq!(code(&failed), ("invalid_request", 3));
    }
    let shown = client.ok("show", json!({"id": pending}));
    assert_eq!(shown["data"]["capture"]["status"], json!("captured"));
    client.finish();
}

#[test]
fn a_promote_with_every_field_matches_the_cli() {
    let project = project(false);
    let mut client = Client::start(project.path());
    let database = || Database::open(project.path().join(".arcl/arcl.db")).expect("open database");
    let renderer = Renderer::new(OutputMode::Json, ColorChoice::Never);
    let render = |result: &CapturePromotionResult| -> Value {
        let rendered = renderer
            .render_capture_promotion(result)
            .expect("render")
            .expect("JSON output");
        serde_json::from_str(&rendered).expect("CLI JSON")
    };
    // Two captures alike, one promoted by the tool and one as the CLI does.
    let pair = |client: &mut Client| {
        let mut ids = Vec::new();
        for _ in 0..2 {
            let captured = client.ok("capture_create", json!({"title": "Original", "body": "Original body."}));
            ids.push(
                captured["data"]["capture"]["id"]
                    .as_str()
                    .expect("capture ID")
                    .to_owned(),
            );
        }
        (ids[0].clone(), ids[1].clone())
    };

    let spec = database()
        .create_spec("Spec".to_owned(), String::new(), String::new())
        .expect("spec");
    let plan = database()
        .create_plan(spec.id, "Plan".to_owned(), String::new())
        .expect("plan");
    let phase = database()
        .create_phase(plan.id, "Phase".to_owned(), String::new(), 0)
        .expect("phase");
    let (spec_id, plan_id, phase_id) = (spec.id.to_string(), plan.id.to_string(), phase.id.to_string());
    let parent = create_task(
        &mut client,
        json!({"title": "Parent", "spec": spec_id, "plan": plan_id, "phase": phase_id}),
    );

    let (tool_capture, cli_capture) = pair(&mut client);
    let promoted = client.ok(
        "capture_promote",
        json!({
            "capture_id": tool_capture,
            "target": "task",
            "title": "Promoted title",
            "body": "Promoted body.",
            "acceptance_criteria": "- Ignored for a task.",
            "spec": spec_id,
            "plan": plan_id,
            "phase": phase_id,
            "parent": parent,
            "priority": "high",
            "position": 7
        }),
    );
    let task = &promoted["data"]["task"];
    assert_eq!(
        (
            &task["title"],
            &task["body"],
            &task["spec_id"],
            &task["plan_id"],
            &task["phase_id"],
            &task["parent_id"],
            &task["priority"],
            &task["position"]
        ),
        (
            &json!("Promoted title"),
            &json!("Promoted body."),
            &json!(spec_id),
            &json!(plan_id),
            &json!(phase_id),
            &json!(parent),
            &json!("high"),
            &json!(7)
        ),
        "{task}"
    );
    let result = database()
        .promote_capture_to_task(
            CaptureId::parse(&cli_capture).expect("capture ID"),
            CaptureTaskPromotion {
                spec_id: Some(SpecId::parse(&spec_id).expect("spec ID")),
                plan_id: Some(PlanId::parse(&plan_id).expect("plan ID")),
                phase_id: Some(PhaseId::parse(&phase_id).expect("phase ID")),
                parent_id: Some(TaskId::parse(&parent).expect("task ID")),
                title: "Promoted title".to_owned(),
                body: "Promoted body.".to_owned(),
                priority: TaskPriority::parse("high").expect("priority"),
                position: 7,
            },
        )
        .expect("the store promotes the capture");
    let cli = render(&result);
    let renames = [
        (cli_capture.as_str(), tool_capture.as_str()),
        (
            &result.promotion.target.promotion_target_id(),
            task["id"].as_str().expect("task ID"),
        ),
    ];
    assert_eq!(normalize(&promoted, &[]), normalize(&cli, &renames));

    let (tool_capture, cli_capture) = pair(&mut client);
    let promoted = client.ok(
        "capture_promote",
        json!({
            "capture_id": tool_capture,
            "target": "spec",
            "title": "Promoted spec",
            "body": "Spec body.",
            "acceptance_criteria": "- It works."
        }),
    );
    assert_eq!(promoted["data"]["spec"]["acceptance_criteria"], json!("- It works."));
    let result = database()
        .promote_capture_to_spec(
            CaptureId::parse(&cli_capture).expect("capture ID"),
            "Promoted spec".to_owned(),
            "Spec body.".to_owned(),
            "- It works.".to_owned(),
        )
        .expect("the store promotes the capture");
    let cli = render(&result);
    let spec_id = promoted["data"]["spec"]["id"].as_str().expect("spec ID");
    let renames = [
        (cli_capture.as_str(), tool_capture.as_str()),
        (&result.promotion.target.promotion_target_id(), spec_id),
    ];
    assert_eq!(normalize(&promoted, &[]), normalize(&cli, &renames));
    client.finish();
}

#[test]
fn a_promote_is_exported_to_the_workspace() {
    let project = project(true);
    let mut client = Client::start(project.path());
    let captured = client.ok("capture_create", json!({"title": "Export the promote"}));
    let capture = captured["data"]["capture"]["id"]
        .as_str()
        .expect("capture ID")
        .to_owned();

    let promoted = client.ok(
        "capture_promote",
        json!({"capture_id": capture, "target": "task", "priority": "high"}),
    );
    assert!(promoted.get("warnings").is_none(), "{promoted}");
    let task = promoted["data"]["task"]["id"].as_str().expect("task ID");

    let workspace = project.path().join(".arcl/workspace");
    let capture_file =
        fs::read_to_string(workspace.join("captures").join(format!("{capture}.md"))).expect("the capture is exported");
    assert!(capture_file.contains("status = \"promoted\""), "{capture_file}");
    assert!(capture_file.contains(task), "{capture_file}");
    let task_file =
        fs::read_to_string(workspace.join("tasks").join(format!("{task}.md"))).expect("the task is exported");
    assert!(task_file.contains("title = \"Export the promote\""), "{task_file}");
    assert!(task_file.contains("priority = \"high\""), "{task_file}");
    client.finish();
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

/// `value` with each `(from, to)` ID renamed and every `*_at` timestamp
/// blanked, so two promotions of alike captures compare equal.
fn normalize(value: &Value, renames: &[(&str, &str)]) -> Value {
    let mut text = value.to_string();
    for (from, to) in renames {
        text = text.replace(from, to);
    }
    let mut value: Value = serde_json::from_str(&text).expect("JSON");
    blank_timestamps(&mut value);
    value
}

fn blank_timestamps(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, field) in map.iter_mut() {
                if key.ends_with("_at") && field.is_string() {
                    *field = json!("<time>");
                } else {
                    blank_timestamps(field);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(blank_timestamps),
        _ => {}
    }
}
