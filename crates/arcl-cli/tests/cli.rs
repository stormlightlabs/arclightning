use std::{
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use assert_cmd::Command;
use serde_json::Value;
use tempfile::{TempDir, tempdir};

/// Git variables that point a command at another repository, index or object
/// store. A Git hook in a linked worktree exports an absolute `GIT_DIR` and
/// `GIT_INDEX_FILE`, so a test run from that hook would otherwise commit into
/// the developer's repository.
const GIT_LOCATION_VARIABLES: [&str; 9] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_NAMESPACE",
    "GIT_CEILING_DIRECTORIES",
    "GIT_DISCOVERY_ACROSS_FILESYSTEM",
];

/// The Git state arcl must leave alone: HEAD, refs, an unfinished merge, the
/// index, local configuration and changes outside the workspace.
#[derive(Debug, Eq, PartialEq)]
struct GitState {
    head: String,
    refs: String,
    merge_head: Option<String>,
    index: String,
    config: String,
    outside_workspace: String,
}

impl GitState {
    /// Read the state of the Git worktree at `directory`.
    fn read(directory: &Path) -> Self {
        let git_dir = directory.join(".git");
        let head = fs::read_to_string(git_dir.join("HEAD")).expect("HEAD reads");
        Self {
            head: head + &git(directory, &["rev-parse", "HEAD"]),
            refs: git(directory, &["for-each-ref"]),
            merge_head: fs::read_to_string(git_dir.join("MERGE_HEAD")).ok(),
            index: git(directory, &["ls-files", "--stage"]),
            config: git(directory, &["config", "--list", "--local"]),
            outside_workspace: git(
                directory,
                &["status", "--porcelain", "--", ".", ":(exclude).arcl/workspace"],
            ),
        }
    }
}

/// Build a command for `program` in `directory` that sees no Git state from the
/// host. It drops [`GIT_LOCATION_VARIABLES`], ignores global and system config,
/// and points `HOME` and `XDG_CONFIG_HOME` at an empty directory, since Git and
/// gix read `$XDG_CONFIG_HOME/git/ignore` even with global config off. A host
/// excludes file that ignores `.arcl/` or `*.md` would otherwise hide records
/// from both the test's `git` and arcl.
fn isolated_command(program: impl AsRef<std::ffi::OsStr>, directory: &Path) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    for variable in GIT_LOCATION_VARIABLES {
        command.env_remove(variable);
    }
    let home = empty_home();
    command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .current_dir(directory);
    command
}

/// An empty directory under Cargo's test temp directory, created once and
/// shared by every command as `HOME`.
fn empty_home() -> &'static Path {
    static HOME: OnceLock<PathBuf> = OnceLock::new();
    HOME.get_or_init(|| {
        let home = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-empty-home");
        fs::create_dir_all(&home).expect("empty home creates");
        home
    })
}

/// Build an `arcl` command in `directory`, isolated from host Git state.
fn arcl_in(directory: &Path) -> Command {
    Command::from_std(isolated_command(assert_cmd::cargo::cargo_bin("arcl"), directory))
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
    for command in ["idea", "epic", "milestone", "snapshot"] {
        assert!(!help.contains(command), "help still mentions removed command {command}");
    }
    assert!(help.contains("capture promote arcl-c-… spec"));
    assert!(help.contains("capture promote arcl-c-… task"));

    let old = arcl_in(directory.path())
        .args(["idea", "create", "obsolete"])
        .output()
        .expect("removed command is rejected");
    assert_eq!(old.status.code(), Some(2));

    let snapshot = arcl_in(directory.path())
        .args(["snapshot", "export"])
        .output()
        .expect("removed snapshot noun is rejected");
    assert_eq!(snapshot.status.code(), Some(2));
}

#[test]
fn init_with_workspace_creates_the_layout_under_arcl_workspace() {
    let directory = tempdir().expect("temporary directory can be created");
    let init = run_json(directory.path(), ["--json", "init", "--workspace"].as_ref());
    assert_eq!(init["data"]["workspace_enabled"], true);

    let config = fs::read_to_string(directory.path().join(".arcl/config.toml")).expect("config is written");
    assert!(config.contains("enabled = true"), "config: {config}");
    assert!(config.contains("path = \".arcl/workspace\""), "config: {config}");

    let workspace = directory.path().join(".arcl/workspace");
    assert!(workspace.join("manifest.toml").is_file());
    for kind in ["captures", "releases", "specs", "plans", "phases", "tasks", "notes"] {
        assert!(workspace.join(kind).is_dir(), "missing workspace directory {kind}");
    }
}

