//! CLI tests for `arcl compact` and the archived tasks it leaves, on a
//! scratch project with the workspace on.

use std::{
    fs,
    path::{Path, PathBuf},
};

use assert_cmd::Command;
use serde_json::Value;
use tempfile::{TempDir, tempdir};

/// A date after every completion in these tests.
const LATER: &str = "2999-01-01";
/// A date before every completion in these tests.
const EARLIER: &str = "2000-01-01";

/// A scratch project with the workspace on, a spec, a plan and two phases.
struct Project {
    directory: TempDir,
    home: TempDir,
    first: String,
    second: String,
}

impl Project {
    /// Initialize the project and create `Milestone 1` and `Milestone 2`.
    fn new() -> Self {
        let project = Self {
            directory: tempdir().expect("project directory creates"),
            home: tempdir().expect("home directory creates"),
            first: String::new(),
            second: String::new(),
        };
        project.json(&["init", "--workspace"]);
        let spec = project.id(&["spec", "create", "Feature"], "spec");
        let plan = project.id(&["plan", "create", "Roadmap", "--spec", &spec], "plan");
        let phase = |title: &str, position: &str| {
            project.id(
                &[
                    "plan",
                    "phase",
                    "create",
                    title,
                    "--plan",
                    &plan,
                    "--position",
                    position,
                ],
                "phase",
            )
        };
        let first = phase("Milestone 1", "0");
        let second = phase("Milestone 2", "1");
        Self { first, second, ..project }
    }

    fn root(&self) -> &Path {
        self.directory.path()
    }

    /// An `arcl` command in the project with no Git state or home from the
    /// host.
    fn arcl(&self) -> Command {
        arcl_in(self.root(), self.home.path())
    }

    /// Run `arcl --json <args>`, require success and no stderr, and return
    /// the envelope.
    fn json(&self, args: &[&str]) -> Value {
        let output = self.arcl().arg("--json").args(args).output().expect("arcl runs");
        assert!(
            output.status.success(),
            "arcl {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty(), "{}", String::from_utf8_lossy(&output.stderr));
        serde_json::from_slice(&output.stdout).expect("JSON output parses")
    }

    /// Run `arcl <args>` with colour off, require success, and return stdout.
    fn stdout(&self, args: &[&str]) -> String {
        let output = self
            .arcl()
            .args(["--color", "never"])
            .args(args)
            .output()
            .expect("arcl runs");
        assert!(
            output.status.success(),
            "arcl {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("stdout is UTF-8")
    }

    /// Run a create command and return the ID of the `record` it made.
    fn id(&self, args: &[&str], record: &str) -> String {
        self.json(args)["data"][record]["id"]
            .as_str()
            .expect("JSON output holds an ID")
            .to_owned()
    }

    /// Create a task in `phase`, under `parent` when given.
    fn task(&self, title: &str, phase: &str, parent: Option<&str>) -> String {
        match parent {
            Some(parent) => self.id(&["task", "create", title, "--parent", parent], "task"),
            None => self.id(&["task", "create", title, "--phase", phase], "task"),
        }
    }

    /// Create a task in `phase` and complete it with `evidence`.
    fn done(&self, title: &str, phase: &str, evidence: &str) -> String {
        let task = self.task(title, phase, None);
        self.json(&["task", "complete", &task, "--evidence", evidence]);
        task
    }

    /// The task as `arcl show` returns it.
    fn show(&self, task: &str) -> Value {
        self.json(&["show", task])["data"]["task"].clone()
    }

    /// The IDs `arcl list --kind <kind>` returns, with `extra` arguments.
    fn listed(&self, kind: &str, extra: &[&str]) -> Vec<String> {
        let mut args = vec!["list", "--kind", kind];
        args.extend(extra);
        self.json(&args)["data"]["records"]
            .as_array()
            .expect("records array")
            .iter()
            .map(|record| record["id"].as_str().expect("record ID").to_owned())
            .collect()
    }

    /// Run `arcl compact` with `args` and return the envelope's data.
    fn compact(&self, args: &[&str]) -> Value {
        let mut all = vec!["compact"];
        all.extend(args);
        self.json(&all)["data"].clone()
    }

    /// Every file under `.arcl`, the database included, with its bytes.
    fn files(&self) -> Vec<(PathBuf, Vec<u8>)> {
        tree_files(&self.root().join(".arcl"))
    }

    /// The path of a workspace record file.
    fn record_path(&self, kind: &str, id: &str) -> PathBuf {
        self.root().join(".arcl/workspace").join(kind).join(format!("{id}.md"))
    }
}

/// An `arcl` command in `directory` with no Git state from the host and
/// `home` as its home.
fn arcl_in(directory: &Path, home: &Path) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin("arcl"));
    for variable in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_CEILING_DIRECTORIES",
    ] {
        command.env_remove(variable);
    }
    command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .current_dir(directory);
    command
}

