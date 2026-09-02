use std::{fs, path::Path};

use assert_cmd::Command;
use serde_json::Value;
use tempfile::{TempDir, tempdir};

fn arcl_in(directory: &Path) -> Command {
    let mut command = Command::cargo_bin("arcl").expect("Cargo exposes the arcl binary");
    command.current_dir(directory);
    command
}

fn initialized_directory() -> TempDir {
    let directory = tempdir().expect("temporary directory can be created");
    let output = arcl_in(directory.path())
        .args(["--color", "never", "init"])
        .output()
        .expect("arcl init runs");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    directory
}

fn json(output: &[u8]) -> Value {
    serde_json::from_slice(output).expect("JSON output parses")
}

fn id(output: &[u8], record: &str) -> String {
    json(output)["data"][record]["id"]
        .as_str()
        .expect("JSON output contains an ID")
        .to_owned()
}

fn run_json(directory: &Path, args: &[&str]) -> Value {
    let output = arcl_in(directory).args(args).output().expect("command runs");
    assert!(
        output.status.success(),
        "command failed: {}\nstderr: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty(), "successful JSON command wrote diagnostics");
    json(&output.stdout)
}

#[test]
fn help_uses_the_connected_vocabulary_without_compatibility_commands() {
    let directory = tempdir().expect("temporary directory can be created");
    let output = arcl_in(directory.path()).arg("--help").output().expect("help runs");
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    for command in ["capture", "spec", "plan", "task", "note"] {
        assert!(help.contains(command), "help does not mention {command}");
    }
    for command in ["idea", "epic", "milestone"] {
        assert!(!help.contains(command), "help still mentions removed command {command}");
    }
    assert!(help.contains("capture promote arcl-c-… spec"));
    assert!(help.contains("capture promote arcl-c-… task"));

    let old = arcl_in(directory.path())
        .args(["idea", "create", "obsolete"])
        .output()
        .expect("removed command is rejected");
    assert_eq!(old.status.code(), Some(2));
}

#[test]
fn capture_spec_plan_task_workflow_round_trips_through_json() {
    let directory = initialized_directory();
    let capture = run_json(
        directory.path(),
        [
            "--json",
            "capture",
            "create",
            "Import errors",
            "--body",
            "Capture details",
        ]
        .as_ref(),
    );
    let capture_id = capture["data"]["capture"]["id"].as_str().expect("capture ID");
    assert_eq!(capture["data"]["capture"]["body"], "Capture details");

    let promoted = run_json(
        directory.path(),
        [
            "--json",
            "capture",
            "promote",
            capture_id,
            "spec",
            "--acceptance-criteria",
            "Invalid records are rejected.",
        ]
        .as_ref(),
    );
    let spec_id = promoted["data"]["spec"]["id"].as_str().expect("spec ID");
    assert_eq!(promoted["data"]["capture"]["status"], "promoted");
    assert_eq!(promoted["data"]["spec"]["source_capture_id"], capture_id);

    let repeated = run_json(
        directory.path(),
        ["--json", "capture", "promote", capture_id, "spec"].as_ref(),
    );
    assert_eq!(repeated["data"]["spec"]["id"], spec_id);

    let plan = run_json(
        directory.path(),
        [
            "--json",
            "plan",
            "create",
            "Implementation",
            "--spec",
            spec_id,
            "--body",
            "Plan details",
        ]
        .as_ref(),
    );
    let plan_id = plan["data"]["plan"]["id"].as_str().expect("plan ID");
    let task = run_json(
        directory.path(),
        [
            "--json",
            "task",
            "create",
            "Validate records",
            "--plan",
            plan_id,
            "--priority",
            "high",
        ]
        .as_ref(),
    );
    let task_id = task["data"]["task"]["id"].as_str().expect("task ID");
    assert_eq!(task["data"]["task"]["plan_id"], plan_id);

    let shown = run_json(directory.path(), ["--json", "task", "show", task_id].as_ref());
    assert_eq!(shown["data"]["task"]["id"], task_id);
    assert_eq!(shown["data"]["spec"]["id"], spec_id);
    assert_eq!(shown["data"]["plan"]["id"], plan_id);

    let listed = run_json(directory.path(), ["--json", "spec", "list"].as_ref());
    assert_eq!(listed["data"]["specs"].as_array().expect("spec list").len(), 1);
    let ready = run_json(directory.path(), ["--json", "ready", "--plan", plan_id].as_ref());
    assert_eq!(ready["data"]["tasks"][0]["id"], task_id);
}

#[test]
fn capture_can_promote_directly_to_a_task_and_invalid_json_writes_structured_stderr() {
    let directory = initialized_directory();
    let capture = run_json(
        directory.path(),
        ["--json", "capture", "create", "Small fix", "--body", "Do the small fix"].as_ref(),
    );
    let capture_id = capture["data"]["capture"]["id"].as_str().expect("capture ID");
    let promoted = run_json(
        directory.path(),
        [
            "--json",
            "capture",
            "promote",
            capture_id,
            "task",
            "--priority",
            "critical",
        ]
        .as_ref(),
    );
    let task_id = promoted["data"]["task"]["id"].as_str().expect("task ID");
    assert_eq!(promoted["data"]["task"]["body"], "Do the small fix");

    let invalid = arcl_in(directory.path())
        .args(["--json", "task", "show", "arcl-t-not-an-id"])
        .output()
        .expect("invalid command runs");
    assert_eq!(invalid.status.code(), Some(3));
    assert!(invalid.stdout.is_empty());
    let error = json(&invalid.stderr);
    assert_eq!(error["format_version"], 1);
    assert_eq!(error["error"]["code"], "invalid_request");
    assert!(
        error["error"]["message"]
            .as_str()
            .expect("error message")
            .contains("invalid")
    );

    let shown = run_json(directory.path(), ["--json", "capture", "show", capture_id].as_ref());
    assert_eq!(shown["data"]["capture"]["status"], "promoted");
    let task = run_json(directory.path(), ["--json", "task", "show", task_id].as_ref());
    assert_eq!(task["data"]["task"]["body"], "Do the small fix");
}

#[test]
fn structured_plan_check_diff_apply_and_repeat_are_deterministic() {
    let directory = initialized_directory();
    let spec = run_json(directory.path(), ["--json", "spec", "create", "Feature"].as_ref());
    let spec_id = spec["data"]["spec"]["id"].as_str().expect("spec ID");
    let plan = run_json(
        directory.path(),
        ["--json", "plan", "create", "Build feature", "--spec", spec_id].as_ref(),
    );
    let plan_id = plan["data"]["plan"]["id"].as_str().expect("plan ID");
    let input_path = directory.path().join("plan.toml");
    fs::write(
        &input_path,
        "format-version = 1\n\n[[phases]]\nkey = \"build\"\ntitle = \"Build\"\nposition = 0\n\n[[phases.tasks]]\nkey = \"schema\"\ntitle = \"Update schema\"\nposition = 0\n",
    )
    .expect("plan input writes");
    let input = input_path.to_str().expect("plan path is UTF-8");

    let checked = run_json(
        directory.path(),
        ["--json", "plan", "check", plan_id, "--file", input].as_ref(),
    );
    assert_eq!(checked["data"]["phases"][0]["change"], "create");
    assert_eq!(checked["data"]["tasks"][0]["key"], "build/schema");
    let diff = run_json(
        directory.path(),
        ["--json", "plan", "diff", plan_id, "--file", input].as_ref(),
    );
    assert_eq!(diff["data"], checked["data"]);

    let applied = run_json(
        directory.path(),
        ["--json", "plan", "apply", plan_id, "--file", input].as_ref(),
    );
    assert_eq!(applied["data"]["plan"]["id"], plan_id);
    assert_eq!(applied["data"]["phases"].as_array().expect("phases").len(), 1);
    assert_eq!(applied["data"]["tasks"].as_array().expect("tasks").len(), 1);

    let repeated = run_json(
        directory.path(),
        ["--json", "plan", "apply", plan_id, "--file", input].as_ref(),
    );
    assert_eq!(repeated["data"]["tasks"][0]["id"], applied["data"]["tasks"][0]["id"]);
    let unchanged = run_json(
        directory.path(),
        ["--json", "plan", "diff", plan_id, "--file", input].as_ref(),
    );
    assert_eq!(unchanged["data"]["tasks"][0]["change"], "unchanged");
}

#[test]
fn task_dependencies_lifecycle_handoff_context_and_plain_output_use_connected_records() {
    let directory = initialized_directory();
    let blocker = run_json(directory.path(), ["--json", "task", "create", "Blocker"].as_ref());
    let blocker_id = blocker["data"]["task"]["id"].as_str().expect("blocker ID");
    let task = run_json(directory.path(), ["--json", "task", "create", "Blocked"].as_ref());
    let task_id = task["data"]["task"]["id"].as_str().expect("task ID");
    run_json(
        directory.path(),
        ["--json", "dependency", "add", task_id, "--blocked-by", blocker_id].as_ref(),
    );

    let ready = arcl_in(directory.path())
        .args(["--plain", "ready"])
        .output()
        .expect("ready runs");
    assert!(ready.status.success());
    assert_eq!(String::from_utf8_lossy(&ready.stdout).trim(), blocker_id);
    assert!(!ready.stdout.contains(&0x1b));

    run_json(directory.path(), ["--json", "task", "start", blocker_id].as_ref());
    let handoff = run_json(
        directory.path(),
        [
            "--json",
            "task",
            "handoff",
            blocker_id,
            "--note",
            "Resume with the query.",
        ]
        .as_ref(),
    );
    assert_eq!(handoff["data"]["task"]["status"], "parked");
    assert_eq!(handoff["data"]["task"]["handoff"], "Resume with the query.");

    run_json(directory.path(), ["--json", "task", "unpark", blocker_id].as_ref());
    run_json(
        directory.path(),
        ["--json", "task", "complete", blocker_id, "--evidence", "Verified"].as_ref(),
    );
    let context = run_json(directory.path(), ["--json", "context", task_id].as_ref());
    assert_eq!(context["data"]["blockers"][0]["task"]["id"], blocker_id);
    assert_eq!(context["data"]["completion_evidence"][0]["evidence"], "Verified");

    let next = run_json(directory.path(), ["--json", "next"].as_ref());
    assert_eq!(next["data"]["task"]["id"], task_id);
}

#[test]
fn notes_release_membership_and_file_bodies_are_exposed_as_json() {
    let directory = initialized_directory();
    let body_path = directory.path().join("note.md");
    fs::write(&body_path, "# Decision\n\nUse SQLite.\n").expect("note body writes");
    let body_path = body_path.to_str().expect("body path is UTF-8");
    let note = run_json(
        directory.path(),
        ["--json", "note", "create", "Decision", "--body-file", body_path].as_ref(),
    );
    let note_id = id(&serde_json::to_vec(&note).expect("value serializes"), "note");
    assert_eq!(note["data"]["note"]["body"], "# Decision\n\nUse SQLite.\n");

    let release = run_json(directory.path(), ["--json", "release", "create", "v1"].as_ref());
    let release_id = release["data"]["release"]["id"].as_str().expect("release ID");
    let membership = run_json(
        directory.path(),
        [
            "--json",
            "release",
            "member",
            "add",
            release_id,
            "--kind",
            "note",
            "--record-id",
            &note_id,
        ]
        .as_ref(),
    );
    assert_eq!(membership["data"]["membership"]["record_id"], note_id);
    let members = run_json(
        directory.path(),
        ["--json", "release", "member", "list", release_id].as_ref(),
    );
    assert_eq!(members["data"]["memberships"][0]["record_id"], note_id);

    let links = run_json(
        directory.path(),
        [
            "--json",
            "note",
            "link",
            "add",
            &note_id,
            "--kind",
            "release",
            "--record-id",
            release_id,
        ]
        .as_ref(),
    );
    assert_eq!(links["data"]["link"]["record_id"], release_id);
    let shown = run_json(directory.path(), ["--json", "note", "show", &note_id].as_ref());
    assert_eq!(shown["data"]["note"]["title"], "Decision");
}