#[test]
fn workspace_export_and_import_round_trip_under_arcl_workspace() {
    let disabled = initialized_directory();
    let refused = arcl_in(disabled.path())
        .args(["--json", "workspace", "export"])
        .output()
        .expect("export runs");
    assert_eq!(refused.status.code(), Some(3));

    let source = tempdir().expect("temporary directory can be created");
    run_json(source.path(), ["--json", "init", "--workspace"].as_ref());
    let capture = run_json(
        source.path(),
        [
            "--json",
            "capture",
            "create",
            "Exported idea",
            "--body",
            "Workspace body",
        ]
        .as_ref(),
    );
    let capture_id = capture["data"]["capture"]["id"].as_str().expect("capture ID");
    let exported = arcl_in(source.path())
        .args(["workspace", "export"])
        .output()
        .expect("export runs");
    assert!(
        exported.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&exported.stderr)
    );
    let record_path = format!(".arcl/workspace/captures/{capture_id}.md");
    let record = fs::read_to_string(source.path().join(&record_path)).expect("capture record is exported");
    assert!(record.contains("Exported idea"), "record: {record}");

    let target = tempdir().expect("temporary directory can be created");
    run_json(target.path(), ["--json", "init", "--workspace"].as_ref());
    for file in [".arcl/workspace/manifest.toml", record_path.as_str()] {
        fs::copy(source.path().join(file), target.path().join(file)).expect("workspace file copies");
    }
    let imported = arcl_in(target.path())
        .args(["workspace", "import"])
        .output()
        .expect("import runs");
    assert!(
        imported.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&imported.stderr)
    );
    let shown = run_json(target.path(), ["--json", "capture", "show", capture_id].as_ref());
    assert_eq!(shown["data"]["capture"]["title"], "Exported idea");
    assert_eq!(shown["data"]["capture"]["body"], "Workspace body");
}

#[test]
fn workspace_sync_carries_one_sided_changes_and_refuses_divergence() {
    let directory = tempdir().expect("temporary directory can be created");
    run_json(directory.path(), ["--json", "init", "--workspace"].as_ref());
    let capture = run_json(
        directory.path(),
        ["--json", "capture", "create", "Synced idea", "--body", "Database body"].as_ref(),
    );
    let capture_id = capture["data"]["capture"]["id"].as_str().expect("capture ID");
    let sync = || {
        arcl_in(directory.path())
            .args(["workspace", "sync"])
            .output()
            .expect("sync runs")
    };

    let exported = sync();
    assert!(
        exported.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&exported.stderr)
    );
    let record_path = directory
        .path()
        .join(format!(".arcl/workspace/captures/{capture_id}.md"));
    let record = fs::read_to_string(&record_path).expect("capture record is exported");
    assert!(record.contains("Database body"), "record: {record}");

    fs::write(&record_path, record.replace("Database body", "Workspace body")).expect("record edits");
    let imported = sync();
    assert!(
        imported.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&imported.stderr)
    );
    let shown = run_json(directory.path(), ["--json", "capture", "show", capture_id].as_ref());
    assert_eq!(shown["data"]["capture"]["body"], "Workspace body");

    let record = fs::read_to_string(&record_path).expect("record reads");
    fs::write(&record_path, record.replace("Workspace body", "Edited in the file")).expect("record edits");
    with_workspace_off(directory.path(), || {
        run_json(
            directory.path(),
            [
                "--json",
                "capture",
                "update",
                capture_id,
                "--body",
                "Edited in the database",
            ]
            .as_ref(),
        )
    });
    let refused = sync();
    assert_eq!(refused.status.code(), Some(4));
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        stderr.contains(&format!("captures/{capture_id}.md")),
        "stderr: {stderr}"
    );
    assert!(
        fs::read_to_string(&record_path)
            .expect("record reads")
            .contains("Edited in the file")
    );
}

/// Run `action` with the workspace switched off in `.arcl/config.toml`, so
/// writes reach only the database and leave the workspace behind.
fn with_workspace_off<T>(directory: &Path, action: impl FnOnce() -> T) -> T {
    let config = directory.join(".arcl/config.toml");
    let on = fs::read_to_string(&config).expect("config reads");
    assert!(on.contains("enabled = true"), "config: {on}");
    fs::write(&config, on.replace("enabled = true", "enabled = false")).expect("config writes");
    let result = action();
    fs::write(&config, on).expect("config writes");
    result
}

/// Run `git` in `directory` with a fixed identity and default branch, and
/// return its output whether or not it succeeded. The command is built by
/// [`isolated_command`], so settings such as `merge.ff = only`, a hooks path or
/// a `GIT_DIR` on the host cannot change what the scenarios do.
fn git_output(directory: &Path, args: &[&str]) -> std::process::Output {
    isolated_command("git", directory)
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "init.defaultBranch=main",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .expect("git runs")
}