/// Every file under `root` with its bytes, in path order.
fn tree_files(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).expect("directory reads") {
            let path = entry.expect("entry reads").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push((path.clone(), fs::read(&path).expect("file reads")));
            }
        }
    }
    files.sort();
    files
}

/// The IDs of the tasks in one phase entry of a compaction.
fn archived_ids(phase: &Value) -> Vec<String> {
    let mut ids = phase["tasks"]
        .as_array()
        .expect("tasks array")
        .iter()
        .map(|task| task["id"].as_str().expect("task ID").to_owned())
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

fn sorted(mut ids: Vec<String>) -> Vec<String> {
    ids.sort();
    ids
}

#[test]
fn task_complete_records_a_completion_time() {
    let project = Project::new();
    let task = project.done("Alpha", &project.first, "Shipped");
    let completed_at = project.show(&task)["completed_at"]
        .as_str()
        .expect("completion time")
        .to_owned();
    assert_eq!(completed_at.len(), "2026-11-08T10:00:00.000Z".len(), "{completed_at}");
    assert!(completed_at.ends_with('Z'), "{completed_at}");
    let file = fs::read_to_string(project.record_path("tasks", &task)).expect("task file reads");
    assert!(file.contains(&format!("completed-at = \"{completed_at}\"\n")), "{file}");
    let manifest = fs::read_to_string(project.root().join(".arcl/workspace/manifest.toml")).expect("manifest reads");
    assert_eq!(manifest, "format-version = 6\n");
    assert!(project.show(&task).get("archived_at").is_none());

    let pending = project.task("Pending", &project.first, None);
    assert!(project.show(&pending).get("completed_at").is_none());
}

#[test]
fn compaction_leaves_one_note_per_phase_and_archives_its_tasks() {
    let project = Project::new();
    let alpha = project.done("Alpha", &project.first, "Shipped alpha");
    let parent = project.task("Parent", &project.first, None);
    let child = project.task("Child", &project.first, Some(&parent));
    project.json(&["task", "complete", &child, "--evidence", "Child done"]);
    project.json(&["task", "complete", &parent, "--evidence", "Parent done"]);
    // A completed task that blocks a pending one stays, and so do the
    // pending and cancelled tasks.
    let blocker = project.done("Blocker", &project.first, "Unblocks waiting");
    let waiting = project.task("Waiting", &project.first, None);
    project.json(&["dependency", "add", &waiting, "--blocked-by", &blocker]);
    let dropped = project.task("Dropped", &project.first, None);
    project.json(&["task", "cancel", &dropped]);
    let beta = project.done("Beta", &project.second, "Shipped beta");

    let compaction = project.compact(&["--before", LATER]);
    assert_eq!(compaction["dry_run"], false);
    assert!(compaction["archived_at"].is_string(), "{compaction}");
    let phases = compaction["phases"].as_array().expect("phases array");
    assert_eq!(phases.len(), 2, "{compaction}");
    assert_eq!(phases[0]["phase_id"], project.first.as_str());
    assert_eq!(
        archived_ids(&phases[0]),
        sorted(vec![alpha.clone(), parent.clone(), child.clone()])
    );
    assert_eq!(phases[1]["phase_id"], project.second.as_str());
    assert_eq!(archived_ids(&phases[1]), vec![beta.clone()]);
    for phase in phases {
        assert_eq!(phase["note_created"], true);
    }

    let notes = project.listed("note", &[]);
    assert_eq!(notes.len(), 2, "one note per phase");
    let first_note = phases[0]["note_id"].as_str().expect("note ID");
    let note = project.json(&["note", "show", first_note])["data"]["note"].clone();
    assert_eq!(note["title"], "Compacted work: Milestone 1");
    let body = note["body"].as_str().expect("note body");
    for (task, title, evidence) in [
        (&alpha, "Alpha", "Shipped alpha"),
        (&parent, "Parent", "Parent done"),
        (&child, "Child", "Child done"),
    ] {
        assert!(body.contains(&format!("## {title}\n\n`{task}` · completed ")), "{body}");
        assert!(body.contains(evidence), "{body}");
    }
    assert!(!body.contains("Unblocks waiting"), "{body}");
    let links = project.json(&["note", "link", "list", first_note]);
    assert!(links.to_string().contains(&project.first), "{links}");

    for task in [&alpha, &parent, &child, &beta] {
        let shown = project.show(task);
        assert_eq!(shown["status"], "completed");
        assert_eq!(shown["archived_at"], compaction["archived_at"]);
        let file = fs::read_to_string(project.record_path("tasks", task)).expect("task file reads");
        assert!(file.contains("archived-at = "), "{file}");
    }
    for task in [&blocker, &waiting, &dropped] {
        assert!(project.show(task).get("archived_at").is_none(), "{task}");
    }
}

#[test]
fn rerunning_compaction_changes_nothing_and_new_work_appends_to_the_note() {
    let project = Project::new();
    project.done("Alpha", &project.first, "Shipped alpha");
    let first = project.compact(&["--before", LATER]);
    let note = first["phases"][0]["note_id"].as_str().expect("note ID").to_owned();

    let before = project.files();
    let again = project.compact(&["--before", LATER]);
    assert_eq!(again["phases"], Value::Array(Vec::new()), "{again}");
    assert_eq!(again["archived_at"], Value::Null);
    assert!(project.files() == before, "a repeat compaction changed a file");

    let gamma = project.done("Gamma", &project.first, "Shipped gamma");
    let appended = project.compact(&["--before", LATER]);
    assert_eq!(appended["phases"][0]["note_id"], note.as_str());
    assert_eq!(appended["phases"][0]["note_created"], false);
    assert_eq!(archived_ids(&appended["phases"][0]), vec![gamma.clone()]);
    assert_eq!(project.listed("note", &[]), vec![note.clone()]);
    let body = project.json(&["note", "show", &note])["data"]["note"]["body"]
        .as_str()
        .expect("note body")
        .to_owned();
    let alpha_at = body.find("## Alpha").expect("Alpha section");
    let gamma_at = body.find("## Gamma").expect("Gamma section");
    assert!(alpha_at < gamma_at, "{body}");
    assert!(body.contains(&format!("`{gamma}`")), "{body}");
}

#[test]
fn archived_tasks_are_hidden_by_default_and_shown_with_all() {
    let project = Project::new();
    let alpha = project.done("Alpha", &project.first, "Shipped alpha");
    let pending = project.task("Pending", &project.first, None);
    // `follow` stays: it is in the second phase, which `--phase` leaves out.
    let follow = project.done("Follow-up", &project.second, "Built on alpha");
    project.json(&["dependency", "add", &follow, "--blocked-by", &alpha]);

    let compaction = project.compact(&["--before", LATER, "--phase", &project.first]);
    assert_eq!(compaction["phases"].as_array().expect("phases").len(), 1);
    assert_eq!(archived_ids(&compaction["phases"][0]), vec![alpha.clone()]);

    let tasks = project.listed("task", &[]);
    assert!(!tasks.contains(&alpha), "{tasks:?}");
    assert!(tasks.contains(&pending) && tasks.contains(&follow), "{tasks:?}");
    assert!(project.listed("task", &["--all"]).contains(&alpha));

    let tree = project.stdout(&["tree"]);
    assert!(!tree.contains("Alpha"), "{tree}");
    // The archived task still counts as done.
    assert!(tree.contains("Milestone 1  1/2"), "{tree}");
    let all = project.stdout(&["tree", "--all"]);
    assert!(all.contains("Alpha") && all.contains("Milestone 1  1/2"), "{all}");
    let tree_json = project.json(&["tree"]).to_string();
    assert!(!tree_json.contains(&alpha), "{tree_json}");
    assert!(tree_json.contains("\"archived_tasks\":1"), "{tree_json}");
    let roadmap = project.stdout(&["roadmap"]);
    assert!(roadmap.contains("| Milestone 1 | in progress | 1/2 |"), "{roadmap}");

    let context = project.json(&["context", &follow])["data"].clone();
    assert_eq!(context["blockers"], Value::Array(Vec::new()), "{context}");
    assert_eq!(context["completion_evidence"], Value::Array(Vec::new()), "{context}");
    let context = project.json(&["context", &follow, "--all"])["data"].clone();
    assert_eq!(context["blockers"][0]["task"]["id"], alpha.as_str(), "{context}");
    let context = project.json(&["task", "context", &follow, "--all"])["data"].clone();
    assert_eq!(
        context["completion_evidence"][0]["task"]["id"],
        alpha.as_str(),
        "{context}"
    );

    let shown = project.show(&alpha);
    assert_eq!(shown["title"], "Alpha");
    assert!(shown["archived_at"].is_string(), "{shown}");
    let rooted = project.stdout(&["tree", &alpha]);
    assert!(rooted.contains("Alpha"), "{rooted}");
}

#[test]
fn a_dry_run_writes_nothing() {
    let project = Project::new();
    let alpha = project.done("Alpha", &project.first, "Shipped alpha");
    let before = project.files();

    let compaction = project.compact(&["--before", LATER, "--dry-run"]);
    assert_eq!(compaction["dry_run"], true);
    assert_eq!(compaction["archived_at"], Value::Null);
    assert_eq!(compaction["phases"][0]["note_id"], Value::Null);
    assert_eq!(compaction["phases"][0]["note_title"], "Compacted work: Milestone 1");
    assert_eq!(archived_ids(&compaction["phases"][0]), vec![alpha.clone()]);
    let human = project.stdout(&["compact", "--before", LATER, "--dry-run"]);
    assert!(
        human.starts_with("Would archive 1 task in 1 phase. Nothing was written."),
        "{human}"
    );

    assert!(project.files() == before, "a dry run changed a file under .arcl");
    assert!(project.show(&alpha).get("archived_at").is_none());
    assert!(project.listed("note", &[]).is_empty());
}

#[test]
fn a_task_completed_after_the_date_stays() {
    let project = Project::new();
    let alpha = project.done("Alpha", &project.first, "Shipped alpha");
    let compaction = project.compact(&["--before", EARLIER]);
    assert_eq!(compaction["phases"], Value::Array(Vec::new()));
    assert!(project.show(&alpha).get("archived_at").is_none());
    assert_eq!(
        project.stdout(&["compact", "--before", EARLIER]).trim_end(),
        "No completed tasks to archive."
    );

    let refused = project
        .arcl()
        .args(["--json", "compact", "--before", "next week"])
        .output()
        .expect("arcl runs");
    assert_eq!(refused.status.code(), Some(3));
}

#[test]
fn undated_tasks_are_archived_only_with_include_undated() {
    let project = Project::new();
    let undated = project.done("Undated", &project.first, "Done long ago");
    // Remove the completion time, as a task completed by an older arcl has
    // none. The next command imports the edited file.
    let path = project.record_path("tasks", &undated);
    let file = fs::read_to_string(&path).expect("task file reads");
    let edited = file
        .lines()
        .filter(|line| !line.starts_with("completed-at = "))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&path, format!("{edited}\n")).expect("task file writes");
    assert!(project.show(&undated).get("completed_at").is_none());

    let compaction = project.compact(&["--before", LATER]);
    assert_eq!(compaction["phases"], Value::Array(Vec::new()), "{compaction}");

    let compaction = project.compact(&["--before", EARLIER, "--include-undated"]);
    assert_eq!(archived_ids(&compaction["phases"][0]), vec![undated.clone()]);
    let note = compaction["phases"][0]["note_id"].as_str().expect("note ID");
    let body = project.json(&["note", "show", note])["data"]["note"]["body"].clone();
    assert!(
        body.as_str()
            .expect("note body")
            .contains(&format!("`{undated}` · completed at an unknown time")),
        "{body}"
    );
}