/// Run `git` in `directory`, require success and return its stdout.
fn git(directory: &Path, args: &[&str]) -> String {
    let output = git_output(directory, args);
    assert!(
        output.status.success(),
        "git {}: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("git prints UTF-8")
}

#[test]
fn workspace_status_reports_changes_head_and_unresolved_merges() {
    let directory = tempdir().expect("temporary directory can be created");
    git(directory.path(), &["init", "-q"]);
    run_json(directory.path(), ["--json", "init", "--workspace"].as_ref());
    let unborn = run_json(directory.path(), ["--json", "workspace", "status"].as_ref());
    assert_eq!(unborn["data"]["head"]["kind"], "unborn");
    assert_eq!(unborn["data"]["head"]["branch"], "main");

    let capture = run_json(
        directory.path(),
        ["--json", "capture", "create", "Status idea", "--body", "First body"].as_ref(),
    );
    let capture_id = capture["data"]["capture"]["id"].as_str().expect("capture ID");
    let record = format!("captures/{capture_id}.md");
    let exported = run_json(directory.path(), ["--json", "workspace", "status"].as_ref());
    assert_eq!(exported["data"]["records"], Value::Array(Vec::new()));
    assert!(directory.path().join(".arcl/workspace").join(&record).is_file());

    let synced = arcl_in(directory.path())
        .args(["workspace", "sync"])
        .output()
        .expect("sync runs");
    assert!(
        synced.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&synced.stderr)
    );
    let uncommitted = run_json(directory.path(), ["--json", "workspace", "status"].as_ref());
    assert_eq!(uncommitted["data"]["records"], Value::Array(Vec::new()));
    let git_changes = uncommitted["data"]["git_changes"].as_array().expect("git changes");
    assert!(
        git_changes
            .iter()
            .any(|change| change["path"] == record.as_str() && change["git"] == "untracked"),
        "git changes: {git_changes:?}"
    );
    git(directory.path(), &["add", "."]);
    git(directory.path(), &["commit", "-q", "-m", "base"]);
    let clean = run_json(directory.path(), ["--json", "workspace", "status"].as_ref());
    assert_eq!(clean["data"]["head"]["kind"], "branch");
    assert_eq!(clean["data"]["records"], Value::Array(Vec::new()));
    assert_eq!(clean["data"]["git_changes"], Value::Array(Vec::new()));

    git(directory.path(), &["checkout", "-q", "--detach"]);
    let detached = run_json(directory.path(), ["--json", "workspace", "status"].as_ref());
    assert_eq!(detached["data"]["head"]["kind"], "detached");
    git(directory.path(), &["checkout", "-q", "main"]);

    let record_path = directory.path().join(".arcl/workspace").join(&record);
    let base = fs::read_to_string(&record_path).expect("record reads");
    git(directory.path(), &["checkout", "-q", "-b", "other"]);
    fs::write(&record_path, base.replace("First body", "Other body")).expect("record writes");
    git(directory.path(), &["commit", "-q", "-am", "other"]);
    git(directory.path(), &["checkout", "-q", "main"]);
    fs::write(&record_path, base.replace("First body", "Main body")).expect("record writes");
    git(directory.path(), &["commit", "-q", "-am", "main"]);
    let merge = git_output(directory.path(), &["merge", "-q", "other"]);
    assert!(!merge.status.success(), "the merge should conflict");

    let conflicted = run_json(directory.path(), ["--json", "workspace", "status"].as_ref());
    assert_eq!(conflicted["data"]["unmerged"][0], record);
    assert_eq!(conflicted["data"]["conflict_markers"][0], record);
    assert_eq!(conflicted["data"]["records"][0]["git"], "conflicted");

    let human = arcl_in(directory.path())
        .args(["--color", "never", "workspace", "status"])
        .output()
        .expect("status runs");
    let text = String::from_utf8_lossy(&human.stdout);
    assert!(text.contains("Unmerged in Git:"), "status: {text}");
    assert!(text.contains("Sync will stop"), "status: {text}");

    let refused = arcl_in(directory.path())
        .args(["workspace", "sync"])
        .output()
        .expect("sync runs");
    assert_eq!(refused.status.code(), Some(4));
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        stderr.contains("unmerged in Git") && stderr.contains(&record),
        "stderr: {stderr}"
    );
    let after = arcl_in(directory.path())
        .args(["--json", "capture", "show", capture_id])
        .output()
        .expect("show runs");
    assert!(after.status.success(), "a read still works during a merge");
    assert!(String::from_utf8_lossy(&after.stderr).contains("unmerged in Git"));
    assert_eq!(json(&after.stdout)["data"]["capture"]["body"], "First body");
}

#[test]
fn workspace_resolve_keeps_the_chosen_side_of_a_divergent_file() {
    let directory = tempdir().expect("temporary directory can be created");
    run_json(directory.path(), ["--json", "init", "--workspace"].as_ref());
    let capture = run_json(
        directory.path(),
        ["--json", "capture", "create", "Resolve idea", "--body", "First body"].as_ref(),
    );
    let capture_id = capture["data"]["capture"]["id"].as_str().expect("capture ID");
    let workspace = |args: &[&str]| {
        arcl_in(directory.path())
            .arg("workspace")
            .args(args)
            .output()
            .expect("workspace command runs")
    };
    assert!(workspace(&["sync"]).status.success());

    let record = format!("captures/{capture_id}.md");
    let record_path = directory.path().join(".arcl/workspace").join(&record);
    let diverge = |file_body: &str, database_body: &str| {
        let content = fs::read_to_string(&record_path).expect("record reads");
        let current = run_json(directory.path(), ["--json", "capture", "show", capture_id].as_ref());
        let body = current["data"]["capture"]["body"].as_str().expect("body").to_owned();
        fs::write(&record_path, content.replace(&body, file_body)).expect("record writes");
        with_workspace_off(directory.path(), || {
            run_json(
                directory.path(),
                ["--json", "capture", "update", capture_id, "--body", database_body].as_ref(),
            )
        });
    };

    diverge("File wins", "Database loses");
    assert_eq!(workspace(&["sync"]).status.code(), Some(4));
    let resolved = workspace(&["resolve", "--keep", "workspace", &record]);
    assert!(
        resolved.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&resolved.stderr)
    );
    let shown = run_json(directory.path(), ["--json", "capture", "show", capture_id].as_ref());
    assert_eq!(shown["data"]["capture"]["body"], "File wins");

    diverge("File loses", "Database wins");
    let resolved = workspace(&["resolve", "--keep", "database", &record]);
    assert!(
        resolved.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&resolved.stderr)
    );
    assert!(
        fs::read_to_string(&record_path)
            .expect("record reads")
            .contains("Database wins")
    );

    diverge("File from a subdirectory", "Database loses again");
    let subdirectory = directory.path().join("src");
    fs::create_dir(&subdirectory).expect("subdirectory creates");
    let resolved = arcl_in(&subdirectory)
        .args(["workspace", "resolve", "--keep", "workspace"])
        .arg(format!("../.arcl/workspace/{record}"))
        .output()
        .expect("workspace command runs");
    assert!(
        resolved.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&resolved.stderr)
    );
    let shown = run_json(directory.path(), ["--json", "capture", "show", capture_id].as_ref());
    assert_eq!(shown["data"]["capture"]["body"], "File from a subdirectory");
    assert!(workspace(&["sync"]).status.success());

    let nothing = workspace(&["resolve", "--keep", "database", &record]);
    assert_eq!(nothing.status.code(), Some(3));
    let missing_side = workspace(&["resolve", &record]);
    assert_eq!(missing_side.status.code(), Some(2));
}