#[test]
fn completion_and_archive_times_round_trip_through_the_workspace() {
    let project = Project::new();
    let alpha = project.done("Alpha", &project.first, "Shipped alpha");
    let beta = project.done("Beta", &project.second, "Shipped beta");
    project.compact(&["--before", LATER, "--phase", &project.first]);
    let alpha_before = project.show(&alpha);
    let beta_before = project.show(&beta);

    let target = tempdir().expect("target directory creates");
    let home = tempdir().expect("home directory creates");
    let run = |args: &[&str]| {
        let output = arcl_in(target.path(), home.path())
            .args(args)
            .output()
            .expect("arcl runs");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        output.stdout
    };
    run(&["init", "--workspace"]);
    let source = project.root().join(".arcl/workspace");
    for (path, content) in tree_files(&source) {
        let relative = path.strip_prefix(&source).expect("path is under the workspace");
        let destination = target.path().join(".arcl/workspace").join(relative);
        fs::create_dir_all(destination.parent().expect("parent")).expect("directory creates");
        fs::write(destination, content).expect("file writes");
    }
    run(&["workspace", "import"]);
    let show = |task: &str| {
        let output = run(&["--json", "show", task]);
        serde_json::from_slice::<Value>(&output).expect("JSON parses")["data"]["task"].clone()
    };
    for (task, before) in [(&alpha, &alpha_before), (&beta, &beta_before)] {
        let after = show(task);
        assert_eq!(after["completed_at"], before["completed_at"], "{after}");
        assert_eq!(after["archived_at"], before["archived_at"], "{after}");
    }
    assert!(show(&alpha)["archived_at"].is_string());
    assert!(show(&beta).get("archived_at").is_none());
}