#[test]
fn commands_sync_the_workspace_around_reads_and_writes() {
    let directory = tempdir().expect("temporary directory can be created");
    run_json(directory.path(), ["--json", "init", "--workspace"].as_ref());
    let capture = run_json(
        directory.path(),
        ["--json", "capture", "create", "Auto idea", "--body", "Created body"].as_ref(),
    );
    let capture_id = capture["data"]["capture"]["id"].as_str().expect("capture ID");
    let record_path = directory
        .path()
        .join(format!(".arcl/workspace/captures/{capture_id}.md"));
    let record = fs::read_to_string(&record_path).expect("a write exports its record");
    assert!(record.contains("Created body"), "record: {record}");

    fs::write(&record_path, record.replace("Created body", "Edited in the file")).expect("record writes");
    let shown = run_json(directory.path(), ["--json", "capture", "show", capture_id].as_ref());
    assert_eq!(
        shown["data"]["capture"]["body"], "Edited in the file",
        "a read imports first"
    );

    run_json(
        directory.path(),
        ["--json", "capture", "update", capture_id, "--title", "Renamed"].as_ref(),
    );
    let record = fs::read_to_string(&record_path).expect("record reads");
    assert!(
        record.contains("Renamed") && record.contains("Edited in the file"),
        "record: {record}"
    );

    let note = with_workspace_off(directory.path(), || {
        run_json(directory.path(), ["--json", "note", "create", "Missed export"].as_ref())
    });
    let note_id = note["data"]["note"]["id"].as_str().expect("note ID");
    let note_path = directory.path().join(format!(".arcl/workspace/notes/{note_id}.md"));
    assert!(!note_path.exists());
    run_json(directory.path(), ["--json", "list"].as_ref());
    assert!(
        note_path.is_file(),
        "the next command exports a change that missed its export"
    );

    fs::write(&record_path, record.replace("Edited in the file", "File side")).expect("record writes");
    with_workspace_off(directory.path(), || {
        run_json(
            directory.path(),
            ["--json", "capture", "update", capture_id, "--body", "Database side"].as_ref(),
        )
    });

    let refused = arcl_in(directory.path())
        .args(["capture", "update", capture_id, "--title", "Blocked"])
        .output()
        .expect("update runs");
    assert_eq!(refused.status.code(), Some(4));
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("arcl workspace status"), "stderr: {stderr}");
    assert!(stderr.contains("arcl workspace resolve"), "stderr: {stderr}");
    let read = arcl_in(directory.path())
        .args(["--json", "capture", "show", capture_id])
        .output()
        .expect("show runs");
    assert!(read.status.success());
    assert!(String::from_utf8_lossy(&read.stderr).contains("warning"));
    assert_eq!(json(&read.stdout)["data"]["capture"]["title"], "Renamed");
    assert_eq!(json(&read.stdout)["data"]["capture"]["body"], "Database side");
    assert!(
        fs::read_to_string(&record_path)
            .expect("record reads")
            .contains("File side")
    );
}

#[cfg(unix)]
#[test]
fn commands_treat_a_symlinked_workspace_as_a_failed_sync() {
    let directory = tempdir().expect("temporary directory can be created");
    run_json(directory.path(), ["--json", "init", "--workspace"].as_ref());
    let workspace = directory.path().join(".arcl/workspace");
    let moved = directory.path().join("moved-workspace");
    fs::rename(&workspace, &moved).expect("workspace moves");
    std::os::unix::fs::symlink(&moved, &workspace).expect("symlink is created");

    let refused = arcl_in(directory.path())
        .args(["capture", "create", "Blocked"])
        .output()
        .expect("create runs");
    assert_eq!(refused.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("symbolic link"), "stderr: {stderr}");
    assert!(
        !stderr.contains("workspace resolve"),
        "resolve cannot fix a symlinked workspace: {stderr}"
    );

    let read = arcl_in(directory.path())
        .args(["--json", "capture", "list"])
        .output()
        .expect("list runs");
    assert!(read.status.success());
    let stderr = String::from_utf8_lossy(&read.stderr);
    assert!(
        stderr.contains("warning") && stderr.contains("symbolic link"),
        "stderr: {stderr}"
    );
    assert_eq!(json(&read.stdout)["data"]["captures"], Value::Array(Vec::new()));
    let warning: Value = serde_json::from_slice(&read.stderr).expect("stderr is one JSON value");
    assert_eq!(warning["format_version"], 1);
    assert_eq!(warning["warning"]["code"], "workspace_sync");

    let quiet = arcl_in(directory.path())
        .args(["--quiet", "capture", "list"])
        .output()
        .expect("list runs");
    assert!(quiet.status.success());
    assert!(
        quiet.stderr.is_empty(),
        "stderr: {}",
        String::from_utf8_lossy(&quiet.stderr)
    );

    let failed = arcl_in(directory.path())
        .args(["--json", "capture", "show", "not-an-id"])
        .output()
        .expect("show runs");
    assert_eq!(failed.status.code(), Some(3));
    let error: Value = serde_json::from_slice(&failed.stderr).expect("stderr is one JSON value");
    assert_eq!(error["error"]["code"], "invalid_request");
}

#[test]
fn parallel_writes_with_the_workspace_on_all_succeed() {
    let directory = tempdir().expect("temporary directory can be created");
    run_json(directory.path(), ["--json", "init", "--workspace"].as_ref());
    let children = (0..16)
        .map(|index| {
            isolated_command(assert_cmd::cargo::cargo_bin("arcl"), directory.path())
                .args(["--json", "note", "create", &format!("Parallel {index}")])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .expect("arcl spawns")
        })
        .collect::<Vec<_>>();
    for child in children {
        let output = child.wait_with_output().expect("arcl finishes");
        assert!(
            output.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stderr.is_empty(),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let note_id = id(&output.stdout, "note");
        assert!(
            directory
                .path()
                .join(format!(".arcl/workspace/notes/{note_id}.md"))
                .is_file(),
            "note {note_id} was not exported"
        );
    }
    let status = run_json(directory.path(), ["--json", "workspace", "status"].as_ref());
    assert_eq!(status["data"]["records"], Value::Array(Vec::new()));
}

#[test]
fn workspace_status_reports_a_record_only_the_database_has() {
    let directory = tempdir().expect("temporary directory can be created");
    run_json(directory.path(), ["--json", "init", "--workspace"].as_ref());
    let capture = with_workspace_off(directory.path(), || {
        run_json(
            directory.path(),
            ["--json", "capture", "create", "Database only"].as_ref(),
        )
    });
    let capture_id = capture["data"]["capture"]["id"].as_str().expect("capture ID");
    let status = run_json(directory.path(), ["--json", "workspace", "status"].as_ref());
    let record = &status["data"]["records"][0];
    assert_eq!(record["path"], format!("captures/{capture_id}.md"));
    assert_eq!(record["state"], "database-changed");
    assert_eq!(record["database"], "added");
}

#[test]
fn a_missing_database_reports_not_initialized_once() {
    let directory = tempdir().expect("temporary directory can be created");
    run_json(directory.path(), ["--json", "init", "--workspace"].as_ref());
    fs::remove_file(directory.path().join(".arcl/arcl.db")).expect("database is removed");

    for args in [["--json", "list"].as_ref(), ["--json", "note", "create", "x"].as_ref()] {
        let output = arcl_in(directory.path()).args(args).output().expect("command runs");
        assert_eq!(output.status.code(), Some(3));
        let stderr: Value = serde_json::from_slice(&output.stderr).expect("stderr is one JSON value");
        assert!(stderr.to_string().contains("not initialized"), "stderr: {stderr}");
    }
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
fn tree_orders_plan_children_by_position_not_id() {
    let directory = initialized_directory();
    let dir = directory.path();
    let spec = run_json(dir, ["--json", "spec", "create", "Feature"].as_ref());
    let spec_id = spec["data"]["spec"]["id"].as_str().expect("spec ID");
    let plan = run_json(dir, ["--json", "plan", "create", "Build", "--spec", spec_id].as_ref());
    let plan_id = plan["data"]["plan"]["id"].as_str().expect("plan ID").to_owned();

    // Each record is created after the last, so IDs ascend while positions descend.
    let create_phase = |title: &str, position: &str| {
        let phase = run_json(
            dir,
            [
                "--json",
                "plan",
                "phase",
                "create",
                title,
                "--plan",
                &plan_id,
                "--position",
                position,
            ]
            .as_ref(),
        );
        phase["data"]["phase"]["id"].as_str().expect("phase ID").to_owned()
    };
    let create_task = |title: &str, container: &str, id: &str, position: &str| {
        let task = run_json(
            dir,
            ["--json", "task", "create", title, container, id, "--position", position].as_ref(),
        );
        task["data"]["task"]["id"].as_str().expect("task ID").to_owned()
    };
    create_task("Loose", "--plan", &plan_id, "0");
    let third = create_phase("Third", "2");
    let second = create_phase("Second", "1");
    let first = create_phase("First", "0");
    assert!(third < second && second < first, "phase IDs ascend in creation order");
    create_task("Later", "--phase", &first, "1");
    let earlier = create_task("Earlier", "--phase", &first, "0");
    create_task("Sub later", "--parent", &earlier, "1");
    create_task("Sub earlier", "--parent", &earlier, "0");

    let tree = run_json(dir, ["--json", "tree", &plan_id].as_ref());
    let titles = |node: &Value| -> Vec<String> {
        node["children"]
            .as_array()
            .expect("children")
            .iter()
            .map(|child| child["title"].as_str().expect("title").to_owned())
            .collect()
    };
    let plan_node = &tree["data"]["nodes"][0];
    assert_eq!(titles(plan_node), ["First", "Second", "Third", "Loose"]);
    let first_node = &plan_node["children"][0];
    assert_eq!(titles(first_node), ["Earlier", "Later"]);
    assert_eq!(titles(&first_node["children"][0]), ["Sub earlier", "Sub later"]);
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

/// Create a Git repository with an arcl project and the workspace on.
fn git_project() -> TempDir {
    let directory = tempdir().expect("temporary directory can be created");
    git(directory.path(), &["init", "-q"]);
    run_json(directory.path(), ["--json", "init", "--workspace"].as_ref());
    directory
}

fn commit_all(directory: &Path, message: &str) {
    git(directory, &["add", "-A"]);
    git(directory, &["commit", "-q", "-m", message]);
}

fn create_capture(directory: &Path, title: &str, body: &str) -> String {
    let created = run_json(
        directory,
        ["--json", "capture", "create", title, "--body", body].as_ref(),
    );
    created["data"]["capture"]["id"]
        .as_str()
        .expect("capture ID")
        .to_owned()
}

fn update_body(directory: &Path, capture_id: &str, body: &str) {
    run_json(
        directory,
        ["--json", "capture", "update", capture_id, "--body", body].as_ref(),
    );
}

fn capture_field(directory: &Path, capture_id: &str, field: &str) -> String {
    let shown = run_json(directory, ["--json", "capture", "show", capture_id].as_ref());
    shown["data"]["capture"][field]
        .as_str()
        .expect("capture field")
        .to_owned()
}

fn assert_success(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "status: {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn git_branches_that_change_different_records_merge_and_import() {
    let directory = git_project();
    let path = directory.path();
    let first = create_capture(path, "First", "First body");
    let second = create_capture(path, "Second", "Second body");
    commit_all(path, "base");

    git(path, &["checkout", "-q", "-b", "left"]);
    update_body(path, &first, "Left body");
    commit_all(path, "left");
    git(path, &["checkout", "-q", "main"]);
    assert_eq!(
        capture_field(path, &first, "body"),
        "First body",
        "the checkout's files import"
    );
    update_body(path, &second, "Main body");
    commit_all(path, "main");
    git(path, &["merge", "-q", "--no-edit", "left"]);

    let before = GitState::read(path);
    let listed = run_json(path, ["--json", "capture", "list"].as_ref());
    assert_eq!(listed["data"]["captures"].as_array().map(Vec::len), Some(2));
    assert_eq!(capture_field(path, &first, "body"), "Left body");
    assert_eq!(capture_field(path, &second, "body"), "Main body");
    assert_eq!(GitState::read(path), before);
    assert_eq!(
        git(path, &["status", "--porcelain"]),
        "",
        "the import leaves the merged files as committed"
    );
}

#[test]
fn an_external_edit_in_a_git_worktree_imports_without_staging() {
    let directory = git_project();
    let path = directory.path();
    let capture_id = create_capture(path, "Edited", "Committed body");
    commit_all(path, "base");

    let record = format!(".arcl/workspace/captures/{capture_id}.md");
    let content = fs::read_to_string(path.join(&record)).expect("record reads");
    fs::write(path.join(&record), content.replace("Committed body", "Edited body")).expect("record writes");
    let before = GitState::read(path);
    assert_eq!(capture_field(path, &capture_id, "body"), "Edited body");
    assert_eq!(GitState::read(path), before);
    assert_eq!(git(path, &["status", "--porcelain"]), format!(" M {record}\n"));
}

#[test]
fn a_branch_switch_imports_its_records_and_stops_on_records_it_lacks() {
    let origin = git_project();
    let kept = create_capture(origin.path(), "On main", "Main body");
    commit_all(origin.path(), "base");
    git(origin.path(), &["checkout", "-q", "-b", "feature"]);
    let added = create_capture(origin.path(), "On feature", "Feature body");
    commit_all(origin.path(), "feature");
    git(origin.path(), &["checkout", "-q", "main"]);

    let parent = tempdir().expect("temporary directory can be created");
    let source = origin.path().to_str().expect("UTF-8 path");
    git(parent.path(), &["clone", "-q", source, "clone"]);
    let path = &parent.path().join("clone");
    run_json(path, ["--json", "init", "--workspace"].as_ref());
    assert_success(
        &arcl_in(path)
            .args(["workspace", "import"])
            .output()
            .expect("import runs"),
    );
    assert_eq!(capture_field(path, &kept, "body"), "Main body");

    git(path, &["checkout", "-q", "feature"]);
    assert_eq!(
        capture_field(path, &added, "body"),
        "Feature body",
        "the branch's records appear"
    );

    git(path, &["checkout", "-q", "main"]);
    let before = GitState::read(path);
    let record = format!("captures/{added}.md");
    let refused = arcl_in(path)
        .args(["capture", "update", &kept, "--title", "Blocked"])
        .output()
        .expect("update runs");
    assert_eq!(refused.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        stderr.contains(&record) && stderr.contains("does not support hard deletion"),
        "stderr: {stderr}"
    );

    assert_success(
        &arcl_in(path)
            .args(["workspace", "export"])
            .output()
            .expect("export runs"),
    );
    assert!(path.join(".arcl/workspace").join(&record).is_file());
    assert_success(&arcl_in(path).args(["workspace", "sync"]).output().expect("sync runs"));
    assert_eq!(capture_field(path, &kept, "title"), "On main");
    assert_eq!(GitState::read(path), before);
    assert_eq!(
        git(path, &["status", "--porcelain"]),
        format!("?? .arcl/workspace/{record}\n")
    );
}

#[test]
fn an_unresolved_merge_stops_writes_until_git_finishes_it() {
    let directory = git_project();
    let path = directory.path();
    let capture_id = create_capture(path, "Merged", "First body");
    commit_all(path, "base");
    git(path, &["checkout", "-q", "-b", "other"]);
    update_body(path, &capture_id, "Other body");
    commit_all(path, "other");
    git(path, &["checkout", "-q", "main"]);
    update_body(path, &capture_id, "Main body");
    commit_all(path, "main");
    let merge = git_output(path, &["merge", "-q", "other"]);
    assert!(!merge.status.success(), "the merge should conflict");

    let record = format!("captures/{capture_id}.md");
    let record_path = path.join(".arcl/workspace").join(&record);
    let conflicted = fs::read(&record_path).expect("record reads");
    let before = GitState::read(path);
    assert!(before.merge_head.is_some());
    for args in [
        ["capture", "update", capture_id.as_str(), "--title", "Blocked"].as_ref(),
        ["workspace", "sync"].as_ref(),
    ] {
        let refused = arcl_in(path).args(args).output().expect("command runs");
        assert_eq!(refused.status.code(), Some(4), "{}", args.join(" "));
        let stderr = String::from_utf8_lossy(&refused.stderr);
        assert!(
            stderr.contains("unmerged in Git") && stderr.contains(&record),
            "stderr: {stderr}"
        );
    }
    assert_eq!(GitState::read(path), before);
    assert_eq!(fs::read(&record_path).expect("record reads"), conflicted);

    let ours = git(path, &["show", &format!("HEAD:.arcl/workspace/{record}")]);
    fs::write(&record_path, ours.replace("Main body", "Merged body")).expect("record writes");
    git(path, &["add", "-A"]);
    git(path, &["commit", "-q", "--no-edit"]);
    assert_success(&arcl_in(path).args(["workspace", "sync"]).output().expect("sync runs"));
    assert_eq!(capture_field(path, &capture_id, "body"), "Merged body");
    assert_eq!(capture_field(path, &capture_id, "title"), "Merged");
}

#[test]
fn a_pull_over_an_unsynced_database_change_needs_an_explicit_side() {
    let directory = git_project();
    let path = directory.path();
    let capture_id = create_capture(path, "Diverged", "First body");
    commit_all(path, "base");
    let record = format!("captures/{capture_id}.md");
    let record_path = path.join(".arcl/workspace").join(&record);

    // A branch changes the file while the database holds a change that never
    // reached the workspace, then the branch merges in.
    let diverge = |branch: &str, file_body: &str, database_body: &str| {
        let current = capture_field(path, &capture_id, "body");
        git(path, &["checkout", "-q", "-b", branch]);
        let content = fs::read_to_string(&record_path).expect("record reads");
        fs::write(&record_path, content.replace(&current, file_body)).expect("record writes");
        commit_all(path, branch);
        git(path, &["checkout", "-q", "main"]);
        with_workspace_off(path, || update_body(path, &capture_id, database_body));
        git(path, &["merge", "-q", "--ff-only", branch]);
    };
    let resolve = |keep: &str| {
        arcl_in(path)
            .args(["workspace", "resolve", "--keep", keep, &record])
            .output()
            .expect("resolve runs")
    };

    diverge("file-wins", "File wins", "Database loses");
    let before = GitState::read(path);
    let refused = arcl_in(path).args(["workspace", "sync"]).output().expect("sync runs");
    assert_eq!(refused.status.code(), Some(4));
    assert!(String::from_utf8_lossy(&refused.stderr).contains(&record));
    assert_success(&resolve("workspace"));
    assert_eq!(capture_field(path, &capture_id, "body"), "File wins");
    assert_eq!(GitState::read(path), before);
    assert_eq!(git(path, &["status", "--porcelain"]), "");

    diverge("database-wins", "File loses", "Database wins");
    let before = GitState::read(path);
    assert_success(&resolve("database"));
    assert_eq!(capture_field(path, &capture_id, "body"), "Database wins");
    assert!(
        fs::read_to_string(&record_path)
            .expect("record reads")
            .contains("Database wins")
    );
    assert_eq!(GitState::read(path), before);
    assert_eq!(
        git(path, &["status", "--porcelain"]),
        format!(" M .arcl/workspace/{record}\n")
    );
}

#[test]
fn the_next_command_restores_workspace_files_a_write_left_behind() {
    let directory = git_project();
    let path = directory.path();
    let capture_id = create_capture(path, "Exported", "First body");
    commit_all(path, "base");
    let record = format!("captures/{capture_id}.md");
    let record_path = path.join(".arcl/workspace").join(&record);

    // A write whose export never ran, as when arcl stops after the database
    // commits.
    with_workspace_off(path, || update_body(path, &capture_id, "Saved body"));
    let before = GitState::read(path);
    run_json(path, ["--json", "list"].as_ref());
    let content = fs::read_to_string(&record_path).expect("record reads");
    assert!(content.contains("Saved body"), "record: {content}");
    assert_eq!(GitState::read(path), before);

    // A record file deleted after a write is not a request to delete the
    // record: writes stop, and resolve writes the file back.
    fs::remove_file(&record_path).expect("record is removed");
    let refused = arcl_in(path)
        .args(["note", "create", "Blocked"])
        .output()
        .expect("note create runs");
    assert_eq!(refused.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        stderr.contains(&format!("arcl workspace resolve --keep database {record}")),
        "stderr: {stderr}"
    );
    assert_success(
        &arcl_in(path)
            .args(["workspace", "resolve", "--keep", "database", &record])
            .output()
            .expect("resolve runs"),
    );
    assert_eq!(fs::read_to_string(&record_path).expect("record reads"), content);
    assert_eq!(GitState::read(path), before);
}

/// Create a task, under `parent` when given, and return its ID.
fn create_task(directory: &Path, title: &str, parent: Option<&str>) -> String {
    let mut args = vec!["--json", "task", "create", title];
    if let Some(parent) = parent {
        args.extend(["--parent", parent]);
    }
    run_json(directory, &args)["data"]["task"]["id"]
        .as_str()
        .expect("task ID")
        .to_owned()
}

/// Run a command that must fail with exit code 3 and return its error message.
fn refused_message(directory: &Path, args: &[&str]) -> String {
    let output = arcl_in(directory).args(args).output().expect("command runs");
    assert_eq!(
        output.status.code(),
        Some(3),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    json(&output.stderr)["error"]["message"]
        .as_str()
        .expect("error message")
        .to_owned()
}

/// The blocker edges stored for the project, as `(task, blocker)` pairs.
fn dependency_pairs(directory: &Path) -> Vec<(String, String)> {
    let list = run_json(directory, ["--json", "dependency", "list"].as_ref());
    list["data"]["dependencies"]
        .as_array()
        .expect("dependency array")
        .iter()
        .map(|edge| {
            (
                edge["task_id"].as_str().expect("task ID").to_owned(),
                edge["blocker_id"].as_str().expect("blocker ID").to_owned(),
            )
        })
        .collect()
}

#[test]
fn dependency_edge_cycle_is_refused_with_its_path() {
    let directory = initialized_directory();
    let path = directory.path();
    let (a, b, c) = (
        create_task(path, "A", None),
        create_task(path, "B", None),
        create_task(path, "C", None),
    );
    run_json(path, ["--json", "dependency", "add", &a, "--blocked-by", &b].as_ref());
    run_json(path, ["--json", "dependency", "add", &b, "--blocked-by", &c].as_ref());
    let before = dependency_pairs(path);

    let message = refused_message(path, ["--json", "dependency", "add", &c, "--blocked-by", &a].as_ref());
    assert!(
        message.contains(&format!("cycle: {c} -> {a} -> {b} -> {c}")),
        "message: {message}"
    );
    assert_eq!(dependency_pairs(path), before);
}

#[test]
fn dependency_edge_ancestor_is_refused_by_add_and_create() {
    let directory = initialized_directory();
    let path = directory.path();
    let root = create_task(path, "Root", None);
    let child = create_task(path, "Child", Some(&root));

    let message = refused_message(
        path,
        ["--json", "dependency", "add", &child, "--blocked-by", &root].as_ref(),
    );
    assert!(
        message.contains(&format!("its ancestor; parent chain: {child} -> {root}")),
        "message: {message}"
    );

    let message = refused_message(
        path,
        [
            "--json",
            "task",
            "create",
            "Grandchild",
            "--parent",
            &child,
            "--blocked-by",
            &root,
        ]
        .as_ref(),
    );
    assert!(message.contains("its ancestor; parent chain: "), "message: {message}");
    assert!(
        message.ends_with(&format!(" -> {child} -> {root}")),
        "message: {message}"
    );
    assert!(dependency_pairs(path).is_empty());
    let tasks = run_json(path, ["--json", "task", "list"].as_ref());
    assert_eq!(tasks["data"]["tasks"].as_array().expect("task array").len(), 2);
}

#[test]
fn dependency_edge_descendant_is_refused() {
    let directory = initialized_directory();
    let path = directory.path();
    let root = create_task(path, "Root", None);
    let child = create_task(path, "Child", Some(&root));
    let grandchild = create_task(path, "Grandchild", Some(&child));

    let message = refused_message(
        path,
        ["--json", "dependency", "add", &root, "--blocked-by", &grandchild].as_ref(),
    );
    assert!(
        message.contains(&format!(
            "its descendant; parent chain: {grandchild} -> {child} -> {root}"
        )),
        "message: {message}"
    );
    assert!(dependency_pairs(path).is_empty());
}

#[test]
fn dependency_edge_existing_violation_does_not_block_unrelated_add() {
    let directory = initialized_directory();
    let path = directory.path();
    let root = create_task(path, "Root", None);
    let child = create_task(path, "Child", Some(&root));
    let (a, b) = (create_task(path, "A", None), create_task(path, "B", None));
    let database = rusqlite::Connection::open(path.join(".arcl/arcl.db")).expect("database opens");
    for (task, blocker) in [(&child, &root), (&a, &b), (&b, &a)] {
        database
            .execute(
                "INSERT INTO planning_task_dependencies (project_id, task_id, blocker_id)
                 SELECT project_id, ?1, ?2 FROM planning_tasks WHERE id = ?1",
                [task, blocker],
            )
            .expect("bad edge inserts");
    }
    drop(database);

    let other = create_task(path, "Other", None);
    run_json(
        path,
        ["--json", "dependency", "add", &other, "--blocked-by", &a].as_ref(),
    );
    run_json(
        path,
        [
            "--json",
            "task",
            "create",
            "Late",
            "--parent",
            &root,
            "--blocked-by",
            &b,
        ]
        .as_ref(),
    );
    assert_eq!(dependency_pairs(path).len(), 5);
}