#[test]
fn a_workspace_written_before_completion_times_imports() {
    let project = Project::new();
    let alpha = project.done("Alpha", &project.first, "Shipped alpha");
    let workspace = project.root().join(".arcl/workspace");
    let path = project.record_path("tasks", &alpha);
    let file = fs::read_to_string(&path).expect("task file reads");
    let old = file
        .lines()
        .filter(|line| !line.starts_with("completed-at = "))
        .collect::<Vec<_>>()
        .join("\n");

    let target = tempdir().expect("target directory creates");
    let home = tempdir().expect("home directory creates");
    let run = |args: &[&str]| {
        let output = arcl_in(target.path(), home.path())
            .args(args)
            .output()
            .expect("arcl runs");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        output.stdout
    };
    run(&["init", "--workspace"]);
    let destination = target.path().join(".arcl/workspace");
    for (path, content) in tree_files(&workspace) {
        let relative = path.strip_prefix(&workspace).expect("path is under the workspace");
        let to = destination.join(relative);
        fs::create_dir_all(to.parent().expect("parent")).expect("directory creates");
        fs::write(to, content).expect("file writes");
    }
    // Version 5 predates `completed-at`, so its task files have none.
    fs::write(destination.join("manifest.toml"), "format-version = 5\n").expect("manifest writes");
    fs::write(
        destination.join("tasks").join(format!("{alpha}.md")),
        format!("{old}\n"),
    )
    .expect("task file writes");
    run(&["workspace", "import"]);
    let shown = serde_json::from_slice::<Value>(&run(&["--json", "show", &alpha])).expect("JSON parses");
    assert_eq!(shown["data"]["task"]["status"], "completed");
    assert_eq!(shown["data"]["task"]["evidence"], "Shipped alpha");
    assert!(shown["data"]["task"].get("completed_at").is_none(), "{shown}");
}
