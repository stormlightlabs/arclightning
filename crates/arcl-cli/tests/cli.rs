use std::{
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use assert_cmd::Command;
use serde_json::Value;
use tempfile::{TempDir, tempdir};
use unicode_width::UnicodeWidthStr;

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
    let warnings = warning["warnings"].as_array().expect("warnings array");
    assert_eq!(warnings.len(), 1, "warnings: {warnings:?}");
    assert_eq!(warnings[0]["code"], "workspace_sync");
    assert!(
        warnings[0]["message"]
            .as_str()
            .expect("message")
            .contains("symbolic link")
    );

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
fn tree_lines_start_with_the_status_mark() {
    let directory = initialized_directory();
    let dir = directory.path();
    let done = create_task(dir, "Done", None);
    let active = create_task(dir, "Active", None);
    let parked = create_task(dir, "Parked", None);
    let dropped = create_task(dir, "Dropped", None);
    let ready = create_task(dir, "Ready", None);
    let blocked = create_task(dir, "Blocked", None);
    run_json(
        dir,
        ["--json", "task", "complete", &done, "--evidence", "Verified"].as_ref(),
    );
    run_json(dir, ["--json", "task", "start", &active].as_ref());
    run_json(dir, ["--json", "task", "park", &parked].as_ref());
    run_json(dir, ["--json", "task", "cancel", &dropped].as_ref());
    block_start_task(dir, &blocked, &ready);

    let human = stdout_of(dir, &["--color", "never", "tree"]);
    let mark = |title: &str| tree_line(&human, title).0;
    assert_eq!(mark("Done"), '✓');
    assert_eq!(mark("Active"), '◐');
    assert_eq!(mark("Parked"), '‖');
    assert_eq!(mark("Dropped"), '✗');
    assert_eq!(mark("Ready"), '●');
    assert_eq!(mark("Blocked"), '○');
    assert_eq!(
        tree_line(&human, "Blocked").1,
        format!("Blocked  blocked by {}", short_id(dir, &ready))
    );

    let plain = stdout_of(dir, &["--plain", "tree"]);
    for (id, status, title) in [
        (&done, "completed", "Done"),
        (&active, "in_progress", "Active"),
        (&parked, "parked", "Parked"),
        (&dropped, "cancelled", "Dropped"),
        (&ready, "pending", "Ready"),
        (&blocked, "pending", "Blocked"),
    ] {
        let line = format!("0\ttask\t{id}\t{status}\t{title}");
        assert!(
            plain.lines().any(|plain_line| plain_line == line),
            "{line} in:\n{plain}"
        );
    }
}

#[test]
fn rooted_tree_names_the_blockers_a_task_inherits() {
    let directory = initialized_directory();
    let dir = directory.path();
    let blocker = create_task(dir, "Blocker", None);
    let parent = create_task(dir, "Parent", None);
    let child = create_task(dir, "Child", Some(&parent));
    block_start_task(dir, &parent, &blocker);
    let blocker_short = short_id(dir, &blocker);

    let rooted = stdout_of(dir, &["--color", "never", "tree", &child]);
    assert_eq!(
        tree_line(&rooted, "Child").1,
        format!("Child  blocked by {blocker_short}")
    );

    // The full tree names the blocker once, on the parent's line.
    let full = stdout_of(dir, &["--color", "never", "tree"]);
    assert_eq!(
        tree_line(&full, "Parent").1,
        format!("Parent  blocked by {blocker_short}")
    );
    assert_eq!(tree_line(&full, "Child").1, "Child");
}

#[test]
fn tree_leaves_out_closed_captures_unless_all() {
    let directory = initialized_directory();
    let dir = directory.path();
    let open = create_capture(dir, "Open idea", "Still open");
    let promoted = create_capture(dir, "Promoted idea", "Became a task");
    let discarded = create_capture(dir, "Discarded idea", "Not doing it");
    run_json(dir, ["--json", "capture", "promote", &promoted, "task"].as_ref());
    run_json(dir, ["--json", "capture", "discard", &discarded].as_ref());

    let capture_ids = |args: &[&str]| -> Vec<String> {
        let tree = run_json(dir, args);
        tree["data"]["nodes"]
            .as_array()
            .expect("tree nodes")
            .iter()
            .filter(|node| node["kind"] == "capture")
            .map(|node| node["id"].as_str().expect("a node ID").to_owned())
            .collect()
    };
    assert_eq!(capture_ids(&["--json", "tree"]), std::slice::from_ref(&open));
    let mut all = capture_ids(&["--json", "tree", "--all"]);
    all.sort();
    let mut expected = vec![open.clone(), promoted.clone(), discarded.clone()];
    expected.sort();
    assert_eq!(all, expected);

    let plain = stdout_of(dir, &["--plain", "tree"]);
    assert!(
        plain.contains(&format!("0\tcapture\t{open}\tcaptured\tOpen idea")),
        "{plain}"
    );
    assert!(!plain.contains(&promoted) && !plain.contains(&discarded), "{plain}");
    let plain_all = stdout_of(dir, &["--plain", "tree", "--all"]);
    assert!(
        plain_all.contains(&format!("0\tcapture\t{promoted}\tpromoted\tPromoted idea")),
        "{plain_all}"
    );
    assert!(
        plain_all.contains(&format!("0\tcapture\t{discarded}\tdiscarded\tDiscarded idea")),
        "{plain_all}"
    );

    let human = stdout_of(dir, &["--color", "never", "tree"]);
    // The promoted capture's task prints first.
    assert_eq!(human.lines().collect::<Vec<_>>(), ["● Promoted idea", "Inbox: 1 open"]);
    let human_all = stdout_of(dir, &["--color", "never", "tree", "--all"]);
    assert_eq!(
        human_all.lines().collect::<Vec<_>>(),
        [
            "● Promoted idea",
            "Inbox",
            "├── ○ Open idea",
            "├── ✓ Promoted idea",
            "└── ✗ Discarded idea"
        ]
    );

    for closed in [&promoted, &discarded] {
        let rooted = run_json(dir, ["--json", "tree", closed].as_ref());
        let nodes = rooted["data"]["nodes"].as_array().expect("tree nodes");
        assert_eq!(nodes.len(), 1, "{rooted}");
        assert_eq!(nodes[0]["id"], closed.as_str());
    }
}

#[test]
fn tree_puts_specs_first_and_unplaced_records_under_sections() {
    let directory = initialized_directory();
    let dir = directory.path();
    // Created before the spec, so these records' IDs sort ahead of its ID.
    run_json(dir, ["--json", "release", "create", "v1"].as_ref());
    run_json(dir, ["--json", "note", "create", "Decision"].as_ref());
    create_capture(dir, "Idea", "Later");
    create_task(dir, "Loose", None);
    run_json(dir, ["--json", "spec", "create", "Feature"].as_ref());

    let human = stdout_of(dir, &["--color", "never", "tree"]);
    assert_eq!(
        human.lines().collect::<Vec<_>>(),
        ["○ Feature", "● Loose", "Inbox: 1 open · Notes: 1 · Releases: v1"]
    );
    let human_all = stdout_of(dir, &["--color", "never", "tree", "--all"]);
    assert_eq!(
        human_all.lines().collect::<Vec<_>>(),
        [
            "○ Feature",
            "● Loose",
            "Inbox",
            "└── ○ Idea",
            "Notes",
            "└── ○ Decision",
            "Releases",
            "└── ○ v1"
        ]
    );

    let expected = ["spec", "task", "capture", "note", "release"];
    let plain = stdout_of(dir, &["--plain", "tree"]);
    let plain_kinds = plain
        .lines()
        .map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            assert_eq!(fields.len(), 5, "{plain}");
            assert_eq!(fields[0], "0", "{plain}");
            fields[1]
        })
        .collect::<Vec<_>>();
    assert_eq!(plain_kinds, expected);

    let tree = run_json(dir, ["--json", "tree"].as_ref());
    assert_eq!(tree["format_version"], 1);
    let nodes = tree["data"]["nodes"].as_array().expect("tree nodes");
    let json_kinds = nodes
        .iter()
        .map(|node| node["kind"].as_str().expect("a node kind"))
        .collect::<Vec<_>>();
    assert_eq!(json_kinds, expected);
    for node in nodes {
        let mut keys = node
            .as_object()
            .expect("node object")
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        keys.sort();
        assert_eq!(keys, ["children", "id", "kind", "status", "title"], "{node}");
    }
}

#[test]
fn tree_folds_finished_phases_and_limits_depth() {
    let directory = initialized_directory();
    let dir = directory.path();
    let spec = run_json(dir, ["--json", "spec", "create", "Feature"].as_ref());
    let spec_id = spec["data"]["spec"]["id"].as_str().expect("spec ID").to_owned();
    let plan = run_json(
        dir,
        ["--json", "plan", "create", "Roadmap", "--spec", &spec_id].as_ref(),
    );
    let plan_id = plan["data"]["plan"]["id"].as_str().expect("plan ID").to_owned();
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
    let create_in = |title: &str, phase: &str| {
        let task = run_json(dir, ["--json", "task", "create", title, "--phase", phase].as_ref());
        task["data"]["task"]["id"].as_str().expect("task ID").to_owned()
    };
    for (title, position) in [("Milestone 1: First", "0"), ("Milestone 2: Second", "1")] {
        let phase = create_phase(title, position);
        let task = create_in("Shipped", &phase);
        run_json(
            dir,
            ["--json", "task", "complete", &task, "--evidence", "Verified"].as_ref(),
        );
        run_json(dir, ["--json", "plan", "phase", "complete", &phase].as_ref());
    }
    let open = create_phase("Milestone 3: Third", "2");
    let blocker = create_in("Blocker", &open);
    let blocked = create_in("Blocked", &open);
    block_start_task(dir, &blocked, &blocker);
    let blocker_short = short_id(dir, &blocker);

    let folded = stdout_of(dir, &["--color", "never", "tree"]);
    assert_eq!(
        folded.lines().collect::<Vec<_>>(),
        [
            "◐ Feature",
            "└── ◐ Roadmap",
            "    ├── ✓ Milestones 1–2  done · 2 tasks",
            "    └── ○ Milestone 3: Third  0/2",
            "        ├── ● Blocker",
            &format!("        └── ○ Blocked  blocked by {blocker_short}"),
        ]
    );
    let ascii_all = stdout_of(dir, &["--color", "never", "tree", "--ascii", "--all"]);
    assert!(ascii_all.is_ascii(), "{ascii_all}");
    assert_eq!(ascii_all.lines().nth(2), Some("    |-- + Milestone 1: First  1/1"));
    assert_eq!(ascii_all.lines().count(), 9, "{ascii_all}");
    let shallow = stdout_of(dir, &["--color", "never", "tree", "--depth", "3"]);
    assert_eq!(
        shallow.lines().collect::<Vec<_>>(),
        [
            "◐ Feature",
            "└── ◐ Roadmap",
            "    ├── ✓ Milestones 1–2  done · 2 tasks",
            "    └── ○ Milestone 3: Third  0/2"
        ]
    );
    let rooted = stdout_of(dir, &["--color", "never", "tree", &open, "--depth", "1"]);
    assert_eq!(rooted.trim_end(), "○ Milestone 3: Third  0/2");
    for flag in ["--ascii", "--all"] {
        assert_eq!(
            stdout_of(dir, &["--plain", "tree", flag, "--depth", "1"]),
            stdout_of(dir, &["--plain", "tree", "--all"]),
            "--plain prints every record whatever {flag} and --depth say"
        );
    }
    let refused = arcl_in(dir).args(["tree", "--depth", "0"]).output().expect("tree runs");
    assert!(!refused.status.success());
}

#[test]
fn tree_styles_print_full_and_outline_views() {
    let directory = initialized_directory();
    let dir = directory.path();
    let parent = create_task(dir, "Parent", None);
    let active = create_task(dir, "Active child", Some(&parent));
    create_task(dir, "Waiting child", Some(&parent));
    run_json(dir, ["--json", "task", "start", &active].as_ref());
    let (parent_short, active_short) = (short_id(dir, &parent), short_id(dir, &active));

    let outline = stdout_of(dir, &["--color", "never", "tree", "--style", "outline"]);
    assert!(outline.starts_with("STATUS  "), "{outline}");
    assert!(outline.is_ascii(), "no box characters or marks:\n{outline}");
    let in_progress = outline
        .lines()
        .filter(|line| line.contains("in_progress"))
        .collect::<Vec<_>>();
    assert_eq!(in_progress, [format!("in_progress  {active_short}  . Active child")]);
    assert!(
        outline
            .lines()
            .any(|line| line == format!("pending      {parent_short}  Parent")),
        "{outline}"
    );

    let full = stdout_of(dir, &["--color", "never", "tree", "--style", "full"]);
    let lines = full.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 3, "{full}");
    assert!(
        lines[0].starts_with("○ Parent") && lines[0].ends_with(&parent_short),
        "{full}"
    );
    assert!(
        lines[1].starts_with("├── ◐ Active child") && lines[1].ends_with(&active_short),
        "{full}"
    );
    let width = lines[0].width();
    assert!(lines.iter().all(|line| line.width() == width), "IDs align:\n{full}");
    let ascii = stdout_of(
        dir,
        &["--color", "never", "tree", "--style", "full", "--ascii", "--depth", "1"],
    );
    assert_eq!(ascii.trim_end(), format!("o Parent  {parent_short}"));

    for flag in ["--plain", "--json"] {
        assert_eq!(
            stdout_of(dir, &[flag, "tree", "--style", "outline"]),
            stdout_of(dir, &[flag, "tree"]),
            "{flag} ignores --style"
        );
    }
    let refused = arcl_in(dir)
        .args(["tree", "--style", "wide"])
        .output()
        .expect("tree runs");
    assert!(!refused.status.success());
}

#[test]
fn tree_colour_follows_the_terminal_no_color_and_the_flag() {
    let directory = initialized_directory();
    let dir = directory.path();
    create_task(dir, "Task", None);
    // `IGNORE_IS_TERMINAL` makes the colour check treat stdout as a terminal.
    // The other variables are every one the check reads that could force
    // colour on or off whatever the case sets.
    let tree = |args: &[&str], envs: &[(&str, &str)]| {
        let mut command = arcl_in(dir);
        for key in [
            "NO_COLOR",
            "FORCE_COLOR",
            "CLICOLOR_FORCE",
            "CLICOLOR",
            "IGNORE_IS_TERMINAL",
        ] {
            command.env_remove(key);
        }
        command.env("TERM", "xterm");
        for (key, value) in envs {
            command.env(key, value);
        }
        let output = command.args(args).output().expect("tree runs");
        assert_success(&output);
        String::from_utf8(output.stdout).expect("output is UTF-8")
    };
    let terminal = ("IGNORE_IS_TERMINAL", "1");
    let coloured = |output: &str| output.contains('\u{1b}');

    for style in ["folded", "full", "outline"] {
        let with = |flags: &[&'static str]| [flags, &["tree", "--style", style]].concat();
        assert!(coloured(&tree(&with(&[]), &[terminal])), "a terminal gets colour");
        assert!(!coloured(&tree(&with(&[]), &[])), "a pipe gets none");
        assert!(
            !coloured(&tree(&with(&[]), &[terminal, ("NO_COLOR", "1")])),
            "NO_COLOR turns it off"
        );
        assert!(!coloured(&tree(&with(&["--color", "never"]), &[terminal])));
        assert!(
            coloured(&tree(&with(&["--color", "always"]), &[])),
            "--color always colours a pipe"
        );
    }
}

#[test]
fn rooted_tree_prints_the_record_without_a_section_heading() {
    let directory = initialized_directory();
    let dir = directory.path();
    let release = run_json(dir, ["--json", "release", "create", "v1"].as_ref());
    let release_id = release["data"]["release"]["id"]
        .as_str()
        .expect("release ID")
        .to_owned();
    let capture_id = create_capture(dir, "Idea", "Later");

    for (id, line) in [(&release_id, "○ v1"), (&capture_id, "○ Idea")] {
        let human = stdout_of(dir, &["--color", "never", "tree", id]);
        assert_eq!(human.trim_end(), line, "no heading and no footer");
    }
}

#[test]
fn open_containers_show_started_when_a_task_below_them_started() {
    let directory = initialized_directory();
    let dir = directory.path();
    let spec = run_json(dir, ["--json", "spec", "create", "Feature"].as_ref());
    let spec_id = spec["data"]["spec"]["id"].as_str().expect("spec ID").to_owned();
    let create_plan = |title: &str| {
        let plan = run_json(dir, ["--json", "plan", "create", title, "--spec", &spec_id].as_ref());
        plan["data"]["plan"]["id"].as_str().expect("plan ID").to_owned()
    };
    let create_phase = |title: &str, plan: &str| {
        let phase = run_json(
            dir,
            ["--json", "plan", "phase", "create", title, "--plan", plan].as_ref(),
        );
        phase["data"]["phase"]["id"].as_str().expect("phase ID").to_owned()
    };
    let create_in = |title: &str, flag: &str, container: &str| {
        let task = run_json(dir, ["--json", "task", "create", title, flag, container].as_ref());
        task["data"]["task"]["id"].as_str().expect("task ID").to_owned()
    };
    let complete = |task: &str| {
        run_json(
            dir,
            ["--json", "task", "complete", task, "--evidence", "Verified"].as_ref(),
        );
    };

    let direct = create_plan("Direct");
    let started = create_phase("Started", &direct);
    complete(&create_in("Shipped", "--phase", &started));
    let idle = create_phase("Idle", &direct);
    let dropped = create_in("Dropped", "--phase", &idle);
    run_json(dir, ["--json", "task", "cancel", &dropped].as_ref());
    let parked = create_in("Parked", "--phase", &idle);
    run_json(dir, ["--json", "task", "park", &parked].as_ref());
    let closed = create_phase("Closed", &direct);
    complete(&create_in("Finished", "--phase", &closed));
    run_json(dir, ["--json", "plan", "phase", "complete", &closed].as_ref());

    let nested_plan = create_plan("Nested plan");
    let nested = create_phase("Nested", &nested_plan);
    let parent = create_in("Parent", "--phase", &nested);
    complete(&create_task(dir, "Child", Some(&parent)));

    let human = stdout_of(dir, &["--color", "never", "tree", "--all"]);
    let mark = |title: &str| tree_line(&human, title).0;
    assert_eq!(mark("Started"), '◐', "a completed task starts its phase");
    assert_eq!(mark("Direct"), '◐', "and the plan above it");
    assert_eq!(mark("Feature"), '◐', "and the spec above that");
    assert_eq!(mark("Nested"), '◐', "a completed subtask starts its phase");
    assert_eq!(mark("Nested plan"), '◐');
    assert_eq!(mark("Parent"), '○', "a task with subtasks is never ready itself");
    assert_eq!(mark("Idle"), '○', "cancelled and parked tasks do not start a phase");
    assert_eq!(mark("Closed"), '✓', "a completed phase stays completed");
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

/// Make `task` wait on `blocker`.
fn block_start_task(directory: &Path, task: &str, blocker: &str) {
    run_json(
        directory,
        ["--json", "dependency", "add", task, "--blocked-by", blocker].as_ref(),
    );
}

/// Run `task start` and return the JSON error it fails with.
fn refused_start(directory: &Path, task: &str) -> Value {
    let output = arcl_in(directory)
        .args(["--json", "task", "start", task])
        .output()
        .expect("start runs");
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    json(&output.stderr)
}

#[test]
fn start_refuses_a_blocked_task_with_its_reasons_and_writes_nothing() {
    let directory = initialized_directory();
    let blocker = create_task(directory.path(), "Blocker", None);
    let task = create_task(directory.path(), "Blocked", None);
    block_start_task(directory.path(), &task, &blocker);
    let before = run_json(directory.path(), ["--json", "task", "show", &task].as_ref());

    let error = refused_start(directory.path(), &task);
    assert_eq!(error["error"]["code"], "invalid_request");
    let message = error["error"]["message"].as_str().expect("error message");
    assert!(message.contains(&format!("cannot start task `{task}` while it is not ready")));
    assert!(message.contains(&format!("blocked by `{blocker}` (pending)")));
    assert!(message.contains("--force"));

    let after = run_json(directory.path(), ["--json", "task", "show", &task].as_ref());
    assert_eq!(after, before);
    assert_eq!(after["data"]["task"]["status"], "pending");
}

#[test]
fn start_force_starts_a_blocked_task() {
    let directory = initialized_directory();
    let blocker = create_task(directory.path(), "Blocker", None);
    let task = create_task(directory.path(), "Blocked", None);
    block_start_task(directory.path(), &task, &blocker);

    let started = run_json(directory.path(), ["--json", "task", "start", &task, "--force"].as_ref());
    assert_eq!(started["data"]["action"], "started");
    assert_eq!(started["data"]["task"]["status"], "in_progress");
}

#[test]
fn start_repeats_on_an_in_progress_task_with_or_without_force() {
    let directory = initialized_directory();
    let blocker = create_task(directory.path(), "Blocker", None);
    let task = create_task(directory.path(), "Blocked", None);
    block_start_task(directory.path(), &task, &blocker);
    run_json(directory.path(), ["--json", "task", "start", &task, "--force"].as_ref());

    for args in [
        ["--json", "task", "start", &task].as_ref(),
        ["--json", "task", "start", &task, "--force"].as_ref(),
    ] {
        let repeated = run_json(directory.path(), args);
        assert_eq!(repeated["data"]["task"]["status"], "in_progress");
    }
}

#[test]
fn start_starts_a_parent_with_children_without_force() {
    let directory = initialized_directory();
    let parent = create_task(directory.path(), "Parent", None);
    create_task(directory.path(), "Child", Some(&parent));

    let started = run_json(directory.path(), ["--json", "task", "start", &parent].as_ref());
    assert_eq!(started["data"]["task"]["status"], "in_progress");
}

#[test]
fn start_refuses_a_blocked_parent() {
    let directory = initialized_directory();
    let blocker = create_task(directory.path(), "Blocker", None);
    let parent = create_task(directory.path(), "Parent", None);
    create_task(directory.path(), "Child", Some(&parent));
    block_start_task(directory.path(), &parent, &blocker);

    let error = refused_start(directory.path(), &parent);
    let message = error["error"]["message"].as_str().expect("error message");
    assert!(message.contains(&format!("blocked by `{blocker}` (pending)")));
    assert!(!message.contains("child task"));
}

/// Assert that `task start` refuses `task` with `reason` and leaves it
/// pending, then that `--force` starts it.
fn assert_start_needs_force(directory: &Path, task: &str, reason: &str) {
    let error = refused_start(directory, task);
    let message = error["error"]["message"].as_str().expect("error message");
    assert!(message.contains(reason), "{message}");
    let shown = run_json(directory, ["--json", "task", "show", task].as_ref());
    assert_eq!(shown["data"]["task"]["status"], "pending");

    let started = run_json(directory, ["--json", "task", "start", task, "--force"].as_ref());
    assert_eq!(started["data"]["task"]["status"], "in_progress");
}

#[test]
fn start_refuses_a_task_under_a_parked_ancestor_unless_forced() {
    let directory = initialized_directory();
    let parent = create_task(directory.path(), "Parent", None);
    let child = create_task(directory.path(), "Child", Some(&parent));
    run_json(directory.path(), ["--json", "task", "park", &parent].as_ref());

    assert_start_needs_force(directory.path(), &child, &format!("ancestor `{parent}` is `parked`"));
}

#[test]
fn start_refuses_a_task_in_a_closed_plan_unless_forced() {
    let directory = initialized_directory();
    let dir = directory.path();
    let spec = run_json(dir, ["--json", "spec", "create", "Feature"].as_ref());
    let spec_id = spec["data"]["spec"]["id"].as_str().expect("spec ID");
    let plan = run_json(dir, ["--json", "plan", "create", "Build", "--spec", spec_id].as_ref());
    let plan_id = plan["data"]["plan"]["id"].as_str().expect("plan ID").to_owned();
    let task = run_json(dir, ["--json", "task", "create", "Task", "--plan", &plan_id].as_ref())["data"]["task"]["id"]
        .as_str()
        .expect("task ID")
        .to_owned();
    run_json(
        dir,
        ["--json", "plan", "cancel", &plan_id, "--allow-open-children"].as_ref(),
    );

    assert_start_needs_force(dir, &task, &format!("plan `{plan_id}` is `cancelled`"));
}

#[test]
fn start_blocked_ancestor_refused_unless_forced() {
    let directory = initialized_directory();
    let blocker = create_task(directory.path(), "Blocker", None);
    let parent = create_task(directory.path(), "Parent", None);
    let child = create_task(directory.path(), "Child", Some(&parent));
    block_start_task(directory.path(), &parent, &blocker);

    assert_start_needs_force(
        directory.path(),
        &child,
        &format!("ancestor `{parent}` is blocked by `{blocker}` (pending)"),
    );
}

#[test]
fn start_starts_a_child_its_parent_is_blocked_by_without_force() {
    let directory = initialized_directory();
    let parent = create_task(directory.path(), "Parent", None);
    let child = create_task(directory.path(), "Child", Some(&parent));
    // `dependency add` refuses this edge, so insert it directly.
    let database = rusqlite::Connection::open(directory.path().join(".arcl/arcl.db")).expect("database opens");
    database
        .execute(
            "INSERT INTO planning_task_dependencies (project_id, task_id, blocker_id)
             SELECT project_id, ?1, ?2 FROM planning_tasks WHERE id = ?1",
            [&parent, &child],
        )
        .expect("bad edge inserts");
    drop(database);

    let started = run_json(directory.path(), ["--json", "task", "start", &child].as_ref());
    assert_eq!(started["data"]["action"], "started");
    assert_eq!(started["data"]["task"]["status"], "in_progress");
}

/// Run a command that fails with a request error and return its JSON error.
fn refused(directory: &Path, args: &[&str]) -> Value {
    let output = arcl_in(directory).args(args).output().expect("command runs");
    assert_eq!(
        output.status.code(),
        Some(3),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    json(&output.stderr)
}

fn task_field(directory: &Path, task: &str, field: &str) -> Value {
    run_json(directory, ["--json", "task", "show", task].as_ref())["data"]["task"][field].clone()
}

#[test]
fn complete_with_subtasks_completes_nested_open_subtasks_in_tree_order() {
    let directory = initialized_directory();
    let dir = directory.path();
    let parent = create_task(dir, "Parent", None);
    let started = create_task(dir, "Started", Some(&parent));
    let grandchild = create_task(dir, "Grandchild", Some(&started));
    let pending = create_task(dir, "Pending", Some(&parent));
    run_json(dir, ["--json", "task", "start", &started].as_ref());

    let completed = run_json(
        dir,
        [
            "--json",
            "task",
            "complete",
            &parent,
            "--with-subtasks",
            "--evidence",
            "All done",
        ]
        .as_ref(),
    );
    assert_eq!(completed["data"]["action"], "completed");
    assert_eq!(completed["data"]["task"]["id"], parent.as_str());
    assert_eq!(completed["data"]["task"]["status"], "completed");
    assert_eq!(completed["data"]["task"]["evidence"], "All done");
    assert_eq!(
        completed["data"]["completed_subtasks"],
        serde_json::json!([started, grandchild, pending])
    );
    for task in [&started, &grandchild, &pending] {
        assert_eq!(task_field(dir, task, "status"), "completed");
        assert_eq!(task_field(dir, task, "evidence"), "");
    }
}

#[test]
fn complete_with_subtasks_names_subtasks_in_human_and_plain_output() {
    let directory = initialized_directory();
    let dir = directory.path();
    let parent = create_task(dir, "Parent", None);
    let child = create_task(dir, "Child", Some(&parent));
    let other = create_task(dir, "Other parent", None);
    let other_child = create_task(dir, "Other child", Some(&other));

    let human = arcl_in(dir)
        .args(["--color", "never", "task", "complete", &parent, "--with-subtasks"])
        .output()
        .expect("complete runs");
    assert!(human.status.success());
    assert_eq!(
        String::from_utf8_lossy(&human.stdout),
        format!("Completed task `{parent}`: Parent\nCompleted subtask `{child}`: Child\n")
    );

    let plain = arcl_in(dir)
        .args(["--plain", "task", "complete", &other, "--with-subtasks"])
        .output()
        .expect("complete runs");
    assert!(plain.status.success());
    assert_eq!(
        String::from_utf8_lossy(&plain.stdout),
        format!("{other}\nsubtask\t{other_child}\n")
    );
}

#[test]
fn complete_with_subtasks_leaves_completed_and_cancelled_subtasks_alone() {
    let directory = initialized_directory();
    let dir = directory.path();
    let parent = create_task(dir, "Parent", None);
    let done = create_task(dir, "Done", Some(&parent));
    let dropped = create_task(dir, "Dropped", Some(&parent));
    let open = create_task(dir, "Open", Some(&parent));
    run_json(
        dir,
        ["--json", "task", "complete", &done, "--evidence", "Earlier"].as_ref(),
    );
    run_json(dir, ["--json", "task", "cancel", &dropped].as_ref());

    let completed = run_json(
        dir,
        [
            "--json",
            "task",
            "complete",
            &parent,
            "--with-subtasks",
            "--evidence",
            "Parent",
        ]
        .as_ref(),
    );
    assert_eq!(completed["data"]["completed_subtasks"], serde_json::json!([open]));
    assert_eq!(task_field(dir, &done, "status"), "completed");
    assert_eq!(task_field(dir, &done, "evidence"), "Earlier");
    assert_eq!(task_field(dir, &dropped, "status"), "cancelled");
}

#[test]
fn complete_with_subtasks_refuses_a_parked_subtask_and_writes_nothing() {
    let directory = initialized_directory();
    let dir = directory.path();
    let parent = create_task(dir, "Parent", None);
    let parked = create_task(dir, "Parked", Some(&parent));
    let open = create_task(dir, "Open", Some(&parent));
    run_json(dir, ["--json", "task", "start", &parked].as_ref());
    run_json(dir, ["--json", "task", "park", &parked].as_ref());

    let error = refused(
        dir,
        [
            "--json",
            "task",
            "complete",
            &parent,
            "--with-subtasks",
            "--evidence",
            "No",
        ]
        .as_ref(),
    );
    let message = error["error"]["message"].as_str().expect("error message");
    assert!(message.contains(&format!("cannot complete task `{parent}` with its subtasks")));
    assert!(message.contains(&format!("subtask `{parked}` is parked")), "{message}");
    assert_eq!(task_field(dir, &parent, "status"), "pending");
    assert_eq!(task_field(dir, &parent, "evidence"), "");
    assert_eq!(task_field(dir, &parked, "status"), "parked");
    assert_eq!(task_field(dir, &open, "status"), "pending");
}

#[test]
fn complete_with_subtasks_refuses_an_outside_blocker_and_writes_nothing() {
    let directory = initialized_directory();
    let dir = directory.path();
    let outside = create_task(dir, "Outside", None);
    let ancestor_blocker = create_task(dir, "Ancestor blocker", None);
    let grandparent = create_task(dir, "Grandparent", None);
    let parent = create_task(dir, "Parent", Some(&grandparent));
    let child = create_task(dir, "Child", Some(&parent));
    block_start_task(dir, &child, &outside);
    block_start_task(dir, &grandparent, &ancestor_blocker);

    let error = refused(dir, ["--json", "task", "complete", &parent, "--with-subtasks"].as_ref());
    let message = error["error"]["message"].as_str().expect("error message");
    assert!(
        message.contains(&format!("`{child}` is blocked by `{outside}` (pending)")),
        "{message}"
    );
    assert!(
        message.contains(&format!("`{grandparent}` is blocked by `{ancestor_blocker}` (pending)")),
        "{message}"
    );
    assert_eq!(task_field(dir, &parent, "status"), "pending");
    assert_eq!(task_field(dir, &child, "status"), "pending");
}

#[test]
fn complete_with_subtasks_allows_a_blocker_inside_the_subtree() {
    let directory = initialized_directory();
    let dir = directory.path();
    let parent = create_task(dir, "Parent", None);
    let first = create_task(dir, "First", Some(&parent));
    let second = create_task(dir, "Second", Some(&parent));
    block_start_task(dir, &second, &first);

    let completed = run_json(dir, ["--json", "task", "complete", &parent, "--with-subtasks"].as_ref());
    assert_eq!(
        completed["data"]["completed_subtasks"],
        serde_json::json!([first, second])
    );
}

#[test]
fn complete_with_subtasks_allows_a_cancelled_blocker_inside_the_subtree() {
    let directory = initialized_directory();
    let dir = directory.path();
    let parent = create_task(dir, "Parent", None);
    let dropped = create_task(dir, "Dropped", Some(&parent));
    let blocked = create_task(dir, "Blocked", Some(&parent));
    block_start_task(dir, &blocked, &dropped);
    run_json(dir, ["--json", "task", "cancel", &dropped].as_ref());

    let completed = run_json(dir, ["--json", "task", "complete", &parent, "--with-subtasks"].as_ref());
    assert_eq!(completed["data"]["completed_subtasks"], serde_json::json!([blocked]));
    assert_eq!(task_field(dir, &dropped, "status"), "cancelled");
    assert_eq!(task_field(dir, &blocked, "status"), "completed");
    assert_eq!(task_field(dir, &parent, "status"), "completed");
}

#[test]
fn complete_with_subtasks_conflicts_with_allow_open_children() {
    let directory = initialized_directory();
    let dir = directory.path();
    let parent = create_task(dir, "Parent", None);
    let child = create_task(dir, "Child", Some(&parent));

    let output = arcl_in(dir)
        .args([
            "--json",
            "task",
            "complete",
            &parent,
            "--with-subtasks",
            "--allow-open-children",
        ])
        .output()
        .expect("complete runs");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(task_field(dir, &parent, "status"), "pending");
    assert_eq!(task_field(dir, &child, "status"), "pending");
}

#[test]
fn complete_with_subtasks_leaves_allow_open_children_unchanged() {
    let directory = initialized_directory();
    let dir = directory.path();
    let parent = create_task(dir, "Parent", None);
    let child = create_task(dir, "Child", Some(&parent));

    let error = refused(dir, ["--json", "task", "complete", &parent].as_ref());
    let message = error["error"]["message"].as_str().expect("error message");
    assert!(message.contains("--allow-open-children"), "{message}");

    let completed = run_json(
        dir,
        ["--json", "task", "complete", &parent, "--allow-open-children"].as_ref(),
    );
    assert_eq!(completed["data"]["task"]["status"], "completed");
    assert_eq!(completed["data"]["completed_subtasks"], serde_json::json!([]));
    assert_eq!(task_field(dir, &child, "status"), "pending");
}

/// Run `task complete --json` on `task` with `extra` arguments and return the
/// `unblocked` list.
fn unblocked_by(directory: &Path, task: &str, extra: &[&str]) -> Value {
    let mut args = vec!["--json", "task", "complete", task];
    args.extend(extra);
    run_json(directory, &args)["data"]["unblocked"].clone()
}

#[test]
fn unblocked_lists_a_direct_dependent() {
    let directory = initialized_directory();
    let dir = directory.path();
    let blocker = create_task(dir, "Blocker", None);
    let dependent = create_task(dir, "Dependent", None);
    block_start_task(dir, &dependent, &blocker);

    assert_eq!(unblocked_by(dir, &blocker, &[]), serde_json::json!([dependent]));
}

#[test]
fn unblocked_lists_the_child_of_a_dependent() {
    let directory = initialized_directory();
    let dir = directory.path();
    let blocker = create_task(dir, "Blocker", None);
    let parent = create_task(dir, "Blocked parent", None);
    let child = create_task(dir, "Child", Some(&parent));
    block_start_task(dir, &parent, &blocker);

    assert_eq!(unblocked_by(dir, &blocker, &[]), serde_json::json!([child]));
}

#[test]
fn unblocked_leaves_out_a_dependent_with_another_open_blocker() {
    let directory = initialized_directory();
    let dir = directory.path();
    let blocker = create_task(dir, "Blocker", None);
    let other = create_task(dir, "Other blocker", None);
    let dependent = create_task(dir, "Dependent", None);
    block_start_task(dir, &dependent, &blocker);
    block_start_task(dir, &dependent, &other);

    assert_eq!(unblocked_by(dir, &blocker, &[]), serde_json::json!([]));
    assert_eq!(unblocked_by(dir, &other, &[]), serde_json::json!([dependent]));
}

#[test]
fn unblocked_lists_a_dependent_of_a_subtask_completed_with_subtasks() {
    let directory = initialized_directory();
    let dir = directory.path();
    let parent = create_task(dir, "Parent", None);
    let subtask = create_task(dir, "Subtask", Some(&parent));
    let dependent = create_task(dir, "Dependent", None);
    block_start_task(dir, &dependent, &subtask);

    assert_eq!(
        unblocked_by(dir, &parent, &["--with-subtasks"]),
        serde_json::json!([dependent])
    );
}

#[test]
fn unblocked_leaves_out_started_and_parked_dependents() {
    let directory = initialized_directory();
    let dir = directory.path();
    let blocker = create_task(dir, "Blocker", None);
    let started = create_task(dir, "Started early", None);
    let parked = create_task(dir, "Parked", None);
    block_start_task(dir, &started, &blocker);
    block_start_task(dir, &parked, &blocker);
    run_json(dir, ["--json", "task", "start", &started, "--force"].as_ref());
    run_json(dir, ["--json", "task", "park", &parked].as_ref());

    assert_eq!(unblocked_by(dir, &blocker, &[]), serde_json::json!([]));
}

#[test]
fn unblocked_lists_a_dependent_of_a_task_completed_with_open_children() {
    let directory = initialized_directory();
    let dir = directory.path();
    let parent = create_task(dir, "Parent", None);
    create_task(dir, "Open child", Some(&parent));
    let dependent = create_task(dir, "Dependent", None);
    block_start_task(dir, &dependent, &parent);

    assert_eq!(
        unblocked_by(dir, &parent, &["--allow-open-children"]),
        serde_json::json!([dependent])
    );
}

#[test]
fn unblocked_is_empty_when_nothing_was_waiting() {
    let directory = initialized_directory();
    let dir = directory.path();
    let lone = create_task(dir, "Lone", None);
    create_task(dir, "Already ready", None);

    assert_eq!(unblocked_by(dir, &lone, &[]), serde_json::json!([]));
}

#[test]
fn unblocked_tasks_are_named_in_human_and_plain_output() {
    let directory = initialized_directory();
    let dir = directory.path();
    let first = create_task(dir, "First", None);
    let second = create_task(dir, "Second", None);
    let dependent = create_task(dir, "Dependent", None);
    let last = create_task(dir, "Last", None);
    block_start_task(dir, &dependent, &first);
    block_start_task(dir, &last, &second);

    let human = arcl_in(dir)
        .args(["--color", "never", "task", "complete", &first])
        .output()
        .expect("complete runs");
    assert!(human.status.success());
    assert_eq!(
        String::from_utf8_lossy(&human.stdout),
        format!("Completed task `{first}`: First\nUnblocked task `{dependent}`: Dependent\n")
    );

    let plain = arcl_in(dir)
        .args(["--plain", "task", "complete", &second])
        .output()
        .expect("complete runs");
    assert!(plain.status.success());
    assert_eq!(
        String::from_utf8_lossy(&plain.stdout),
        format!("{second}\nunblocked\t{last}\n")
    );
}

fn ready_ids(directory: &Path) -> Vec<String> {
    run_json(directory, ["--json", "ready"].as_ref())["data"]["tasks"]
        .as_array()
        .expect("ready tasks")
        .iter()
        .map(|task| task["id"].as_str().expect("ready task ID").to_owned())
        .collect()
}

fn readiness(directory: &Path, task_id: &str) -> Value {
    run_json(directory, ["--json", "explain", task_id].as_ref())["data"]["readiness"].clone()
}

/// The `message` of each readiness reason, in order.
fn reason_messages(readiness: &Value) -> Vec<String> {
    readiness["reasons"]
        .as_array()
        .expect("reason array")
        .iter()
        .map(|reason| reason["message"].as_str().expect("reason message").to_owned())
        .collect()
}

#[test]
fn readiness_inherits_a_blocker_through_two_levels_of_parents() {
    let directory = initialized_directory();
    let blocker = create_task(directory.path(), "Blocker", None);
    let grandparent = create_task(directory.path(), "Grandparent", None);
    let parent = create_task(directory.path(), "Parent", Some(&grandparent));
    let leaf = create_task(directory.path(), "Leaf", Some(&parent));
    run_json(
        directory.path(),
        ["--json", "dependency", "add", &grandparent, "--blocked-by", &blocker].as_ref(),
    );

    let readiness = readiness(directory.path(), &leaf);
    assert_eq!(readiness["ready"], false);
    assert_eq!(readiness["blocked"], true);
    assert_eq!(
        reason_messages(&readiness),
        vec![format!("ancestor `{grandparent}` is blocked by `{blocker}` (pending)")]
    );
    assert_eq!(ready_ids(directory.path()), vec![blocker]);

    let explained = arcl_in(directory.path())
        .args(["--color", "never", "explain", &leaf])
        .output()
        .expect("explain runs");
    assert!(explained.status.success());
    let short_grandparent = short_id(directory.path(), &grandparent);
    assert!(
        String::from_utf8_lossy(&explained.stdout)
            .contains(&format!("Reason: ancestor `{short_grandparent}` is blocked by"))
    );
}

#[test]
fn readiness_asks_to_replace_a_cancelled_direct_blocker() {
    let directory = initialized_directory();
    let blocker = create_task(directory.path(), "Blocker", None);
    let task = create_task(directory.path(), "Task", None);
    run_json(
        directory.path(),
        ["--json", "dependency", "add", &task, "--blocked-by", &blocker].as_ref(),
    );
    run_json(directory.path(), ["--json", "task", "cancel", &blocker].as_ref());

    let readiness = readiness(directory.path(), &task);
    assert_eq!(readiness["blocked"], true);
    assert_eq!(
        reason_messages(&readiness),
        vec![format!(
            "blocked by cancelled `{blocker}`; remove or replace the dependency"
        )]
    );
    assert!(ready_ids(directory.path()).is_empty());
}

#[test]
fn readiness_asks_to_replace_a_cancelled_inherited_blocker() {
    let directory = initialized_directory();
    let blocker = create_task(directory.path(), "Blocker", None);
    let parent = create_task(directory.path(), "Parent", None);
    let leaf = create_task(directory.path(), "Leaf", Some(&parent));
    run_json(
        directory.path(),
        ["--json", "dependency", "add", &parent, "--blocked-by", &blocker].as_ref(),
    );
    run_json(directory.path(), ["--json", "task", "cancel", &blocker].as_ref());

    let readiness = readiness(directory.path(), &leaf);
    assert_eq!(readiness["blocked"], true);
    assert_eq!(
        reason_messages(&readiness),
        vec![format!(
            "ancestor `{parent}` is blocked by cancelled `{blocker}`; remove or replace the dependency"
        )]
    );
    assert!(ready_ids(directory.path()).is_empty());
}

#[test]
fn readiness_returns_when_the_ancestor_blocker_completes() {
    let directory = initialized_directory();
    let blocker = create_task(directory.path(), "Blocker", None);
    let parent = create_task(directory.path(), "Parent", None);
    let leaf = create_task(directory.path(), "Leaf", Some(&parent));
    run_json(
        directory.path(),
        ["--json", "dependency", "add", &parent, "--blocked-by", &blocker].as_ref(),
    );
    assert!(!ready_ids(directory.path()).contains(&leaf));

    run_json(directory.path(), ["--json", "task", "complete", &blocker].as_ref());

    let readiness = readiness(directory.path(), &leaf);
    assert_eq!(readiness["ready"], true);
    assert_eq!(readiness["blocked"], false);
    assert_eq!(ready_ids(directory.path()), vec![leaf]);
}

fn stdout_of(directory: &Path, args: &[&str]) -> String {
    let output = arcl_in(directory).args(args).output().expect("arcl runs");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

/// The status mark and the text after it on the human `arcl tree` line
/// whose title is `title`, with the box-drawing prefix removed.
fn tree_line(tree: &str, title: &str) -> (char, String) {
    tree.lines()
        .find_map(|line| {
            let record = line.trim_start_matches(['│', '├', '└', '─', ' ']);
            let mut chars = record.chars();
            let mark = chars.next()?;
            let text = chars.as_str().strip_prefix(' ')?;
            (text.split("  ").next() == Some(title)).then(|| (mark, text.to_owned()))
        })
        .unwrap_or_else(|| panic!("a tree line for {title} in:\n{tree}"))
}

#[test]
fn explain_and_context_list_a_blocker_inherited_through_a_grandparent() {
    let directory = initialized_directory();
    let blocker = create_task(directory.path(), "Blocker", None);
    let grandparent = create_task(directory.path(), "Grandparent", None);
    let parent = create_task(directory.path(), "Parent", Some(&grandparent));
    let leaf = create_task(directory.path(), "Leaf", Some(&parent));
    run_json(
        directory.path(),
        ["--json", "dependency", "add", &grandparent, "--blocked-by", &blocker].as_ref(),
    );

    let explained = run_json(directory.path(), ["--json", "explain", &leaf].as_ref());
    assert_eq!(explained["data"]["blockers"], serde_json::json!([]));
    assert_eq!(
        explained["data"]["inherited_blockers"][0]["task"]["id"],
        blocker.as_str()
    );
    assert_eq!(
        explained["data"]["inherited_blockers"][0]["ancestor"],
        grandparent.as_str()
    );
    assert_eq!(
        reason_messages(&explained["data"]["readiness"]),
        vec![format!("ancestor `{grandparent}` is blocked by `{blocker}` (pending)")]
    );

    // Human output shows short IDs, as `show` does; plain keeps full IDs.
    let (short_blocker, short_grandparent) = (
        short_id(directory.path(), &blocker),
        short_id(directory.path(), &grandparent),
    );
    let human = stdout_of(directory.path(), &["--color", "never", "explain", &leaf]);
    assert!(
        human.contains(&format!("Blockers: {short_blocker} (via `{short_grandparent}`)")),
        "{human}"
    );
    let plain = stdout_of(directory.path(), &["--plain", "explain", &leaf]);
    assert!(plain.contains("blockers\t\n"), "{plain}");
    assert!(
        plain.contains(&format!("inherited\t{blocker}\t{grandparent}")),
        "{plain}"
    );

    run_json(
        directory.path(),
        ["--json", "task", "complete", &blocker, "--evidence", "Shipped"].as_ref(),
    );
    let plain = stdout_of(directory.path(), &["--plain", "context", &leaf]);
    assert!(!plain.contains("\nblocker\t"), "{plain}");
    assert!(
        plain.contains(&format!("inherited_blocker\t{blocker}\tcompleted\t{grandparent}")),
        "{plain}"
    );
    let human = stdout_of(directory.path(), &["--color", "never", "context", &leaf]);
    assert!(
        human.contains(&format!("Blocker: {blocker} [completed] via `{grandparent}`")),
        "{human}"
    );

    let context = run_json(directory.path(), ["--json", "context", &leaf].as_ref());
    assert_eq!(context["data"]["blockers"], serde_json::json!([]));
    assert_eq!(
        context["data"]["completion_evidence"][0]["task"]["id"],
        blocker.as_str()
    );
    assert_eq!(context["data"]["completion_evidence"][0]["evidence"], "Shipped");
    assert_eq!(
        context["data"]["completion_evidence"][0]["ancestor"],
        grandparent.as_str()
    );
}

#[test]
fn explain_json_omits_the_ancestor_of_a_direct_blocker() {
    let directory = initialized_directory();
    let blocker = create_task(directory.path(), "Blocker", None);
    let task = create_task(directory.path(), "Task", None);
    run_json(
        directory.path(),
        ["--json", "dependency", "add", &task, "--blocked-by", &blocker].as_ref(),
    );

    let explained = run_json(directory.path(), ["--json", "explain", &task].as_ref());
    assert_eq!(explained["data"]["blockers"][0]["task"]["id"], blocker.as_str());
    assert!(explained["data"]["blockers"][0].get("ancestor").is_none());
    assert_eq!(explained["data"]["inherited_blockers"], serde_json::json!([]));

    let plain = stdout_of(directory.path(), &["--plain", "explain", &task]);
    assert!(plain.contains(&format!("blockers\t{blocker}\n")), "{plain}");
    assert!(!plain.contains("inherited\t"), "{plain}");
    let plain = stdout_of(directory.path(), &["--plain", "context", &task]);
    assert!(plain.contains(&format!("\nblocker\t{blocker}\tpending\n")), "{plain}");
}

#[test]
fn a_parent_blocked_by_its_own_child_leaves_the_child_ready() {
    let directory = initialized_directory();
    let parent = create_task(directory.path(), "Parent", None);
    let leaf = create_task(directory.path(), "Leaf", Some(&parent));
    // `dependency add` refuses this edge, so insert it directly.
    let database = rusqlite::Connection::open(directory.path().join(".arcl/arcl.db")).expect("database opens");
    database
        .execute(
            "INSERT INTO planning_task_dependencies (project_id, task_id, blocker_id)
             SELECT project_id, ?1, ?2 FROM planning_tasks WHERE id = ?1",
            [&parent, &leaf],
        )
        .expect("bad edge inserts");
    drop(database);

    assert_eq!(ready_ids(directory.path()), vec![leaf.clone()]);
    let explained = run_json(directory.path(), ["--json", "explain", &leaf].as_ref());
    assert_eq!(explained["data"]["readiness"]["ready"], true);
    assert_eq!(explained["data"]["readiness"]["reasons"], serde_json::json!([]));
    assert_eq!(explained["data"]["inherited_blockers"], serde_json::json!([]));
    let human = stdout_of(directory.path(), &["--color", "never", "explain", &leaf]);
    assert!(human.contains("Blockers: \n"), "{human}");
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

/// Store blocker edges without the validator, as a database edited by hand
/// can hold edges that `dependency add` refuses.
fn insert_raw_edges(directory: &Path, edges: &[(&str, &str)]) {
    let database = rusqlite::Connection::open(directory.join(".arcl/arcl.db")).expect("database opens");
    for (task, blocker) in edges {
        database
            .execute(
                "INSERT INTO planning_task_dependencies (project_id, task_id, blocker_id)
                 SELECT project_id, ?1, ?2 FROM planning_tasks WHERE id = ?1",
                [task, blocker],
            )
            .expect("bad edge inserts");
    }
}

/// Run `arcl check`, which must fail, and return its JSON errors.
fn failed_check_errors(directory: &Path) -> Vec<String> {
    let output = arcl_in(directory)
        .args(["--json", "check"])
        .output()
        .expect("command runs");
    assert!(
        !output.status.success(),
        "check passed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let report = json(&output.stdout);
    assert_eq!(report["data"]["valid"], false);
    report["data"]["errors"]
        .as_array()
        .expect("error array")
        .iter()
        .map(|error| error.as_str().expect("error text").to_owned())
        .collect()
}

#[test]
fn invalid_edge_cycle_is_reported_by_check_and_explain() {
    let directory = initialized_directory();
    let path = directory.path();
    let (a, b) = (create_task(path, "A", None), create_task(path, "B", None));
    insert_raw_edges(path, &[(&a, &b), (&b, &a)]);
    // The report starts at the smallest task ID, whatever the edge order.
    let (low, high) = if a < b { (&a, &b) } else { (&b, &a) };
    let cycle = format!("blocker cycle: {low} -> {high} -> {low}; each task is blocked by the next");

    assert_eq!(failed_check_errors(path), vec![cycle.clone()]);
    for task in [&a, &b] {
        let readiness = run_json(path, ["--json", "task", "explain", task].as_ref())["data"]["readiness"].clone();
        assert_eq!(readiness["ready"], false);
        assert!(reason_messages(&readiness).contains(&cycle), "{readiness}");
    }
}

#[test]
fn invalid_self_loop_is_reported_by_check_and_explain_and_removes() {
    let directory = initialized_directory();
    let path = directory.path();
    let (a, other) = (create_task(path, "A", None), create_task(path, "Other", None));
    insert_raw_edges(path, &[(&a, &a)]);
    let cycle = format!("blocker cycle: {a} -> {a}; each task is blocked by the next");

    assert_eq!(failed_check_errors(path), vec![cycle.clone()]);
    let explained = readiness(path, &a);
    assert_eq!(explained["ready"], false);
    assert!(reason_messages(&explained).contains(&cycle), "{explained}");
    assert_eq!(ready_ids(path), vec![other]);
    assert_eq!(dependency_pairs(path), vec![(a.clone(), a.clone())]);

    run_json(
        path,
        ["--json", "dependency", "remove", &a, "--blocked-by", &a].as_ref(),
    );
    assert!(dependency_pairs(path).is_empty());
    assert_eq!(readiness(path, &a)["ready"], true);
}

#[test]
fn invalid_edge_to_an_ancestor_is_reported_by_check_and_explain() {
    let directory = initialized_directory();
    let path = directory.path();
    let root = create_task(path, "Root", None);
    let child = create_task(path, "Child", Some(&root));
    insert_raw_edges(path, &[(&child, &root)]);
    let ancestry = format!("task `{child}` is blocked by `{root}`, its ancestor; parent chain: {child} -> {root}");

    assert_eq!(failed_check_errors(path), vec![ancestry.clone()]);
    let readiness = run_json(path, ["--json", "task", "explain", &child].as_ref())["data"]["readiness"].clone();
    assert_eq!(readiness["ready"], false);
    assert!(reason_messages(&readiness).contains(&ancestry), "{readiness}");
    // Human reasons shorten every ID in the message, bare ones included.
    let (short_child, short_root) = (short_id(path, &child), short_id(path, &root));
    let human = stdout_of(path, &["--color", "never", "task", "explain", &child]);
    assert!(
        human.contains(&format!(
            "task `{short_child}` is blocked by `{short_root}`, its ancestor; parent chain: {short_child} -> {short_root}"
        )),
        "{human}"
    );
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

/// Create a spec and a plan under it, and return the plan ID.
fn create_plan(directory: &Path) -> String {
    let spec = run_json(directory, ["--json", "spec", "create", "Feature"].as_ref());
    let spec_id = spec["data"]["spec"]["id"].as_str().expect("spec ID");
    let plan = run_json(
        directory,
        ["--json", "plan", "create", "Build feature", "--spec", spec_id].as_ref(),
    );
    plan["data"]["plan"]["id"].as_str().expect("plan ID").to_owned()
}

/// Plan TOML for phase `work`, where task P holds A, A holds C, and B stands
/// alone. `blocked_by` pairs a task key with the path of a blocker.
fn nested_plan(blocked_by: &[(&str, &str)]) -> String {
    let blockers = |key: &str| {
        let paths: Vec<String> = blocked_by
            .iter()
            .filter(|(task, _)| *task == key)
            .map(|(_, blocker)| format!("\"{blocker}\""))
            .collect();
        format!("blocked-by = [{}]\n", paths.join(", "))
    };
    format!(
        "format-version = 1\n\n[[phases]]\nkey = \"work\"\ntitle = \"Work\"\nposition = 0\n\n\
         [[phases.tasks]]\nkey = \"p\"\ntitle = \"P\"\nposition = 0\n{}\n\
         [[phases.tasks.subtasks]]\nkey = \"a\"\ntitle = \"A\"\nposition = 0\n{}\n\
         [[phases.tasks.subtasks.subtasks]]\nkey = \"c\"\ntitle = \"C\"\nposition = 0\n{}\n\
         [[phases.tasks]]\nkey = \"b\"\ntitle = \"B\"\nposition = 1\n{}",
        blockers("p"),
        blockers("a"),
        blockers("c"),
        blockers("b")
    )
}

/// The arguments that run `plan <verb>` with `input` written to a file.
fn plan_args(directory: &Path, verb: &str, plan_id: &str, input: &str) -> Vec<String> {
    let path = directory.join("plan.toml");
    fs::write(&path, input).expect("plan input writes");
    [
        "--json",
        "plan",
        verb,
        plan_id,
        "--file",
        path.to_str().expect("plan path is UTF-8"),
    ]
    .map(str::to_owned)
    .to_vec()
}

/// Apply `input` to the plan and map each task key to its task ID.
fn apply_nested_plan(directory: &Path, plan_id: &str, input: &str) -> std::collections::HashMap<String, String> {
    let args = plan_args(directory, "apply", plan_id, input);
    let applied = run_json(directory, &args.iter().map(String::as_str).collect::<Vec<_>>());
    applied["data"]["tasks"]
        .as_array()
        .expect("task array")
        .iter()
        .map(|task| {
            let path = task["plan_key"].as_str().expect("plan key");
            let key = path.rsplit('/').next().expect("task key");
            (key.to_owned(), task["id"].as_str().expect("task ID").to_owned())
        })
        .collect()
}

/// Run a plan command that must be refused and return its error message.
fn refused_plan(directory: &Path, verb: &str, plan_id: &str, input: &str) -> String {
    let args = plan_args(directory, verb, plan_id, input);
    refused_message(directory, &args.iter().map(String::as_str).collect::<Vec<_>>())
}

/// Add `blocker` to the workspace record of `task`, then return the message
/// `workspace import` refuses it with.
fn refused_import(directory: &Path, task: &str, blocker: &str) -> String {
    let record = directory.join(format!(".arcl/workspace/tasks/{task}.md"));
    let content = fs::read_to_string(&record).expect("record reads");
    assert!(!content.contains("blocked-by"), "record: {content}");
    let edited = content.replacen("\nstatus = ", &format!("\nblocked-by = [\"{blocker}\"]\nstatus = "), 1);
    fs::write(&record, edited).expect("record writes");
    refused_message(directory, ["--json", "workspace", "import"].as_ref())
}

/// The parent ID of `task`, or `None` for a top-level task.
fn parent_of(directory: &Path, task: &str) -> Option<String> {
    let shown = run_json(directory, ["--json", "task", "show", task].as_ref());
    shown["data"]["task"]["parent_id"].as_str().map(str::to_owned)
}

#[test]
fn dependency_edge_plan_cycle_is_refused() {
    let directory = initialized_directory();
    let path = directory.path();
    let plan_id = create_plan(path);
    let input = nested_plan(&[("p", "work/b"), ("b", "work/p")]);

    for verb in ["check", "apply"] {
        let message = refused_plan(path, verb, &plan_id, &input);
        let named = ["work/p -> work/b -> work/p", "work/b -> work/p -> work/b"];
        assert!(
            named
                .iter()
                .any(|path| message.contains(&format!("blocker edges would form a cycle: {path};"))),
            "message: {message}"
        );
        assert!(!message.contains("arcl-t-"), "message: {message}");
    }
    let tasks = run_json(path, ["--json", "task", "list"].as_ref());
    assert!(tasks["data"]["tasks"].as_array().expect("task array").is_empty());
    assert!(dependency_pairs(path).is_empty());
}

#[test]
fn dependency_edge_plan_names_tasks_it_creates_by_plan_path() {
    let directory = initialized_directory();
    let path = directory.path();
    let plan_id = create_plan(path);
    let input = nested_plan(&[("c", "work/p")]);

    for verb in ["check", "apply"] {
        let message = refused_plan(path, verb, &plan_id, &input);
        assert!(
            message.ends_with(
                "task `work/p/a/c` cannot be blocked by `work/p`, its ancestor; \
                 parent chain: work/p/a/c -> work/p/a -> work/p"
            ),
            "message: {message}"
        );
    }
}

#[test]
fn dependency_edge_plan_ancestor_is_refused() {
    let directory = initialized_directory();
    let path = directory.path();
    let plan_id = create_plan(path);
    let ids = apply_nested_plan(path, &plan_id, &nested_plan(&[]));

    let message = refused_plan(path, "apply", &plan_id, &nested_plan(&[("c", "work/p")]));
    assert!(
        message.ends_with(&format!(
            "task `{}` cannot be blocked by `{}`, its ancestor; parent chain: {} -> {} -> {}",
            ids["c"], ids["p"], ids["c"], ids["a"], ids["p"]
        )),
        "message: {message}"
    );
    assert!(dependency_pairs(path).is_empty());
}

#[test]
fn dependency_edge_import_descendant_is_refused() {
    let directory = tempdir().expect("temporary directory can be created");
    let path = directory.path();
    run_json(path, ["--json", "init", "--workspace"].as_ref());
    let root = create_task(path, "Root", None);
    let child = create_task(path, "Child", Some(&root));

    let message = refused_import(path, &root, &child);
    assert!(
        message.ends_with(&format!(
            "workspace file `tasks/{root}.md`: relationship `blocked-by`: task `{root}` cannot be blocked by \
             `{child}`, its descendant; parent chain: {child} -> {root}"
        )),
        "message: {message}"
    );
}

#[test]
fn dependency_edge_reparent_through_task_update_names_the_chain() {
    let directory = initialized_directory();
    let path = directory.path();
    let p = create_task(path, "P", None);
    let a = create_task(path, "A", None);
    let c = create_task(path, "C", Some(&a));
    run_json(path, ["--json", "dependency", "add", &c, "--blocked-by", &p].as_ref());

    let message = refused_message(path, ["--json", "task", "update", &a, "--parent", &p].as_ref());
    assert!(
        message.ends_with(&format!("its ancestor; parent chain: {c} -> {a} -> {p}")),
        "message: {message}"
    );
    assert_eq!(parent_of(path, &a), None);
}

#[test]
fn dependency_edge_reparent_through_plan_apply_names_the_chain() {
    let directory = initialized_directory();
    let path = directory.path();
    let plan_id = create_plan(path);
    let input = nested_plan(&[]);
    let ids = apply_nested_plan(path, &plan_id, &input);
    let (p, a, c) = (&ids["p"], &ids["a"], &ids["c"]);
    run_json(path, ["--json", "task", "update", a, "--no-parent"].as_ref());
    run_json(path, ["--json", "dependency", "add", c, "--blocked-by", p].as_ref());

    let message = refused_plan(path, "apply", &plan_id, &input);
    assert!(
        message.ends_with(&format!("its ancestor; parent chain: {c} -> {a} -> {p}")),
        "message: {message}"
    );
    assert_eq!(parent_of(path, a), None);
}

#[test]
fn dependency_edge_existing_violation_does_not_block_plan_apply_or_reparent() {
    let directory = initialized_directory();
    let path = directory.path();
    let plan_id = create_plan(path);
    let ids = apply_nested_plan(path, &plan_id, &nested_plan(&[]));
    let (p, a, b, c) = (&ids["p"], &ids["a"], &ids["b"], &ids["c"]);
    let database = rusqlite::Connection::open(path.join(".arcl/arcl.db")).expect("database opens");
    for (task, blocker) in [(c, a), (p, b), (b, p)] {
        database
            .execute(
                "INSERT INTO planning_task_dependencies (project_id, task_id, blocker_id)
                 SELECT project_id, ?1, ?2 FROM planning_tasks WHERE id = ?1",
                [task, blocker],
            )
            .expect("bad edge inserts");
    }
    drop(database);

    let mut input = nested_plan(&[("p", "work/b"), ("b", "work/p")]);
    input.push_str("\n[[phases.tasks]]\nkey = \"q\"\ntitle = \"Q\"\nposition = 2\nblocked-by = [\"work/p\"]\n");
    let ids = apply_nested_plan(path, &plan_id, &input);
    run_json(path, ["--json", "task", "update", a, "--parent", &ids["q"]].as_ref());
    assert_eq!(parent_of(path, a).as_deref(), Some(ids["q"].as_str()));
    assert_eq!(dependency_pairs(path).len(), 4);
}

#[test]
fn dependency_edge_same_refusal_from_every_write_path() {
    // Each case names the blocked task and its blocker, and a valid edge the
    // graph already holds. An empty edge means none. "low" and "high" are
    // P and B ordered by ID, so every path reports the cycle from the same task.
    let cases = [
        ("cycle", ("low", "high"), ("high", "low")),
        ("ancestor", ("c", "p"), ("", "")),
        ("descendant", ("p", "a"), ("", "")),
    ];
    for (name, (task, blocker), (held_task, held_blocker)) in cases {
        let directory = tempdir().expect("temporary directory can be created");
        let path = directory.path();
        run_json(path, ["--json", "init", "--workspace"].as_ref());
        let plan_id = create_plan(path);
        let mut ids = apply_nested_plan(path, &plan_id, &nested_plan(&[]));
        let (low, high) = if ids["p"] < ids["b"] { ("p", "b") } else { ("b", "p") };
        let key = |name: &'static str| -> &'static str {
            match name {
                "low" => low,
                "high" => high,
                other => other,
            }
        };
        let task_path = |key: &str| match key {
            "p" | "b" => format!("work/{key}"),
            "a" => "work/p/a".to_owned(),
            _ => "work/p/a/c".to_owned(),
        };
        let (task, blocker) = (key(task), key(blocker));
        let mut held = Vec::new();
        if !held_task.is_empty() {
            held.push((key(held_task), task_path(key(held_blocker))));
        }
        let held_refs: Vec<(&str, &str)> = held.iter().map(|(task, path)| (*task, path.as_str())).collect();
        ids = apply_nested_plan(path, &plan_id, &nested_plan(&held_refs));
        let before = dependency_pairs(path);

        let cli = refused_message(
            path,
            ["--json", "dependency", "add", &ids[task], "--blocked-by", &ids[blocker]].as_ref(),
        );
        let (_, refusal) = cli.split_once(" is invalid: ").expect("refusal follows the prefix");

        let mut planned = held_refs.clone();
        let blocker_path = task_path(blocker);
        planned.push((task, &blocker_path));
        let plan = refused_plan(path, "apply", &plan_id, &nested_plan(&planned));
        assert_eq!(plan, cli, "case {name}");
        assert_eq!(dependency_pairs(path), before, "case {name}");

        let import = refused_import(path, &ids[task], &ids[blocker]);
        assert!(
            import.ends_with(&format!(
                "workspace file `tasks/{}.md`: relationship `blocked-by`: {refusal}",
                ids[task]
            )),
            "case {name}: {import}"
        );
    }
}

/// Store the edge `task` blocked by `blocker` straight into SQLite, the way
/// an edge saved before the blocker rules existed would be.
fn store_raw_edge(directory: &Path, task: &str, blocker: &str) {
    let database = rusqlite::Connection::open(directory.join(".arcl/arcl.db")).expect("database opens");
    database
        .execute(
            "INSERT INTO planning_task_dependencies (project_id, task_id, blocker_id)
             SELECT project_id, ?1, ?2 FROM planning_tasks WHERE id = ?1",
            [task, blocker],
        )
        .expect("bad edge inserts");
}

/// Replace `from` with `to` in the workspace record of `task`.
fn edit_task_record(directory: &Path, task: &str, from: &str, to: &str) {
    let record = directory.join(format!(".arcl/workspace/tasks/{task}.md"));
    let content = fs::read_to_string(&record).expect("record reads");
    assert!(content.contains(from), "record: {content}");
    fs::write(&record, content.replacen(from, to, 1)).expect("record writes");
}

/// The code and message of each warning in the one JSON value a successful
/// command wrote to stderr.
fn json_warnings(output: &std::process::Output) -> Vec<(String, String)> {
    assert_success(output);
    let envelope = json(&output.stderr);
    assert_eq!(envelope["format_version"], 1);
    envelope["warnings"]
        .as_array()
        .expect("warnings array")
        .iter()
        .map(|warning| {
            let field = |name: &str| warning[name].as_str().expect("warning field").to_owned();
            (field("code"), field("message"))
        })
        .collect()
}

#[test]
fn dependency_edge_already_stored_loads_with_a_warning_on_sync_and_fresh_import() {
    let directory = git_project();
    let path = directory.path();
    let root = create_task(path, "Root", None);
    let child = create_task(path, "Child", Some(&root));
    let other = create_task(path, "Other", None);
    store_raw_edge(path, &root, &child);
    assert_success(
        &arcl_in(path)
            .args(["workspace", "sync"])
            .output()
            .expect("command runs"),
    );
    commit_all(path, "base");
    let violation = format!(
        "workspace file `tasks/{root}.md`: relationship `blocked-by`: task `{root}` cannot be blocked by \
         `{child}`, its descendant; parent chain: {child} -> {root}"
    );

    // An unrelated workspace edit makes the next command import.
    edit_task_record(path, &other, "title = \"Other\"", "title = \"Other edited\"");
    let output = arcl_in(path)
        .args(["--json", "task", "create", "New"])
        .output()
        .expect("command runs");
    assert_eq!(
        json_warnings(&output),
        vec![(
            "blocker_edge".to_owned(),
            format!("{violation}; it was already in the database, so it loads")
        )]
    );
    let output = arcl_in(path)
        .args(["--json", "dependency", "remove", &root, "--blocked-by", &child])
        .output()
        .expect("command runs");
    assert_success(&output);
    assert!(dependency_pairs(path).is_empty());

    // A fresh clone has no database graph to compare against.
    let source = tempdir().expect("temporary directory can be created");
    let source = source.path();
    git(path, &["checkout", "-q", "--", "."]);
    git(path, &["clean", "-q", "-f", "--", ".arcl/workspace"]);
    let clone = source.join("clone");
    git(source, &["clone", "-q", path.to_str().expect("path is UTF-8"), "clone"]);
    run_json(&clone, ["--json", "init"].as_ref());
    let output = arcl_in(&clone)
        .args(["--json", "workspace", "import"])
        .output()
        .expect("command runs");
    assert_eq!(
        json_warnings(&output),
        vec![(
            "blocker_edge".to_owned(),
            format!("{violation}; there is no earlier graph to compare against, so it loads")
        )]
    );
    assert_eq!(dependency_pairs(&clone), vec![(root, child)]);
}

#[test]
fn dependency_edge_a_workspace_change_adds_is_refused_by_sync() {
    let directory = git_project();
    let path = directory.path();
    let root = create_task(path, "Root", None);
    let child = create_task(path, "Child", Some(&root));
    let other = create_task(path, "Other", None);
    run_json(
        path,
        ["--json", "dependency", "add", &other, "--blocked-by", &root].as_ref(),
    );

    // A new edge from Root to its own child.
    edit_task_record(
        path,
        &root,
        "\nstatus = ",
        &format!("\nblocked-by = [\"{child}\"]\nstatus = "),
    );
    let message = refused_message(path, ["--json", "task", "create", "New"].as_ref());
    assert!(
        message.contains(&format!(
            "workspace file `tasks/{root}.md`: relationship `blocked-by`: task `{root}` cannot be blocked by \
             `{child}`, its descendant; parent chain: {child} -> {root}\n"
        )),
        "message: {message}"
    );
    let edge = format!("\nblocked-by = [\"{child}\"]\nstatus = ");
    edit_task_record(path, &root, &edge, "\nstatus = ");

    // A new parent link that puts the stored edge Other <- Root inside one chain.
    edit_task_record(path, &root, "\ntitle = ", &format!("\nparent = \"{other}\"\ntitle = "));
    let message = refused_message(path, ["--json", "task", "create", "New"].as_ref());
    assert!(
        message.contains(&format!(
            "task `{other}` cannot be blocked by `{root}`, its descendant; parent chain: {root} -> {other}\n"
        )),
        "message: {message}"
    );
    assert_eq!(with_workspace_off(path, || parent_of(path, &root)), None);
}

/// The readiness reasons `--json task explain` reports for `task`.
fn explained_reasons(directory: &Path, task: &str) -> Value {
    run_json(directory, ["--json", "task", "explain", task].as_ref())["data"]["readiness"]["reasons"].clone()
}

#[test]
fn reason_kind_status_names_the_task() {
    let directory = initialized_directory();
    let task = create_task(directory.path(), "Task", None);
    run_json(directory.path(), ["--json", "task", "start", &task].as_ref());

    assert_eq!(
        explained_reasons(directory.path(), &task),
        serde_json::json!([{
            "kind": "status",
            "message": "task status is `in_progress`",
            "task": task,
        }])
    );
}

#[test]
fn reason_kind_has_children_names_the_task() {
    let directory = initialized_directory();
    let parent = create_task(directory.path(), "Parent", None);
    create_task(directory.path(), "Child", Some(&parent));

    assert_eq!(
        explained_reasons(directory.path(), &parent),
        serde_json::json!([{
            "kind": "has_children",
            "message": "task has 1 child task(s)",
            "task": parent,
        }])
    );
}

#[test]
fn reason_kind_closed_ancestor_names_the_ancestor() {
    let directory = initialized_directory();
    let parent = create_task(directory.path(), "Parent", None);
    let child = create_task(directory.path(), "Child", Some(&parent));
    run_json(directory.path(), ["--json", "task", "park", &parent].as_ref());

    assert_eq!(
        explained_reasons(directory.path(), &child),
        serde_json::json!([{
            "kind": "closed_ancestor",
            "message": format!("ancestor `{parent}` is `parked`"),
            "ancestor": parent,
        }])
    );
}

#[test]
fn reason_kind_closed_container_names_the_container() {
    let directory = initialized_directory();
    let dir = directory.path();
    let spec = run_json(dir, ["--json", "spec", "create", "Feature"].as_ref());
    let spec_id = spec["data"]["spec"]["id"].as_str().expect("spec ID");
    let plan = run_json(dir, ["--json", "plan", "create", "Build", "--spec", spec_id].as_ref());
    let plan_id = plan["data"]["plan"]["id"].as_str().expect("plan ID").to_owned();
    let task = run_json(dir, ["--json", "task", "create", "Task", "--plan", &plan_id].as_ref())["data"]["task"]["id"]
        .as_str()
        .expect("task ID")
        .to_owned();
    run_json(
        dir,
        ["--json", "plan", "cancel", &plan_id, "--allow-open-children"].as_ref(),
    );

    assert_eq!(
        explained_reasons(dir, &task),
        serde_json::json!([{
            "kind": "closed_container",
            "message": format!("plan `{plan_id}` is `cancelled`"),
            "container": plan_id,
        }])
    );
}

#[test]
fn reason_kind_direct_blocker_names_the_blocker() {
    let directory = initialized_directory();
    let blocker = create_task(directory.path(), "Blocker", None);
    let task = create_task(directory.path(), "Task", None);
    block_start_task(directory.path(), &task, &blocker);

    assert_eq!(
        explained_reasons(directory.path(), &task),
        serde_json::json!([{
            "kind": "direct_blocker",
            "message": format!("blocked by `{blocker}` (pending)"),
            "blocker": blocker,
        }])
    );
}

#[test]
fn reason_kind_inherited_blocker_names_the_ancestor_and_blocker() {
    let directory = initialized_directory();
    let blocker = create_task(directory.path(), "Blocker", None);
    let parent = create_task(directory.path(), "Parent", None);
    let child = create_task(directory.path(), "Child", Some(&parent));
    block_start_task(directory.path(), &parent, &blocker);

    assert_eq!(
        explained_reasons(directory.path(), &child),
        serde_json::json!([{
            "kind": "inherited_blocker",
            "message": format!("ancestor `{parent}` is blocked by `{blocker}` (pending)"),
            "ancestor": parent,
            "blocker": blocker,
        }])
    );
}

#[test]
fn reason_kind_cancelled_blocker_names_the_blocker() {
    let directory = initialized_directory();
    let blocker = create_task(directory.path(), "Blocker", None);
    let task = create_task(directory.path(), "Task", None);
    block_start_task(directory.path(), &task, &blocker);
    run_json(directory.path(), ["--json", "task", "cancel", &blocker].as_ref());

    assert_eq!(
        explained_reasons(directory.path(), &task),
        serde_json::json!([{
            "kind": "cancelled_blocker",
            "message": format!("blocked by cancelled `{blocker}`; remove or replace the dependency"),
            "blocker": blocker,
        }])
    );
}

#[test]
fn reason_kind_invalid_edge_names_the_cycle_path() {
    let directory = initialized_directory();
    let path = directory.path();
    let (a, b) = (create_task(path, "A", None), create_task(path, "B", None));
    insert_raw_edges(path, &[(&a, &b), (&b, &a)]);
    let (low, high) = if a < b { (&a, &b) } else { (&b, &a) };

    let reasons = explained_reasons(path, &a);
    let invalid: Vec<&Value> = reasons
        .as_array()
        .expect("reason array")
        .iter()
        .filter(|reason| reason["kind"] == "invalid_edge")
        .collect();
    assert_eq!(invalid.len(), 1, "{reasons}");
    assert_eq!(invalid[0]["path"], serde_json::json!([low, high, low]));
    // `a` holds the edge to the next step on the cycle, which is `b`.
    assert_eq!(invalid[0]["task"], serde_json::json!(a));
    assert_eq!(invalid[0]["blocker"], serde_json::json!(b));
}

#[test]
fn reason_kind_invalid_edge_names_a_self_loop_twice() {
    let directory = initialized_directory();
    let path = directory.path();
    let a = create_task(path, "A", None);
    insert_raw_edges(path, &[(&a, &a)]);

    let reasons = explained_reasons(path, &a);
    let invalid = reasons
        .as_array()
        .expect("reason array")
        .iter()
        .find(|reason| reason["kind"] == "invalid_edge")
        .expect("invalid edge reason");
    assert_eq!(
        *invalid,
        serde_json::json!({
            "kind": "invalid_edge",
            "message": format!("blocker cycle: {a} -> {a}; each task is blocked by the next"),
            "task": a,
            "blocker": a,
            "path": [a, a],
        })
    );
}

#[test]
fn reason_kind_invalid_edge_names_the_edge_and_parent_chain() {
    let directory = initialized_directory();
    let path = directory.path();
    let root = create_task(path, "Root", None);
    let child = create_task(path, "Child", Some(&root));
    insert_raw_edges(path, &[(&child, &root)]);

    let reasons = explained_reasons(path, &child);
    let invalid = reasons
        .as_array()
        .expect("reason array")
        .iter()
        .find(|reason| reason["kind"] == "invalid_edge")
        .expect("invalid edge reason");
    assert_eq!(
        *invalid,
        serde_json::json!({
            "kind": "invalid_edge",
            "message": format!("task `{child}` is blocked by `{root}`, its ancestor; parent chain: {child} -> {root}"),
            "task": child,
            "blocker": root,
            "path": [child, root],
        })
    );
}

#[test]
fn json_only_chain_picks_work_reads_reasons_and_starts_what_completion_unblocked() {
    let directory = initialized_directory();
    let dir = directory.path();
    let blocker = create_task(dir, "Blocker", None);
    let dependent = create_task(dir, "Dependent", None);
    let unrelated = create_task(dir, "Unrelated", None);
    block_start_task(dir, &dependent, &blocker);

    let ready = run_json(dir, ["--json", "ready"].as_ref());
    assert_eq!(ready["format_version"], 1);
    let picked: Vec<&str> = ready["data"]["tasks"]
        .as_array()
        .expect("ready tasks")
        .iter()
        .map(|task| task["id"].as_str().expect("ready task ID"))
        .collect();
    assert!(picked.contains(&blocker.as_str()) && picked.contains(&unrelated.as_str()));
    assert_eq!(picked.len(), 2);

    let explained = run_json(dir, ["--json", "explain", &dependent].as_ref());
    assert_eq!(explained["format_version"], 2);
    assert_eq!(explained["data"]["readiness"]["ready"], false);
    let reasons = explained["data"]["readiness"]["reasons"].as_array().expect("reasons");
    assert_eq!(reasons.len(), 1);
    assert_eq!(reasons[0]["kind"], "direct_blocker");
    assert_eq!(reasons[0]["blocker"], blocker.as_str());

    let refused = refused(dir, ["--json", "task", "start", &dependent].as_ref());
    assert_eq!(refused["format_version"], 1);
    assert_eq!(refused["error"]["code"], "invalid_request");

    let started = run_json(dir, ["--json", "task", "start", &blocker].as_ref());
    assert_eq!(started["format_version"], 1);
    assert_eq!(started["data"]["task"]["status"], "in_progress");

    let completed = run_json(dir, ["--json", "task", "complete", &blocker].as_ref());
    assert_eq!(completed["format_version"], 2);
    assert_eq!(completed["data"]["completed_subtasks"], serde_json::json!([]));
    let unblocked = completed["data"]["unblocked"].as_array().expect("unblocked");
    assert_eq!(*unblocked, [serde_json::json!(dependent)]);

    let next = unblocked[0].as_str().expect("unblocked ID");
    let started = run_json(dir, ["--json", "task", "start", next].as_ref());
    assert_eq!(started["data"]["task"]["status"], "in_progress");

    let context = run_json(dir, ["--json", "context", next].as_ref());
    assert_eq!(context["format_version"], 2);
    assert_eq!(context["data"]["readiness"]["reasons"][0]["kind"], "status");
    assert_eq!(context["data"]["readiness"]["reasons"][0]["task"], next);
}

/// A running `arcl serve`, stopped when dropped.
struct Served {
    child: std::process::Child,
    port: u16,
    token: String,
}

impl Served {
    /// Start `arcl --plain serve` in `directory` and read the URL it prints.
    fn start(directory: &Path) -> Self {
        use std::io::BufRead;

        let mut child = isolated_command(assert_cmd::cargo::cargo_bin("arcl"), directory)
            .args(["--plain", "serve"])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("arcl serve starts");
        let mut line = String::new();
        std::io::BufReader::new(child.stdout.take().expect("stdout is piped"))
            .read_line(&mut line)
            .expect("arcl serve prints its URL");
        let rest = line
            .trim()
            .strip_prefix("http://127.0.0.1:")
            .expect("the URL names 127.0.0.1");
        let (port, token) = rest.split_once("/?token=").expect("the URL carries the token");
        Self { child, port: port.parse().expect("the URL has a port"), token: token.to_owned() }
    }

    /// Send a GET for `path` with these extra header lines, and return the
    /// status and body.
    fn get(&self, path: &str, headers: &[String]) -> (u16, String) {
        self.send("GET", path, headers, "")
    }

    /// Send a `method` request for `path` with these extra header lines and
    /// `body`, and return the status and body.
    fn send(&self, method: &str, path: &str, headers: &[String], body: &str) -> (u16, String) {
        let (status, _, body) = self.exchange(method, path, headers, body);
        (status, body)
    }

    /// Send a request as [`Served::send`] does, and return the status, the
    /// response head and the body.
    fn exchange(&self, method: &str, path: &str, headers: &[String], body: &str) -> (u16, String, String) {
        use std::io::{Read, Write};

        let mut stream = std::net::TcpStream::connect(("127.0.0.1", self.port)).expect("arcl serve accepts");
        let mut request = format!("{method} {path} HTTP/1.1\r\nConnection: close\r\n");
        for header in headers {
            request.push_str(header);
            request.push_str("\r\n");
        }
        if !body.is_empty() {
            request.push_str(&format!(
                "Content-Type: application/json\r\nContent-Length: {}\r\n",
                body.len()
            ));
        }
        request.push_str("\r\n");
        request.push_str(body);
        stream.write_all(request.as_bytes()).expect("the request writes");
        let mut response = String::new();
        stream.read_to_string(&mut response).expect("the response reads");
        let (head, body) = response.split_once("\r\n\r\n").expect("the response has a body");
        assert!(
            !head.to_ascii_lowercase().contains("access-control-"),
            "the response carries CORS headers: {head}"
        );
        let status = head
            .split(' ')
            .nth(1)
            .expect("a status line")
            .parse()
            .expect("a numeric status");
        (status, head.to_owned(), body.to_owned())
    }

    /// GET `path` with the right `Host` and token.
    fn read(&self, path: &str) -> (u16, Value) {
        let (status, body) = self.get(path, &[self.host(), format!("X-Arcl-Token: {}", self.token)]);
        (status, serde_json::from_str(&body).expect("the body is JSON"))
    }

    /// POST `body` to `path` from the served origin with the right `Host`
    /// and token.
    fn write(&self, path: &str, body: &str) -> (u16, Value) {
        let (status, body) = self.send("POST", path, &self.write_headers(), body);
        (status, serde_json::from_str(&body).expect("the body is JSON"))
    }

    /// The `Host`, token and `Origin` headers a mutation from the app sends.
    fn write_headers(&self) -> Vec<String> {
        vec![
            self.host(),
            format!("X-Arcl-Token: {}", self.token),
            format!("Origin: http://127.0.0.1:{}", self.port),
        ]
    }

    fn host(&self) -> String {
        format!("Host: 127.0.0.1:{}", self.port)
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn serve_read_endpoints_match_the_cli_json() {
    let directory = initialized_directory();
    let blocker = run_json(directory.path(), ["--json", "task", "create", "Blocker"].as_ref());
    let blocker = blocker["data"]["task"]["id"].as_str().expect("a task ID").to_owned();
    let task = run_json(
        directory.path(),
        ["--json", "task", "create", "Blocked", "--priority", "high"].as_ref(),
    );
    let task = task["data"]["task"]["id"].as_str().expect("a task ID").to_owned();
    run_json(
        directory.path(),
        ["--json", "dependency", "add", &task, "--blocked-by", &blocker].as_ref(),
    );

    let served = Served::start(directory.path());
    let pairs: [(String, &[&str]); 6] = [
        (format!("/api/records/{task}"), &["show", &task]),
        (
            "/api/records?kind=task&status=pending".to_owned(),
            &["list", "--kind", "task", "--status", "pending"],
        ),
        ("/api/ready".to_owned(), &["ready"]),
        ("/api/ready?priority=high".to_owned(), &["ready", "--priority", "high"]),
        (format!("/api/tasks/{task}/explain"), &["explain", &task]),
        (format!("/api/tasks/{task}/context"), &["context", &task]),
    ];
    for (path, command) in pairs {
        let expected = run_json(directory.path(), &[&["--json"], command].concat());
        assert_eq!(served.read(&path), (200, expected), "{path}");
    }

    let missing = "arcl-t-00000000000000000000000000";
    let cli = arcl_in(directory.path())
        .args(["--json", "show", missing])
        .output()
        .expect("arcl show runs");
    assert_eq!(cli.status.code(), Some(5));
    assert_eq!(
        served.read(&format!("/api/records/{missing}")),
        (404, json(&cli.stderr))
    );

    let (status, body) = served.read("/api/records?kind=nothing");
    assert_eq!(status, 400);
    assert_eq!(body["error"]["code"], "invalid_request");
}

#[test]
fn serve_work_endpoint_matches_explain_for_every_task() {
    let directory = initialized_directory();
    let blocker = run_json(directory.path(), ["--json", "task", "create", "Blocker"].as_ref());
    let blocker = blocker["data"]["task"]["id"].as_str().expect("a task ID").to_owned();
    let task = run_json(directory.path(), ["--json", "task", "create", "Blocked"].as_ref());
    let task = task["data"]["task"]["id"].as_str().expect("a task ID").to_owned();
    run_json(
        directory.path(),
        ["--json", "dependency", "add", &task, "--blocked-by", &blocker].as_ref(),
    );

    let served = Served::start(directory.path());
    let (status, work) = served.read("/api/work");
    assert_eq!(status, 200);
    assert_eq!(work["format_version"], 2);
    assert_eq!(work["data"]["phases"], serde_json::json!([]));
    let items = work["data"]["tasks"].as_array().expect("tasks");
    let listed = run_json(directory.path(), ["--json", "list", "--kind", "task"].as_ref());
    assert_eq!(
        items.len(),
        listed["data"]["records"].as_array().expect("records").len()
    );
    for item in items {
        let id = item["task"]["id"].as_str().expect("a task ID");
        let explained = run_json(directory.path(), ["--json", "explain", id].as_ref());
        assert_eq!(item["task"], explained["data"]["task"], "{id}");
        assert_eq!(item["readiness"], explained["data"]["readiness"], "{id}");
        assert_eq!(item["phase_id"], Value::Null, "{id}");
    }

    let (status, body) = served.read("/api/work?kind=task");
    assert_eq!(status, 400);
    assert_eq!(body["error"]["code"], "invalid_request");
    assert_eq!(served.get("/api/work", &[served.host()]).0, 401);
    let token = format!("X-Arcl-Token: {}", served.token);
    assert_eq!(
        served.get("/api/work", &["Host: evil.example".to_owned(), token]).0,
        403
    );
}

#[test]
fn serve_refuses_requests_without_the_host_and_token() {
    let directory = initialized_directory();
    let served = Served::start(directory.path());
    let token = format!("X-Arcl-Token: {}", served.token);

    assert_eq!(served.get("/api/ready", &[served.host()]).0, 401);
    assert_eq!(
        served
            .get("/api/ready", &[served.host(), "X-Arcl-Token: wrong".to_owned()])
            .0,
        401
    );
    assert_eq!(
        served
            .get("/api/ready", &["Host: evil.example".to_owned(), token.clone()])
            .0,
        403
    );
    assert_eq!(served.get("/api/ready", std::slice::from_ref(&token)).0, 403);

    let first_load = format!("/?token={}", served.token);
    let (status, page) = served.get(&first_load, &[served.host()]);
    assert_eq!(status, 200);
    assert!(page.contains("<html"), "the first load serves the app page");
    assert_eq!(served.get(&first_load, &[served.host()]).0, 401);
    assert_eq!(
        served
            .get(&format!("/api/ready?token={}", served.token), &[served.host()])
            .0,
        401
    );
    assert_eq!(served.get("/", &[served.host(), token]).0, 200);
}

/// The JSON error `arcl --json <args>` prints, with its exit code.
fn cli_error(directory: &Path, args: &[&str]) -> (Option<i32>, Value) {
    let output = arcl_in(directory)
        .args([&["--json"], args].concat())
        .output()
        .expect("command runs");
    assert!(output.stdout.is_empty(), "the command succeeded: {args:?}");
    (output.status.code(), json(&output.stderr))
}

#[test]
fn serve_task_mutations_change_records_as_the_cli_does() {
    let directory = initialized_directory();
    let dir = directory.path();
    let served = Served::start(dir);

    let (status, created) = served.write(
        "/api/tasks",
        r#"{"title":"Served task","body":"First body","priority":"high"}"#,
    );
    assert_eq!(status, 200, "{created}");
    assert_eq!(created["data"]["action"], "created");
    let task = created["data"]["task"]["id"].as_str().expect("a task ID").to_owned();
    let dependent = create_task(dir, "Waits on it", None);
    run_json(
        dir,
        ["--json", "dependency", "add", &dependent, "--blocked-by", &task].as_ref(),
    );

    let (status, updated) = served.write(
        &format!("/api/tasks/{task}/update"),
        r#"{"title":"Renamed task","body":"Second body"}"#,
    );
    assert_eq!(status, 200, "{updated}");
    assert_eq!(updated["data"]["action"], "updated");
    assert_eq!(task_field(dir, &task, "title"), "Renamed task");
    assert_eq!(task_field(dir, &task, "body"), "Second body");
    assert_eq!(task_field(dir, &task, "priority"), "high");

    let (status, started) = served.write(&format!("/api/tasks/{task}/start"), "");
    assert_eq!((status, &started["data"]["action"]), (200, &Value::from("started")));
    assert_eq!(task_field(dir, &task, "status"), "in_progress");

    let (status, handed) = served.write(&format!("/api/tasks/{task}/handoff"), r#"{"note":"Resume here"}"#);
    assert_eq!((status, &handed["data"]["action"]), (200, &Value::from("handed_off")));
    assert_eq!(task_field(dir, &task, "status"), "parked");
    assert_eq!(task_field(dir, &task, "handoff"), "Resume here");

    let (status, unparked) = served.write(&format!("/api/tasks/{task}/unpark"), "");
    assert_eq!((status, &unparked["data"]["action"]), (200, &Value::from("unparked")));
    let (status, parked) = served.write(&format!("/api/tasks/{task}/park"), "{}");
    assert_eq!((status, &parked["data"]["action"]), (200, &Value::from("parked")));
    served.write(&format!("/api/tasks/{task}/unpark"), "");
    served.write(&format!("/api/tasks/{task}/start"), "");

    let (status, completed) = served.write(
        &format!("/api/tasks/{task}/complete"),
        r#"{"evidence":"Delivered by #1."}"#,
    );
    assert_eq!(status, 200, "{completed}");
    assert_eq!(completed["format_version"], 2);
    assert_eq!(completed["data"]["action"], "completed");
    assert_eq!(completed["data"]["unblocked"], serde_json::json!([dependent]));
    assert_eq!(task_field(dir, &task, "status"), "completed");
    assert_eq!(task_field(dir, &task, "evidence"), "Delivered by #1.");
}

#[test]
fn serve_capture_create_matches_the_cli() {
    let directory = initialized_directory();
    let dir = directory.path();
    let served = Served::start(dir);

    let (status, created) = served.write("/api/captures", r#"{"title":"Served idea","body":"More detail"}"#);
    assert_eq!(status, 200, "{created}");
    assert_eq!(created["data"]["action"], "created");
    let id = created["data"]["capture"]["id"]
        .as_str()
        .expect("a capture ID")
        .to_owned();
    let shown = run_json(dir, ["--json", "capture", "show", &id].as_ref());
    assert_eq!(shown["data"]["capture"]["title"], "Served idea");
    assert_eq!(shown["data"]["capture"]["body"], "More detail");
    assert_eq!(shown["data"]["capture"]["status"], "captured");

    let (code, expected) = cli_error(dir, &["capture", "create", "  "]);
    assert_eq!(code, Some(3));
    assert_eq!(served.write("/api/captures", r#"{"title":"  "}"#), (400, expected));
    assert_eq!(
        served.write("/api/captures", r#"{"title":"T","priority":"high"}"#).0,
        400
    );
}

#[test]
fn serve_task_create_takes_blockers() {
    let directory = initialized_directory();
    let dir = directory.path();
    let blocker = create_task(dir, "First", None);
    let served = Served::start(dir);

    let body = serde_json::json!({ "title": "Second", "blocked_by": [blocker] }).to_string();
    let (status, created) = served.write("/api/tasks", &body);
    assert_eq!(status, 200, "{created}");
    let task = created["data"]["task"]["id"].as_str().expect("a task ID").to_owned();
    let explained = run_json(dir, ["--json", "explain", &task].as_ref());
    assert_eq!(explained["data"]["readiness"]["blocked"], true, "{explained}");
}

#[test]
fn serve_mutation_failures_return_the_cli_error() {
    let directory = initialized_directory();
    let dir = directory.path();
    let blocker = create_task(dir, "Blocker", None);
    let task = create_task(dir, "Blocked", None);
    run_json(
        dir,
        ["--json", "dependency", "add", &task, "--blocked-by", &blocker].as_ref(),
    );
    let served = Served::start(dir);

    let cases: [(String, &str, Vec<&str>); 6] = [
        (format!("/api/tasks/{task}/start"), "", vec!["task", "start", &task]),
        (
            format!("/api/tasks/{blocker}/handoff"),
            r#"{"note":"x"}"#,
            vec!["task", "handoff", &blocker, "--note", "x"],
        ),
        (
            format!("/api/tasks/{blocker}/handoff"),
            "",
            vec!["task", "handoff", &blocker],
        ),
        (
            format!("/api/tasks/{task}/update"),
            r#"{"title":"  "}"#,
            vec!["task", "update", &task, "--title", "  "],
        ),
        (format!("/api/tasks/{task}/update"), "{}", vec!["task", "update", &task]),
        (
            "/api/tasks".to_owned(),
            r#"{"title":"T","priority":"urgent"}"#,
            vec!["task", "create", "T", "--priority", "urgent"],
        ),
    ];
    for (path, body, args) in cases {
        let (code, expected) = cli_error(dir, &args);
        assert_eq!(code, Some(3), "{args:?}");
        assert_eq!(served.write(&path, body), (400, expected), "{path} {body}");
    }

    let missing = "arcl-t-00000000000000000000000000";
    let (code, expected) = cli_error(dir, &["task", "park", missing]);
    assert_eq!(code, Some(5));
    assert_eq!(served.write(&format!("/api/tasks/{missing}/park"), ""), (404, expected));
    assert_eq!(task_field(dir, &task, "status"), "pending");
    assert_eq!(task_field(dir, &task, "title"), "Blocked");
}

#[test]
fn serve_refuses_mutations_without_the_origin_or_on_get() {
    let directory = initialized_directory();
    let dir = directory.path();
    let task = create_task(dir, "Untouched", None);
    let served = Served::start(dir);
    let path = format!("/api/tasks/{task}/start");
    let token = format!("X-Arcl-Token: {}", served.token);

    let missing = served.send("POST", &path, &[served.host(), token.clone()], "{}");
    assert_eq!(missing.0, 403);
    let foreign = served.send(
        "POST",
        &path,
        &[served.host(), token.clone(), "Origin: http://127.0.0.1:1".to_owned()],
        "{}",
    );
    assert_eq!(foreign.0, 403);
    let evil = served.send(
        "POST",
        &path,
        &[served.host(), token.clone(), "Origin: http://evil.example".to_owned()],
        "{}",
    );
    assert_eq!(evil.0, 403);
    let tokenless = served.send(
        "POST",
        &path,
        &[served.host(), format!("Origin: http://127.0.0.1:{}", served.port)],
        "{}",
    );
    assert_eq!(tokenless.0, 401);
    assert_eq!(served.get(&path, &served.write_headers()).0, 405);
    assert_eq!(served.send("POST", "/api/ready", &served.write_headers(), "").0, 405);
    assert_eq!(task_field(dir, &task, "status"), "pending");
}

#[test]
fn serve_mutations_sync_the_workspace() {
    let directory = tempdir().expect("temporary directory can be created");
    let dir = directory.path();
    run_json(dir, ["--json", "init", "--workspace"].as_ref());
    let served = Served::start(dir);

    let (status, created) = served.write("/api/tasks", r#"{"title":"Exported task"}"#);
    assert_eq!(status, 200, "{created}");
    let task = created["data"]["task"]["id"].as_str().expect("a task ID");
    let record =
        fs::read_to_string(dir.join(format!(".arcl/workspace/tasks/{task}.md"))).expect("the task is exported");
    assert!(record.contains("Exported task"), "record: {record}");

    // A change made in the workspace file reaches the next mutation.
    fs::write(
        dir.join(format!(".arcl/workspace/tasks/{task}.md")),
        record.replace("Exported task", "Edited in Git"),
    )
    .expect("the record writes");
    let (status, started) = served.write(&format!("/api/tasks/{task}/start"), "");
    assert_eq!(status, 200, "{started}");
    assert_eq!(started["data"]["task"]["title"], "Edited in Git");
}

#[test]
fn serve_edits_a_record_file_with_the_import_checks() {
    let directory = tempdir().expect("temporary directory can be created");
    let dir = directory.path();
    run_json(dir, ["--json", "init", "--workspace"].as_ref());
    let task = create_task(dir, "Raw task", None);
    let other = create_task(dir, "Other task", None);
    let file = dir.join(format!(".arcl/workspace/tasks/{task}.md"));
    let served = Served::start(dir);

    let (status, read) = served.read(&format!("/api/records/{task}/source"));
    assert_eq!(status, 200, "{read}");
    let source = read["data"]["source"].as_str().expect("a source").to_owned();
    let revision = read["data"]["revision"].as_str().expect("a revision").to_owned();
    assert_eq!(read["data"]["kind"], "task");
    assert_eq!(read["data"]["path"], format!("tasks/{task}.md"));
    assert_eq!(source, fs::read_to_string(&file).expect("the record reads"));

    let edit = |source: &str, revision: &str| {
        served.write(
            &format!("/api/records/{task}/edit"),
            &serde_json::json!({ "source": source, "revision": revision }).to_string(),
        )
    };
    let source_file = |path: &Path| fs::read_to_string(path).expect("the record reads");
    // Each refusal leaves the file as it was.
    let refused = |input: &str, revision: &str, status: u16, message: &str| {
        let (got, reply) = edit(input, revision);
        assert_eq!(got, status, "{reply}");
        let text = reply["error"]["message"].as_str().expect("a message");
        assert!(text.contains(message), "{text}");
        assert_eq!(source_file(&file), source);
    };
    refused("+++\ntitle = \n+++\n", &revision, 400, "front matter TOML is invalid");
    refused(
        &source.replace(&task, &other),
        &revision,
        400,
        "does not match filename",
    );
    refused(
        &format!("+++\nid = \"{task}\"\ntitle = \"Spec\"\nstatus = \"open\"\nacceptance-criteria = \"x\"\n+++\n"),
        &revision,
        400,
        "unknown field",
    );
    refused(
        &source.replace("priority", &format!("parent = \"{task}\"\npriority")),
        &revision,
        400,
        "workspace file",
    );
    // A status change skips the task commands' rules, so it is refused.
    refused(
        &source.replace("status = \"pending\"", "status = \"completed\""),
        &revision,
        400,
        "field `status`: a task's status changes only through `arcl task start`",
    );
    refused(&source, "0000", 409, "changed since it was opened");

    let edited = source
        .replace("title = \"Raw task\"", "title = \"Edited raw\"")
        .replace("priority = \"normal\"", "priority = \"high\"");
    let edited = format!("{edited}\nA new body.\n");
    let (status, saved) = edit(&edited, &revision);
    assert_eq!(status, 200, "{saved}");
    assert_eq!(task_field(dir, &task, "title"), "Edited raw");
    assert_eq!(task_field(dir, &task, "priority"), "high");
    assert_eq!(task_field(dir, &task, "body"), "A new body.");
    let on_disk = source_file(&file);
    assert_eq!(saved["data"]["source"], on_disk.as_str());
    assert!(on_disk.contains("Edited raw"), "{on_disk}");

    // A change to the file since it was read moves its revision, so an edit
    // started from the old one is refused and the change stays.
    let fresh = saved["data"]["revision"].as_str().expect("a revision").to_owned();
    fs::write(&file, on_disk.replace("Edited raw", "Edited in Git")).expect("the record writes");
    let (status, stale) = edit(&on_disk.replace("Edited raw", "Lost edit"), &fresh);
    assert_eq!(status, 409, "{stale}");
    assert_eq!(task_field(dir, &task, "title"), "Edited in Git");

    // A raw edit is a mutation: it needs POST and the served origin.
    let (status, _) = served.get(
        &format!("/api/records/{task}/edit"),
        &[served.host(), format!("X-Arcl-Token: {}", served.token)],
    );
    assert_eq!(status, 405);
    let (status, _) = served.send(
        "POST",
        &format!("/api/records/{task}/edit"),
        &[served.host(), format!("X-Arcl-Token: {}", served.token)],
        r#"{"source":"","revision":""}"#,
    );
    assert_eq!(status, 403);
    let (status, missing) = served.read("/api/records/arcl-t-01M5STHSRRT2ADFN3Y9YAC2N7A/source");
    assert_eq!((status, &missing["error"]["code"]), (404, &Value::from("not_found")));
}

#[test]
fn serve_mutations_give_up_on_a_held_sync_lock_while_reads_go_on() {
    use std::time::{Duration, Instant};

    let directory = tempdir().expect("temporary directory can be created");
    let dir = directory.path();
    run_json(dir, ["--json", "init", "--workspace"].as_ref());
    let task = create_task(dir, "Waits for the lock", None);
    let served = Served::start(dir);
    let lock = fs::OpenOptions::new()
        .write(true)
        .open(dir.join(".arcl/arcl.db-sync.lock"))
        .expect("the lock file opens");
    lock.lock().expect("the test holds the sync lock");

    let started = Instant::now();
    let (status, head, body) = std::thread::scope(|scope| {
        let write = scope.spawn(|| {
            served.exchange(
                "POST",
                &format!("/api/tasks/{task}/start"),
                &served.write_headers(),
                "{}",
            )
        });
        // Reads take no lock, and run while the mutation waits.
        std::thread::sleep(Duration::from_millis(300));
        let read_started = Instant::now();
        let (status, shown) = served.read(&format!("/api/records/{task}"));
        assert_eq!(status, 200, "{shown}");
        assert_eq!(shown["data"]["task"]["status"], "pending");
        assert!(
            read_started.elapsed() < Duration::from_secs(1),
            "the read waited {:?}",
            read_started.elapsed()
        );
        write.join().expect("the mutation answers")
    });
    let waited = started.elapsed();
    assert!(
        waited >= Duration::from_secs(3) && waited < Duration::from_secs(5),
        "waited {waited:?}"
    );
    assert_eq!(status, 503, "{body}");
    assert!(head.to_ascii_lowercase().contains("\r\nretry-after: 1"), "{head}");
    let envelope: Value = serde_json::from_str(&body).expect("the body is JSON");
    assert_eq!(envelope["error"]["code"], "application_error");
    assert_eq!(envelope["error"]["exit_code"], 1);
    assert!(
        envelope["error"]["message"]
            .as_str()
            .expect("a message")
            .contains("sync lock"),
        "{body}"
    );
    // `arcl task show` would wait for the lock, so read through the server.
    let (_, shown) = served.read(&format!("/api/records/{task}"));
    assert_eq!(shown["data"]["task"]["status"], "pending");

    lock.unlock().expect("the test releases the sync lock");
    let (status, started) = served.write(&format!("/api/tasks/{task}/start"), "{}");
    assert_eq!(status, 200, "{started}");
    assert_eq!(task_field(dir, &task, "status"), "in_progress");
}

#[test]
fn serve_mutation_replies_carry_sync_warnings() {
    let directory = git_project();
    let path = directory.path();
    let root = create_task(path, "Root", None);
    let child = create_task(path, "Child", Some(&root));
    let other = create_task(path, "Other", None);
    store_raw_edge(path, &root, &child);
    assert_success(
        &arcl_in(path)
            .args(["workspace", "sync"])
            .output()
            .expect("command runs"),
    );
    commit_all(path, "base");
    // An unrelated workspace edit makes the next mutation import.
    edit_task_record(path, &other, "title = \"Other\"", "title = \"Other edited\"");
    let served = Served::start(path);

    let (status, created) = served.write("/api/tasks", r#"{"title":"New"}"#);
    assert_eq!(status, 200, "{created}");
    assert_eq!(created["data"]["action"], "created");
    let warnings = created["warnings"].as_array().expect("a warnings array");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0]["code"], "blocker_edge");
    let message = warnings[0]["message"].as_str().expect("a message");
    assert!(
        message.contains("it was already in the database, so it loads"),
        "{message}"
    );

    let (status, quiet) = served.write(&format!("/api/tasks/{other}/park"), "");
    assert_eq!(status, 200, "{quiet}");
    assert!(quiet.get("warnings").is_none(), "{quiet}");
}

/// A plan document with one phase holding one task with a subtask.
const SERVED_PLAN: &str = r#"format-version = 1

[[phases]]
key = "first"
title = "First phase"
position = 1

[[phases.tasks]]
key = "build"
title = "Build it"
position = 1
priority = "high"

[[phases.tasks.subtasks]]
key = "test"
title = "Test it"
position = 1
"#;

#[test]
fn serve_promotes_and_discards_captures_as_the_cli_does() {
    let directory = initialized_directory();
    let dir = directory.path();
    let phase_spec = run_json(dir, ["--json", "spec", "create", "Spec"].as_ref())["data"]["spec"]["id"]
        .as_str()
        .expect("a spec ID")
        .to_owned();
    let plan = run_json(
        dir,
        ["--json", "plan", "create", "Plan", "--spec", &phase_spec].as_ref(),
    )["data"]["plan"]["id"]
        .as_str()
        .expect("a plan ID")
        .to_owned();
    let phase = run_json(
        dir,
        [
            "--json",
            "plan",
            "phase",
            "create",
            "Phase",
            "--plan",
            &plan,
            "--position",
            "1",
        ]
        .as_ref(),
    )["data"]["phase"]["id"]
        .as_str()
        .expect("a phase ID")
        .to_owned();
    let to_task = create_capture(dir, "Fix the bug !high", "Details");
    let to_note = create_capture(dir, "A thought", "More");
    let to_discard = create_capture(dir, "Never mind", "");
    let served = Served::start(dir);

    let body = serde_json::json!({ "to": "task", "title": "Fix the bug", "priority": "high", "phase": phase });
    let (status, promoted) = served.write(&format!("/api/captures/{to_task}/promote"), &body.to_string());
    assert_eq!(status, 200, "{promoted}");
    assert_eq!(promoted["data"]["capture"]["status"], "promoted");
    let task = promoted["data"]["task"]["id"].as_str().expect("a task ID").to_owned();
    assert_eq!(task_field(dir, &task, "title"), "Fix the bug");
    assert_eq!(task_field(dir, &task, "body"), "Details");
    assert_eq!(task_field(dir, &task, "priority"), "high");
    assert_eq!(task_field(dir, &task, "phase_id"), Value::from(phase));

    let (status, noted) = served.write(&format!("/api/captures/{to_note}/promote"), r#"{"to":"note"}"#);
    assert_eq!(status, 200, "{noted}");
    let note = noted["data"]["note"]["id"].as_str().expect("a note ID");
    let shown = run_json(dir, ["--json", "note", "show", note].as_ref());
    assert_eq!(shown["data"]["note"]["title"], "A thought");

    let (status, discarded) = served.write(&format!("/api/captures/{to_discard}/discard"), "");
    assert_eq!(status, 200, "{discarded}");
    assert_eq!(discarded["data"]["action"], "discarded");
    let shown = run_json(dir, ["--json", "capture", "show", &to_discard].as_ref());
    assert_eq!(shown["data"]["capture"]["status"], "discarded");

    let open = create_capture(dir, "Still open", "");
    for (path, body, args) in [
        (
            format!("/api/captures/{open}/promote"),
            r#"{"to":"milestone"}"#,
            vec!["capture", "promote", &open, "milestone"],
        ),
        (
            format!("/api/captures/{open}/promote"),
            r#"{"to":"note","phase":"arcl-ph-00000000000000000000000000"}"#,
            vec![
                "capture",
                "promote",
                &open,
                "note",
                "--phase",
                "arcl-ph-00000000000000000000000000",
            ],
        ),
        (
            format!("/api/captures/{to_discard}/promote"),
            r#"{"to":"task"}"#,
            vec!["capture", "promote", &to_discard, "task"],
        ),
    ] {
        let (code, expected) = cli_error(dir, &args);
        let status = match code {
            Some(2 | 3) => 400,
            Some(4) => 409,
            _ => panic!("unexpected exit code {code:?} for {args:?}"),
        };
        assert_eq!(served.write(&path, body), (status, expected), "{path} {body}");
    }
    let shown = run_json(dir, ["--json", "capture", "show", &open].as_ref());
    assert_eq!(shown["data"]["capture"]["status"], "captured");
}

#[test]
fn serve_turns_a_capture_into_a_spec_plan_and_tasks() {
    let directory = initialized_directory();
    let dir = directory.path();
    let capture = create_capture(dir, "Planned idea", "Why it matters");
    let served = Served::start(dir);

    let (status, promoted) = served.write(
        &format!("/api/captures/{capture}/promote"),
        r#"{"to":"spec","acceptance_criteria":"- it works"}"#,
    );
    assert_eq!(status, 200, "{promoted}");
    let spec = promoted["data"]["spec"]["id"].as_str().expect("a spec ID").to_owned();
    let shown = run_json(dir, ["--json", "spec", "show", &spec].as_ref());
    assert_eq!(shown["data"]["spec"]["title"], "Planned idea");
    assert_eq!(shown["data"]["spec"]["acceptance_criteria"], "- it works");

    let body = serde_json::json!({ "title": "Delivery", "spec": spec, "body": "How" }).to_string();
    let (status, created) = served.write("/api/plans", &body);
    assert_eq!(status, 200, "{created}");
    assert_eq!(created["data"]["action"], "created");
    let plan = created["data"]["plan"]["id"].as_str().expect("a plan ID").to_owned();

    let body = serde_json::json!({ "document": SERVED_PLAN }).to_string();
    let (status, applied) = served.write(&format!("/api/plans/{plan}/apply"), &body);
    assert_eq!(status, 200, "{applied}");
    assert_eq!(applied["data"]["plan"]["id"], Value::from(plan.as_str()));
    assert_eq!(applied["data"]["phases"].as_array().expect("phases").len(), 1);
    assert_eq!(applied["data"]["tasks"].as_array().expect("tasks").len(), 2);

    let tree = run_json(dir, ["--json", "tree", &spec].as_ref());
    let plan_node = &tree["data"]["nodes"][0]["children"][0];
    assert_eq!(plan_node["id"], Value::from(plan.as_str()));
    let phase_node = &plan_node["children"][0];
    assert_eq!(phase_node["title"], "First phase");
    assert_eq!(phase_node["children"][0]["title"], "Build it");
    assert_eq!(phase_node["children"][0]["children"][0]["title"], "Test it");
    assert_eq!(
        served.read("/api/tree"),
        (200, run_json(dir, ["--json", "tree"].as_ref()))
    );

    let (status, spec_created) = served.write("/api/specs", r#"{"title":"Fresh spec","acceptance_criteria":"- done"}"#);
    assert_eq!(status, 200, "{spec_created}");
    assert_eq!(spec_created["data"]["spec"]["acceptance_criteria"], "- done");

    let bad = dir.join("bad.toml");
    fs::write(&bad, "format-version = 2\n").expect("the plan file writes");
    let bad_path = bad.to_str().expect("a UTF-8 path");
    let failures = [
        (
            format!("/api/plans/{plan}/apply"),
            serde_json::json!({ "document": "format-version = 2\n" }).to_string(),
            vec!["plan", "apply", plan.as_str(), "--file", bad_path],
        ),
        (
            "/api/plans".to_owned(),
            r#"{"title":"Orphan","spec":"arcl-s-00000000000000000000000000"}"#.to_owned(),
            vec![
                "plan",
                "create",
                "Orphan",
                "--spec",
                "arcl-s-00000000000000000000000000",
            ],
        ),
        (
            "/api/specs".to_owned(),
            r#"{"title":"  "}"#.to_owned(),
            vec!["spec", "create", "  "],
        ),
    ];
    for (path, body, args) in failures {
        let (code, expected) = cli_error(dir, &args);
        let status = match code {
            Some(2 | 3) => 400,
            Some(5) => 404,
            _ => panic!("unexpected exit code {code:?} for {args:?}"),
        };
        assert_eq!(served.write(&path, &body), (status, expected), "{path} {body}");
    }
}

#[test]
fn serve_outside_a_project_fails_before_binding() {
    let directory = tempdir().expect("temporary directory can be created");
    let output = arcl_in(directory.path())
        .args(["--plain", "serve"])
        .timeout(std::time::Duration::from_secs(30))
        .output()
        .expect("arcl serve runs");
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty(), "serve printed a URL outside a project");
}

/// The skill paths `arcl skill install` writes, relative to the project root.
const SKILL_PATHS: [&str; 2] = [
    ".claude/skills/arclightning/SKILL.md",
    ".agents/skills/arclightning/SKILL.md",
];

/// The repository's skill source, which the binary embeds.
fn skill_source() -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills/arclightning/SKILL.md"))
        .expect("skill source reads")
}

/// The `status` of each file in a `skill install` JSON report, keyed by path.
fn skill_statuses(report: &Value) -> Vec<(String, String)> {
    report["data"]["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|file| {
            (
                file["path"].as_str().expect("path").to_owned(),
                file["status"].as_str().expect("status").to_owned(),
            )
        })
        .collect()
}

/// Run `skill install --check` under `--json` and return the exit code, the
/// stdout report and the stderr envelope.
fn skill_check(dir: &Path) -> (Option<i32>, Value, String) {
    let output = arcl_in(dir)
        .args(["--json", "skill", "install", "--check"])
        .output()
        .expect("skill check runs");
    (
        output.status.code(),
        json(&output.stdout),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn skill_install_writes_both_agent_directories() {
    let directory = initialized_directory();
    let dir = directory.path();
    let report = run_json(dir, ["--json", "skill", "install"].as_ref());
    assert_eq!(report["format_version"], 1);
    assert_eq!(report["data"]["action"], "installed");
    let expected = SKILL_PATHS.map(|path| (path.to_owned(), "created".to_owned()));
    assert_eq!(skill_statuses(&report), expected);
    for path in SKILL_PATHS {
        assert_eq!(
            fs::read_to_string(dir.join(path)).expect("skill file written"),
            skill_source()
        );
    }
}

#[test]
fn skill_install_again_leaves_identical_files_unchanged() {
    let directory = initialized_directory();
    let dir = directory.path();
    run_json(dir, ["--json", "skill", "install"].as_ref());
    let modified = SKILL_PATHS.map(|path| {
        fs::metadata(dir.join(path))
            .and_then(|meta| meta.modified())
            .expect("mtime")
    });
    let report = run_json(dir, ["--json", "skill", "install"].as_ref());
    let expected = SKILL_PATHS.map(|path| (path.to_owned(), "unchanged".to_owned()));
    assert_eq!(skill_statuses(&report), expected);
    let after = SKILL_PATHS.map(|path| {
        fs::metadata(dir.join(path))
            .and_then(|meta| meta.modified())
            .expect("mtime")
    });
    assert_eq!(modified, after, "a repeat install rewrote identical files");

    let plain = arcl_in(dir)
        .args(["--plain", "skill", "install"])
        .output()
        .expect("plain runs");
    assert!(plain.status.success());
    let lines = SKILL_PATHS.map(|path| format!("unchanged\t{path}")).join("\n") + "\n";
    assert_eq!(String::from_utf8_lossy(&plain.stdout), lines);

    let quiet = arcl_in(dir)
        .args(["--quiet", "skill", "install"])
        .output()
        .expect("quiet runs");
    assert!(quiet.status.success());
    assert!(quiet.stdout.is_empty());
}

#[test]
fn skill_install_overwrites_an_edited_file() {
    let directory = initialized_directory();
    let dir = directory.path();
    run_json(dir, ["--json", "skill", "install"].as_ref());
    fs::write(dir.join(SKILL_PATHS[1]), "edited\n").expect("edit writes");
    let report = run_json(dir, ["--json", "skill", "install"].as_ref());
    let statuses = skill_statuses(&report);
    assert_eq!(statuses[0].1, "unchanged");
    assert_eq!(statuses[1].1, "updated");
    assert_eq!(
        fs::read_to_string(dir.join(SKILL_PATHS[1])).expect("skill reads"),
        skill_source()
    );
}

#[test]
fn skill_check_passes_after_install() {
    let directory = initialized_directory();
    let dir = directory.path();
    run_json(dir, ["--json", "skill", "install"].as_ref());
    let (code, report, stderr) = skill_check(dir);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert!(stderr.is_empty());
    assert_eq!(report["data"]["action"], "checked");
    let expected = SKILL_PATHS.map(|path| (path.to_owned(), "unchanged".to_owned()));
    assert_eq!(skill_statuses(&report), expected);
}

#[test]
fn skill_check_reports_an_edited_file_and_writes_nothing() {
    let directory = initialized_directory();
    let dir = directory.path();
    run_json(dir, ["--json", "skill", "install"].as_ref());
    fs::write(dir.join(SKILL_PATHS[0]), "edited\n").expect("edit writes");
    let (code, report, stderr) = skill_check(dir);
    assert_eq!(code, Some(4));
    let statuses = skill_statuses(&report);
    assert_eq!(statuses[0], (SKILL_PATHS[0].to_owned(), "changed".to_owned()));
    assert_eq!(statuses[1].1, "unchanged");
    let error = json(stderr.as_bytes());
    assert_eq!(error["error"]["code"], "conflict");
    assert_eq!(
        fs::read_to_string(dir.join(SKILL_PATHS[0])).expect("skill reads"),
        "edited\n"
    );

    let human = arcl_in(dir)
        .args(["--color", "never", "skill", "install", "--check"])
        .output()
        .expect("human check runs");
    assert_eq!(human.status.code(), Some(4));
    let stdout = String::from_utf8_lossy(&human.stdout);
    assert!(
        stdout.contains(&format!("changed {}", SKILL_PATHS[0])),
        "stdout: {stdout}"
    );
    assert!(!stdout.contains(SKILL_PATHS[1]), "stdout: {stdout}");
}

#[test]
fn skill_check_reports_missing_files_and_writes_nothing() {
    let directory = initialized_directory();
    let dir = directory.path();
    let (code, report, _) = skill_check(dir);
    assert_eq!(code, Some(4));
    let expected = SKILL_PATHS.map(|path| (path.to_owned(), "missing".to_owned()));
    assert_eq!(skill_statuses(&report), expected);
    assert!(!dir.join(".claude").exists());
    assert!(!dir.join(".agents").exists());
}

#[test]
fn skill_install_requires_an_initialized_project() {
    let directory = tempdir().expect("temporary directory can be created");
    let output = arcl_in(directory.path())
        .args(["skill", "install"])
        .output()
        .expect("skill install runs");
    assert_eq!(output.status.code(), Some(3));
    assert!(!directory.path().join(".claude").exists());
}

#[cfg(unix)]
#[test]
fn skill_install_follows_a_linked_agents_directory_once() {
    let directory = initialized_directory();
    let dir = directory.path();
    fs::create_dir_all(dir.join(".claude/skills")).expect("claude skills directory");
    fs::create_dir_all(dir.join(".agents")).expect("agents directory");
    std::os::unix::fs::symlink("../.claude/skills", dir.join(".agents/skills")).expect("symlink is created");

    let report = run_json(dir, ["--json", "skill", "install"].as_ref());
    let expected = vec![
        (SKILL_PATHS[0].to_owned(), "created".to_owned()),
        (SKILL_PATHS[1].to_owned(), "unchanged".to_owned()),
    ];
    assert_eq!(skill_statuses(&report), expected);
    assert_eq!(
        fs::read_to_string(dir.join(SKILL_PATHS[1])).expect("skill reads through the link"),
        skill_source()
    );
    assert!(
        fs::symlink_metadata(dir.join(".agents/skills"))
            .expect("link stays")
            .file_type()
            .is_symlink()
    );

    let (code, report, stderr) = skill_check(dir);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    let expected = SKILL_PATHS.map(|path| (path.to_owned(), "unchanged".to_owned()));
    assert_eq!(skill_statuses(&report), expected);
}

#[cfg(unix)]
#[test]
fn skill_install_refuses_a_linked_skill_file() {
    let directory = initialized_directory();
    let dir = directory.path();
    let outside = tempdir().expect("outside directory");
    let target = outside.path().join("bashrc");
    fs::write(&target, "outside\n").expect("outside file writes");
    let skill_dir = dir.join(".agents/skills/arclightning");
    fs::create_dir_all(&skill_dir).expect("skill directory");
    std::os::unix::fs::symlink(&target, skill_dir.join("SKILL.md")).expect("symlink is created");

    for args in [
        ["--json", "skill", "install"].as_ref(),
        ["--json", "skill", "install", "--check"].as_ref(),
    ] {
        let output = arcl_in(dir).args(args).output().expect("skill install runs");
        assert_eq!(output.status.code(), Some(3), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
        let error = json(&output.stderr);
        assert_eq!(error["error"]["code"], "invalid_request");
        let message = error["error"]["message"].as_str().expect("message");
        assert!(message.contains("symbolic link"), "message: {message}");
        assert_eq!(fs::read_to_string(&target).expect("outside file reads"), "outside\n");
        // The check runs before any write, so the valid target stays absent.
        assert!(!dir.join(SKILL_PATHS[0]).exists(), "{args:?}");
    }
}

#[test]
fn skill_install_writes_nothing_when_a_parent_is_a_file() {
    let directory = initialized_directory();
    let dir = directory.path();
    fs::write(dir.join(".agents"), "not a directory\n").expect("file writes");
    let output = arcl_in(dir)
        .args(["--json", "skill", "install"])
        .output()
        .expect("skill install runs");
    assert_eq!(output.status.code(), Some(3));
    let error = json(&output.stderr);
    assert!(
        error["error"]["message"]
            .as_str()
            .expect("message")
            .contains("is not a directory")
    );
    assert!(!dir.join(".claude").exists());
}

/// Run `arcl --json web export` in `directory` into `out` and return the manifest.
fn web_export(directory: &Path, out: &Path) -> Value {
    let out = out.to_str().expect("the export path is UTF-8");
    let exported = run_json(directory, ["--json", "web", "export", out].as_ref());
    assert_eq!(exported["format_version"], 1);
    exported["data"].clone()
}

/// Read the exported document at `path` under `out`.
fn exported(out: &Path, path: &str) -> Value {
    let text = fs::read_to_string(out.join(format!("{path}.json"))).expect("the document file reads");
    serde_json::from_str(&text).expect("the document is JSON")
}

/// The manifest's list `name` as strings.
fn manifest_list(manifest: &Value, name: &str) -> Vec<String> {
    manifest[name]
        .as_array()
        .expect("a list")
        .iter()
        .map(|path| path.as_str().expect("a path").to_owned())
        .collect()
}

#[test]
fn web_export_writes_the_envelopes_arcl_serve_returns() {
    let directory = initialized_directory();
    let dir = directory.path();
    let blocker = create_task(dir, "Blocker", None);
    let task = create_task(dir, "Blocked", None);
    run_json(
        dir,
        ["--json", "dependency", "add", &task, "--blocked-by", &blocker].as_ref(),
    );
    let capture = create_capture(dir, "Idea", "More detail");
    let discarded = create_capture(dir, "Old idea", "");
    run_json(dir, ["--json", "capture", "discard", &discarded].as_ref());
    let out = tempdir().expect("an export directory");

    let manifest = web_export(dir, out.path());
    let served = Served::start(dir);
    let pairs = [
        ("views/work", "/api/work".to_owned()),
        ("views/tree", "/api/tree".to_owned()),
        ("views/tasks", "/api/records?kind=task".to_owned()),
        ("views/captures", "/api/records?kind=capture&status=captured".to_owned()),
        (&*format!("records/{task}"), format!("/api/records/{task}")),
        (&*format!("records/{blocker}"), format!("/api/records/{blocker}")),
        (&*format!("records/{capture}"), format!("/api/records/{capture}")),
        (&*format!("sources/{task}"), format!("/api/records/{task}/source")),
        (&*format!("sources/{blocker}"), format!("/api/records/{blocker}/source")),
    ];
    for (document, path) in &pairs {
        assert_eq!((200, exported(out.path(), document)), served.read(path), "{document}");
    }

    let mut documents = pairs
        .iter()
        .map(|(document, _)| (*document).to_owned())
        .collect::<Vec<_>>();
    documents.push("meta/sync".to_owned());
    let mut listed = manifest_list(&manifest, "documents");
    listed.sort();
    documents.sort();
    assert_eq!(
        listed, documents,
        "a discarded capture is not exported, and a capture has no file document"
    );
    assert_eq!(manifest_list(&manifest, "changed").len(), documents.len());
    assert_eq!(manifest_list(&manifest, "removed"), Vec::<String>::new());
    let meta = exported(out.path(), "meta/sync");
    assert_eq!(meta["revision"], manifest["revision"]);
    assert_eq!(meta["commit"], Value::Null, "the project is not in Git");
    assert_eq!(meta["synced_at"], manifest["synced_at"]);
    let on_disk: Value = serde_json::from_str(&fs::read_to_string(out.path().join("manifest.json")).expect("reads"))
        .expect("the manifest is JSON");
    assert_eq!(on_disk, manifest);
}

#[test]
fn web_export_rewrites_only_the_documents_that_changed() {
    let directory = initialized_directory();
    let dir = directory.path();
    let task = create_task(dir, "First", None);
    let other = create_task(dir, "Second", None);
    let capture = create_capture(dir, "Idea", "");
    let out = tempdir().expect("an export directory");
    let first = web_export(dir, out.path());

    let again = web_export(dir, out.path());
    // Only `meta/sync` can change, and only when the clock has moved to another second.
    let changed = manifest_list(&again, "changed");
    assert!(changed.iter().all(|path| path == "meta/sync"), "{changed:?}");
    assert_eq!(again["revision"], first["revision"]);

    run_json(dir, ["--json", "task", "update", &task, "--title", "Renamed"].as_ref());
    run_json(dir, ["--json", "capture", "discard", &capture].as_ref());
    let after = web_export(dir, out.path());
    let changed = manifest_list(&after, "changed");
    for document in [
        "views/work",
        "views/tree",
        "views/tasks",
        "views/captures",
        &format!("records/{task}"),
        &format!("sources/{task}"),
        "meta/sync",
    ] {
        assert!(changed.iter().any(|path| path == document), "{document} in {changed:?}");
    }
    assert!(!changed.contains(&format!("records/{other}")), "{changed:?}");
    assert!(!changed.contains(&format!("sources/{other}")), "{changed:?}");
    assert_ne!(after["revision"], first["revision"]);
    assert_eq!(manifest_list(&after, "removed"), [format!("records/{capture}")]);
    assert!(!out.path().join(format!("records/{capture}.json")).exists());
    assert_eq!(
        exported(out.path(), &format!("records/{task}"))["data"]["task"]["title"],
        "Renamed"
    );
}

#[test]
fn web_export_splits_a_document_over_the_size_limit() {
    let directory = initialized_directory();
    let dir = directory.path();
    let body = "A line of the task body that runs on.\n".repeat(8_000);
    let body_file = dir.join("body.md");
    fs::write(&body_file, &body).expect("the body writes");
    let created = run_json(
        dir,
        [
            "--json",
            "task",
            "create",
            "Large",
            "--body-file",
            body_file.to_str().expect("UTF-8"),
        ]
        .as_ref(),
    );
    let task = created["data"]["task"]["id"].as_str().expect("a task ID").to_owned();
    let out = tempdir().expect("an export directory");
    let manifest = web_export(dir, out.path());

    let header = exported(out.path(), &format!("records/{task}"));
    let parts = header["chunked"]["parts"].as_u64().expect("a part count");
    assert!(parts >= 2, "{header}");
    let mut joined = String::new();
    for index in 0..parts {
        let path = format!("chunks/records~{task}~{index}");
        let size = fs::metadata(out.path().join(format!("{path}.json")))
            .expect("the part exists")
            .len();
        assert!(size < 256 * 1024, "{path} is {size} bytes");
        joined.push_str(exported(out.path(), &path)["part"].as_str().expect("a part"));
    }
    let served = Served::start(dir);
    let joined: Value = serde_json::from_str(&joined).expect("the parts join into JSON");
    assert_eq!((200, joined), served.read(&format!("/api/records/{task}")));
    // Parts come before the header they belong to, so a reader never sees a header without them.
    let documents = manifest_list(&manifest, "documents");
    let header_at = documents
        .iter()
        .position(|path| *path == format!("records/{task}"))
        .expect("listed");
    let part_at = documents
        .iter()
        .position(|path| *path == format!("chunks/records~{task}~0"))
        .expect("listed");
    assert!(part_at < header_at);
    // `views/work` holds the body too, and is split the same way.
    assert!(exported(out.path(), "views/work")["chunked"].is_object());
    // So does the record's file, which the page's raw editor reads.
    let header = exported(out.path(), &format!("sources/{task}"));
    let parts = header["chunked"]["parts"].as_u64().expect("a part count");
    assert!(parts >= 2, "{header}");
    let joined = (0..parts)
        .map(|index| {
            let path = format!("chunks/sources~{task}~{index}");
            exported(out.path(), &path)["part"].as_str().expect("a part").to_owned()
        })
        .collect::<String>();
    let joined: Value = serde_json::from_str(&joined).expect("the parts join into JSON");
    assert_eq!((200, joined), served.read(&format!("/api/records/{task}/source")));
}

#[test]
fn web_export_full_lists_every_document_as_changed() {
    let directory = initialized_directory();
    let dir = directory.path();
    let task = create_task(dir, "Task", None);
    let capture = create_capture(dir, "Idea", "");
    let out = tempdir().expect("an export directory");
    web_export(dir, out.path());

    run_json(dir, ["--json", "capture", "discard", &capture].as_ref());
    let out_path = out.path().to_str().expect("UTF-8");
    let full = run_json(dir, ["--json", "web", "export", "--full", out_path].as_ref())["data"].clone();
    let documents = manifest_list(&full, "documents");
    assert!(documents.contains(&format!("records/{task}")));
    assert_eq!(manifest_list(&full, "changed"), documents);
    assert_eq!(manifest_list(&full, "removed"), [format!("records/{capture}")]);
}

#[test]
fn web_export_removes_the_parts_a_shrunk_document_no_longer_has() {
    let directory = initialized_directory();
    let dir = directory.path();
    let body_file = dir.join("body.md");
    fs::write(&body_file, "A line of the task body that runs on.\n".repeat(8_000)).expect("the body writes");
    let body_path = body_file.to_str().expect("UTF-8");
    let created = run_json(
        dir,
        ["--json", "task", "create", "Large", "--body-file", body_path].as_ref(),
    );
    let task = created["data"]["task"]["id"].as_str().expect("a task ID").to_owned();
    let out = tempdir().expect("an export directory");
    let first = web_export(dir, out.path());
    let parts = manifest_list(&first, "documents")
        .into_iter()
        .filter(|path| path.starts_with(&format!("chunks/records~{task}~")))
        .collect::<Vec<_>>();
    assert!(parts.len() >= 2, "{parts:?}");

    fs::write(&body_file, "Short now.").expect("the body writes");
    run_json(
        dir,
        ["--json", "task", "update", &task, "--body-file", body_path].as_ref(),
    );
    let second = web_export(dir, out.path());
    let removed = manifest_list(&second, "removed");
    for part in &parts {
        assert!(removed.contains(part), "{part} in {removed:?}");
        assert!(!out.path().join(format!("{part}.json")).exists(), "{part}");
    }
    assert!(removed.iter().all(|path| path.starts_with("chunks/")), "{removed:?}");
    assert!(
        manifest_list(&second, "documents")
            .iter()
            .all(|path| !path.starts_with("chunks/"))
    );
    assert_eq!(
        exported(out.path(), &format!("records/{task}"))["data"]["task"]["body"],
        "Short now."
    );
}

#[cfg(unix)]
#[test]
fn web_export_fails_and_writes_nothing_when_the_sync_fails() {
    let directory = tempdir().expect("temporary directory can be created");
    let dir = directory.path();
    run_json(dir, ["--json", "init", "--workspace"].as_ref());
    create_capture(dir, "Idea", "");
    let workspace = dir.join(".arcl/workspace");
    let moved = dir.join("moved-workspace");
    fs::rename(&workspace, &moved).expect("workspace moves");
    std::os::unix::fs::symlink(&moved, &workspace).expect("symlink is created");

    let out = tempdir().expect("an export directory");
    let output = arcl_in(dir)
        .args(["--json", "web", "export", out.path().to_str().expect("UTF-8")])
        .output()
        .expect("web export runs");
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    let message = json(&output.stderr)["error"]["message"]
        .as_str()
        .expect("a message")
        .to_owned();
    assert!(
        message.contains("did not sync") && message.contains("symbolic link"),
        "{message}"
    );
    assert_eq!(fs::read_dir(out.path()).expect("the directory reads").count(), 0);
}

/// Write `queue` as the queue file under `directory` and run `arcl --json web apply` on it.
fn web_apply(directory: &Path, queue: &Value) -> std::process::Output {
    let file = directory.join("queue.json");
    fs::write(&file, queue.to_string()).expect("the queue writes");
    arcl_in(directory)
        .args(["--json", "web", "apply", file.to_str().expect("UTF-8")])
        .output()
        .expect("web apply runs")
}

#[test]
fn web_apply_runs_queued_edits_as_arcl_serve_does() {
    let directory = initialized_directory();
    let dir = directory.path();
    let capture = create_capture(dir, "Idea", "");
    let promoted = create_capture(dir, "Make it a task", "");
    let task = create_task(dir, "Task", None);
    let missing = "arcl-c-00000000000000000000000000";
    let queue = serde_json::json!([
        { "id": "q1", "op": "discard_capture", "target": capture, "input": {}, "created_at": "2026-10-25T06:30:00Z", "created_by": "u_1", "status": "queued" },
        { "id": "q2", "op": "promote_capture", "target": promoted, "input": { "to": "task", "priority": "high" }, "status": "queued" },
        { "id": "q3", "op": "create_capture", "input": { "title": "  " }, "status": "queued" },
        { "id": "q4", "op": "create_capture", "input": { "title": "T", "priority": "high" }, "status": "queued" },
        { "id": "q5", "op": "discard_capture", "target": missing, "input": {}, "status": "queued" },
        { "id": "q6", "op": "delete_everything", "target": task, "status": "queued" },
        { "id": "q7", "op": "park_task", "target": task, "status": "applied" },
        { "id": "q8", "op": "update_task", "target": task, "input": "not an object", "status": "queued" },
        { "id": "q9", "op": "start_task", "target": task, "input": {}, "status": "failed", "error": { "code": "x", "message": "y" } },
        { "id": "q10", "op": "park_task", "target": task, "input": {}, "status": "applying" },
        { "id": "q11", "op": "park_task", "target": task, "input": {} }
    ]);
    let output = web_apply(dir, &queue);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let results = json(&output.stdout)["data"]["results"]
        .as_array()
        .expect("results")
        .clone();
    let statuses = results
        .iter()
        .map(|result| {
            (
                result["id"].as_str().expect("an ID"),
                result["status"].as_str().expect("a status"),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        statuses,
        [
            ("q1", "applied"),
            ("q2", "applied"),
            ("q3", "failed"),
            ("q4", "failed"),
            ("q5", "failed"),
            ("q6", "failed"),
            ("q7", "skipped"),
            ("q8", "failed"),
            ("q9", "skipped"),
            ("q10", "skipped"),
            ("q11", "skipped")
        ]
    );

    assert_eq!(capture_field(dir, &capture, "status"), "discarded");
    assert_eq!(results[0]["data"]["capture"]["status"], "discarded");
    let made = results[1]["data"]["task"]["id"].as_str().expect("the new task's ID");
    assert_eq!(task_field(dir, made, "priority"), "high");
    assert_eq!(
        task_field(dir, &task, "status"),
        "pending",
        "skipped entries do not run"
    );

    let (_, blank) = cli_error(dir, &["capture", "create", "  "]);
    assert_eq!(results[2]["error"], blank["error"], "the CLI's validation and error");
    let (_, absent) = cli_error(dir, &["capture", "discard", missing]);
    assert_eq!(results[4]["error"], absent["error"]);
    for index in [3, 5, 7] {
        assert_eq!(results[index]["error"]["code"], "invalid_request", "{}", results[index]);
    }
    assert!(
        results[5]["error"]["message"]
            .as_str()
            .expect("a message")
            .contains("unknown operation")
    );
}

#[test]
fn web_apply_runs_a_queued_raw_edit_as_the_edit_endpoint_does() {
    let directory = initialized_directory();
    let dir = directory.path();
    let task = create_task(dir, "Raw task", None);
    let out = tempdir().expect("an export directory");
    web_export(dir, out.path());
    // The page edits the file it read from the export, with that revision.
    let read = exported(out.path(), &format!("sources/{task}"));
    let source = read["data"]["source"].as_str().expect("a source").to_owned();
    let revision = read["data"]["revision"].as_str().expect("a revision").to_owned();
    let entry = |id: &str, source: &str, revision: &str| serde_json::json!({ "id": id, "op": "update_source", "target": task, "input": { "source": source, "revision": revision }, "status": "queued" });
    let large = format!(
        "{}\n{}",
        source.replace("Raw task", "Large raw"),
        "A long line.\n".repeat(20_000)
    );
    let queue = serde_json::json!([
        entry("q1", "+++\ntitle = \n+++\n", &revision),
        entry(
            "q2",
            &source.replace("status = \"pending\"", "status = \"completed\""),
            &revision
        ),
        entry("q3", &source.replace("Raw task", "Edited raw"), &revision),
        // q3 moved the file's revision, so an edit from the read before it is stale.
        entry("q4", &source.replace("Raw task", "Lost edit"), &revision),
    ]);
    let output = web_apply(dir, &queue);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let results = json(&output.stdout)["data"]["results"]
        .as_array()
        .expect("results")
        .clone();
    let statuses = results
        .iter()
        .map(|result| result["status"].as_str().expect("a status"))
        .collect::<Vec<_>>();
    assert_eq!(statuses, ["failed", "failed", "applied", "failed"]);
    let message = |index: usize| {
        results[index]["error"]["message"]
            .as_str()
            .expect("a message")
            .to_owned()
    };
    assert!(message(0).contains("front matter TOML is invalid"), "{}", message(0));
    assert!(message(1).contains("field `status`"), "{}", message(1));
    assert_eq!(results[3]["error"]["code"], "conflict");
    assert!(message(3).contains("changed since it was opened"), "{}", message(3));
    assert_eq!(task_field(dir, &task, "title"), "Edited raw");
    assert_eq!(task_field(dir, &task, "status"), "pending");
    let saved = results[2]["data"].clone();
    assert_eq!(saved["id"], task.as_str());
    assert_ne!(saved["revision"], revision.as_str());

    // The page refuses a queue entry over 200 KiB; `apply` takes an input up
    // to the edit endpoint's 1 MiB body limit, so a larger file saves. An
    // input over that limit fails on its own, and the entries after it run.
    let fresh = saved["revision"].as_str().expect("a revision").to_owned();
    assert!(large.len() > 200 * 1024);
    let output = web_apply(dir, &serde_json::json!([entry("q5", &large, &fresh)]));
    let results = json(&output.stdout)["data"]["results"].clone();
    assert_eq!(results[0]["status"], "applied", "{results}");
    assert_eq!(task_field(dir, &task, "title"), "Large raw");

    let fresh = results[0]["data"]["revision"].as_str().expect("a revision").to_owned();
    let huge = format!("{source}\n{}", "A long line.\n".repeat(90_000));
    assert!(huge.len() > 1024 * 1024);
    let small = source.replace("Raw task", "After huge");
    let output = web_apply(
        dir,
        &serde_json::json!([entry("q6", &huge, &fresh), entry("q7", &small, &fresh)]),
    );
    let results = json(&output.stdout)["data"]["results"].clone();
    assert_eq!(results[0]["status"], "failed", "{results}");
    assert_eq!(results[0]["error"]["code"], "invalid_request");
    assert!(
        results[0]["error"]["message"]
            .as_str()
            .expect("a message")
            .contains("over 1 MiB")
    );
    assert_eq!(results[1]["status"], "applied", "{results}");
    assert_eq!(task_field(dir, &task, "title"), "After huge");
}

#[test]
fn web_apply_refuses_a_file_that_is_not_an_array_and_applies_nothing() {
    let directory = initialized_directory();
    let dir = directory.path();
    let capture = create_capture(dir, "Idea", "");
    let entry = serde_json::json!({ "id": "q1", "op": "discard_capture", "target": capture, "status": "queued" });
    let output = web_apply(dir, &entry);
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    let error = json(&output.stderr);
    assert_eq!(error["error"]["code"], "invalid_request");
    assert!(
        error["error"]["message"]
            .as_str()
            .expect("a message")
            .contains("not a JSON array")
    );
    assert_eq!(capture_field(dir, &capture, "status"), "captured");
}

#[test]
fn web_apply_fails_a_malformed_entry_and_applies_the_rest() {
    let directory = initialized_directory();
    let dir = directory.path();
    let first = create_capture(dir, "First", "");
    let second = create_capture(dir, "Second", "");
    let queue = serde_json::json!([
        { "id": "q1", "op": "discard_capture", "target": first, "status": "queued", "run_as": "root" },
        { "id": "q2", "op": "discard_capture", "target": 5, "status": "queued" },
        { "op": "discard_capture", "target": first, "status": "queued" },
        "not an entry",
        { "id": "q5", "op": "discard_capture", "target": second, "input": {}, "status": "queued" }
    ]);
    let output = web_apply(dir, &queue);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let results = json(&output.stdout)["data"]["results"]
        .as_array()
        .expect("results")
        .clone();
    let summary = results
        .iter()
        .map(|result| (result["id"].clone(), result["status"].as_str().expect("a status")))
        .collect::<Vec<_>>();
    assert_eq!(
        summary,
        [
            (Value::from("q1"), "failed"),
            (Value::from("q2"), "failed"),
            (Value::Null, "failed"),
            (Value::Null, "failed"),
            (Value::from("q5"), "applied"),
        ]
    );
    let message = |index: usize| {
        results[index]["error"]["message"]
            .as_str()
            .expect("a message")
            .to_owned()
    };
    for (index, expected) in [
        (0, "queue entry 0 (`q1`)"),
        (0, "unknown field `run_as`"),
        (1, "queue entry 1 (`q2`)"),
        (2, "queue entry 2 is not"),
        (2, "missing field `id`"),
        (3, "queue entry 3 is not"),
        (3, "not a JSON object"),
    ] {
        assert!(message(index).contains(expected), "{expected} in {}", message(index));
        assert_eq!(results[index]["error"]["code"], "invalid_request");
    }
    assert_eq!(results[0]["op"], "discard_capture");
    assert_eq!(results[0]["target"], first);
    assert_eq!(results[1]["target"], Value::Null);
    assert_eq!(
        capture_field(dir, &first, "status"),
        "captured",
        "a malformed entry does not run"
    );
    assert_eq!(capture_field(dir, &second, "status"), "discarded");
}

/// Import a copy of task `id`, titled `title`, whose ID differs from `id`
/// only in the fifth character from the end, so the two share a 4-character
/// suffix. Returns the copy's ID.
fn import_suffix_twin(directory: &Path, id: &str, title: &str) -> String {
    let ulid = id.strip_prefix("arcl-t-").expect("a task ID");
    let changed = if &ulid[21..22] == "0" { "1" } else { "0" };
    let twin = format!("arcl-t-{}{changed}{}", &ulid[..21], &ulid[22..]);
    let tasks = directory.join(".arcl/workspace/tasks");
    let original = fs::read_to_string(tasks.join(format!("{id}.md"))).expect("task file reads");
    let copy = original
        .replace(id, &twin)
        .replace("title = \"First\"", &format!("title = \"{title}\""));
    fs::write(tasks.join(format!("{twin}.md")), copy).expect("twin task file writes");
    assert_success(
        &arcl_in(directory)
            .args(["workspace", "import"])
            .output()
            .expect("import runs"),
    );
    twin
}

/// The text a successful human-output command prints.
fn human_stdout(directory: &Path, args: &[&str]) -> String {
    let output = arcl_in(directory)
        .args(["--color", "never"])
        .args(args)
        .output()
        .expect("command runs");
    assert_success(&output);
    String::from_utf8(output.stdout).expect("output is UTF-8")
}

/// The short ID human `list` output shows for the record `id`.
fn short_id(directory: &Path, id: &str) -> String {
    human_stdout(directory, &["list"])
        .lines()
        .filter_map(|line| line.split('\t').nth(1))
        .find(|short| id.ends_with(short))
        .unwrap_or_else(|| panic!("`list` shows {id}"))
        .to_owned()
}

#[test]
fn short_ids_select_records_and_grow_when_suffixes_collide() {
    let directory = tempdir().expect("temporary directory can be created");
    let dir = directory.path();
    run_json(dir, ["--json", "init", "--workspace"].as_ref());
    let first = create_task(dir, "First", None);
    let other = create_task(dir, "Other", None);
    let twin = import_suffix_twin(dir, &first, "Twin");
    let suffix = |id: &str, length: usize| id[id.len() - length..].to_owned();

    // The two tasks that share four characters show five; the other shows four.
    let list = human_stdout(dir, &["list", "--kind", "task"]);
    for (id, length, title) in [(&first, 5, "First"), (&twin, 5, "Twin"), (&other, 4, "Other")] {
        assert!(
            list.contains(&format!("task\t{}\t[pending]\t{title}", suffix(id, length))),
            "{list}"
        );
    }
    let plain = human_stdout(dir, &["--plain", "tree"]);
    assert!(plain.contains(&format!("0\ttask\t{other}\tpending\tOther")), "{plain}");

    // A unique suffix selects its record, in either case and with a kind.
    let shown = human_stdout(dir, &["show", &suffix(&first, 5)]);
    assert!(
        shown.starts_with(&format!("Task `{}` [pending] First", suffix(&first, 5))),
        "{shown}"
    );
    let lower = format!("t-{}", suffix(&other, 4).to_lowercase());
    let shown = run_json(dir, ["--json", "show", &lower].as_ref());
    assert_eq!(shown["data"]["task"]["id"], other.as_str(), "JSON keeps the full ID");
    let rooted = human_stdout(dir, &["tree", &suffix(&twin, 6)]);
    assert_eq!(rooted.trim(), "● Twin");
    let started = run_json(dir, ["--json", "task", "start", &suffix(&first, 5)].as_ref());
    assert_eq!(started["data"]["task"]["id"], first.as_str());
    assert_eq!(started["data"]["task"]["status"], "in_progress");
    let listed = run_json(dir, ["--json", "list", "--kind", "task"].as_ref());
    assert!(
        listed["data"]["records"]
            .as_array()
            .expect("records")
            .iter()
            .all(|record| record["id"].as_str().is_some_and(|id| id.starts_with("arcl-t-"))),
        "{listed}"
    );

    // An ambiguous suffix lists every match; a kind that matches none is not found.
    let ambiguous = arcl_in(dir)
        .args(["task", "start", &suffix(&first, 4)])
        .output()
        .expect("start runs");
    assert_eq!(ambiguous.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&ambiguous.stderr);
    assert!(stderr.contains(&format!("{first}  First")), "{stderr}");
    assert!(stderr.contains(&format!("{twin}  Twin")), "{stderr}");
    let missing = arcl_in(dir)
        .args(["show", &format!("n-{}", suffix(&other, 4))])
        .output()
        .expect("show runs");
    assert_eq!(missing.status.code(), Some(5));

    // Too short or invalid input fails as an invalid record.
    for input in ["ABC", "t-AB", "GBU5", "x-ABCD"] {
        let refused = arcl_in(dir)
            .args(["--json", "show", input])
            .output()
            .expect("show runs");
        assert_eq!(refused.status.code(), Some(3), "{input}");
        assert_eq!(json(&refused.stderr)["error"]["code"], "invalid_request", "{input}");
    }
}

#[test]
fn records_sharing_a_whole_ulid_show_their_kind_and_resolve_by_command() {
    let directory = tempdir().expect("temporary directory can be created");
    let dir = directory.path();
    run_json(dir, ["--json", "init", "--workspace"].as_ref());
    let task = create_task(dir, "Task", None);
    let spec = run_json(dir, ["--json", "spec", "create", "Spec"].as_ref())["data"]["spec"]["id"]
        .as_str()
        .expect("spec ID")
        .to_owned();
    // A spec file copied under the task's ULID: a workspace import loads both.
    let ulid = task.strip_prefix("arcl-t-").expect("a task ID");
    let copy = format!("arcl-s-{ulid}");
    let specs = dir.join(".arcl/workspace/specs");
    let original = fs::read_to_string(specs.join(format!("{spec}.md"))).expect("spec file reads");
    let text = original
        .replace(&spec, &copy)
        .replace("title = \"Spec\"", "title = \"Copy\"");
    fs::write(specs.join(format!("{copy}.md")), text).expect("copied spec file writes");
    assert_success(
        &arcl_in(dir)
            .args(["workspace", "import"])
            .output()
            .expect("import runs"),
    );

    // Each record sharing the ULID shows its kind, and that short ID resolves.
    let list = human_stdout(dir, &["list"]);
    assert!(list.contains(&format!("task\tt-{ulid}\t[pending]\tTask")), "{list}");
    assert!(list.contains(&format!("spec\ts-{ulid}\t[open]\tCopy")), "{list}");
    let shown = run_json(dir, ["--json", "show", &format!("s-{ulid}")].as_ref());
    assert_eq!(shown["data"]["spec"]["id"], copy.as_str());

    // `task show`, `task explain` and `explain` print the IDs `show` prints.
    let suffix = &ulid[ulid.len() - 4..];
    let show = human_stdout(dir, &["show", &format!("t-{suffix}")]);
    assert!(show.starts_with(&format!("Task `t-{ulid}`")), "{show}");
    assert_eq!(human_stdout(dir, &["task", "show", suffix]), show);
    assert_eq!(human_stdout(dir, &["task", "explain", suffix]), show);
    assert_eq!(human_stdout(dir, &["explain", suffix]), show);
    for command in [["--plain", "show"], ["--plain", "explain"]] {
        let plain = human_stdout(dir, &[command[0], command[1], &format!("t-{suffix}")]);
        assert!(
            plain.starts_with(&format!("task\t{task}\t")),
            "plain keeps full IDs: {plain}"
        );
    }
    let plain = human_stdout(dir, &["--plain", "tree"]);
    assert!(plain.contains(&format!("\t{copy}\t")), "{plain}");

    // A task command reads a bare suffix as a task's, though a spec shares it.
    let started = run_json(dir, ["--json", "task", "start", suffix].as_ref());
    assert_eq!(started["data"]["task"]["id"], task.as_str());
    let ambiguous = arcl_in(dir).args(["show", suffix]).output().expect("show runs");
    assert_eq!(ambiguous.status.code(), Some(3), "a kindless command stays ambiguous");
}

#[test]
fn readiness_reasons_show_short_ids_in_human_output_only() {
    let directory = initialized_directory();
    let dir = directory.path();
    let blocker = create_task(dir, "Blocker", None);
    let parent = create_task(dir, "Parent", None);
    let leaf = create_task(dir, "Leaf", Some(&parent));
    block_start_task(dir, &leaf, &blocker);
    run_json(dir, ["--json", "task", "park", &parent].as_ref());
    let (short_blocker, short_parent) = (short_id(dir, &blocker), short_id(dir, &parent));

    for command in [["task", "show"], ["task", "explain"]] {
        let human = human_stdout(dir, &[command[0], command[1], &leaf]);
        assert!(
            human.contains(&format!("Reason: blocked by `{short_blocker}` (pending)")),
            "{human}"
        );
        assert!(
            human.contains(&format!("Reason: ancestor `{short_parent}` is `parked`")),
            "{human}"
        );
        assert!(!human.contains("arcl-t-"), "human output names no full ID: {human}");

        let plain = human_stdout(dir, &["--plain", command[0], command[1], &leaf]);
        assert!(
            plain.contains(&format!("reason\tblocked by `{blocker}` (pending)")),
            "{plain}"
        );
        assert!(
            plain.contains(&format!("reason\tancestor `{parent}` is `parked`")),
            "{plain}"
        );
    }
    let explained = run_json(dir, ["--json", "task", "explain", &leaf].as_ref());
    let messages = reason_messages(&explained["data"]["readiness"]);
    assert!(
        messages.contains(&format!("blocked by `{blocker}` (pending)")),
        "{messages:?}"
    );
    assert!(
        messages.contains(&format!("ancestor `{parent}` is `parked`")),
        "{messages:?}"
    );
}

/// Copy the workspace file of record `original`, in `.arcl/workspace/<kind_dir>`,
/// to one for `copy`, and import it.
fn import_record_copy(directory: &Path, kind_dir: &str, original: &str, copy: &str) {
    let records = directory.join(".arcl/workspace").join(kind_dir);
    let text = fs::read_to_string(records.join(format!("{original}.md"))).expect("record file reads");
    fs::write(records.join(format!("{copy}.md")), text.replace(original, copy)).expect("copy writes");
    assert_success(
        &arcl_in(directory)
            .args(["workspace", "import"])
            .output()
            .expect("import runs"),
    );
}

#[test]
fn every_short_id_argument_resolves_by_the_kind_it_expects() {
    let directory = tempdir().expect("temporary directory can be created");
    let dir = directory.path();
    run_json(dir, ["--json", "init", "--workspace"].as_ref());
    let created = |args: &[&str], record: &str| {
        run_json(dir, args)["data"][record]["id"]
            .as_str()
            .expect("record ID")
            .to_owned()
    };
    let task = create_task(dir, "Task", None);
    let spec = created(&["--json", "spec", "create", "Spec"], "spec");
    let plan = created(&["--json", "plan", "create", "Plan", "--spec", &spec], "plan");
    let note = created(&["--json", "note", "create", "Note"], "note");
    let linker = created(&["--json", "note", "create", "Linker"], "note");
    let release = created(&["--json", "release", "create", "v1"], "release");

    // A task, spec, plan and note share one ULID, so they share every suffix.
    let ulid = task.strip_prefix("arcl-t-").expect("a task ID");
    let (spec_twin, plan_twin, note_twin) = (
        format!("arcl-s-{ulid}"),
        format!("arcl-pl-{ulid}"),
        format!("arcl-n-{ulid}"),
    );
    import_record_copy(dir, "specs", &spec, &spec_twin);
    import_record_copy(dir, "plans", &plan, &plan_twin);
    import_record_copy(dir, "notes", &note, &note_twin);
    let suffix = &ulid[ulid.len() - 4..];

    for (kind, expected) in [
        ("spec", &spec_twin),
        ("plan", &plan_twin),
        ("task", &task),
        ("note", &note_twin),
    ] {
        let member = |action: &str| {
            run_json(
                dir,
                [
                    "--json",
                    "release",
                    "member",
                    action,
                    &release,
                    "--kind",
                    kind,
                    "--record-id",
                    suffix,
                ]
                .as_ref(),
            )
        };
        assert_eq!(
            member("add")["data"]["membership"]["record_id"],
            expected.as_str(),
            "{kind}"
        );
        assert_eq!(
            member("remove")["data"]["membership"]["record_id"],
            expected.as_str(),
            "{kind}"
        );
    }
    for (kind, expected) in [
        ("spec", &spec_twin),
        ("plan", &plan_twin),
        ("task", &task),
        ("note", &note_twin),
    ] {
        let link = |action: &str| {
            run_json(
                dir,
                [
                    "--json",
                    "note",
                    "link",
                    action,
                    &linker,
                    "--kind",
                    kind,
                    "--record-id",
                    suffix,
                ]
                .as_ref(),
            )
        };
        assert_eq!(link("add")["data"]["link"]["record_id"], expected.as_str(), "{kind}");
        assert_eq!(link("remove")["data"]["link"]["record_id"], expected.as_str(), "{kind}");
    }

    let under_spec = run_json(
        dir,
        ["--json", "task", "create", "Under spec", "--spec", suffix].as_ref(),
    );
    assert_eq!(under_spec["data"]["task"]["spec_id"], spec_twin.as_str());
    let under_plan = run_json(
        dir,
        ["--json", "task", "create", "Under plan", "--plan", suffix].as_ref(),
    );
    assert_eq!(under_plan["data"]["task"]["plan_id"], plan_twin.as_str());
    let child = run_json(dir, ["--json", "task", "create", "Child", "--parent", suffix].as_ref());
    assert_eq!(child["data"]["task"]["parent_id"], task.as_str());
    let blocked = create_task(dir, "Blocked", None);
    let dependency = run_json(
        dir,
        ["--json", "dependency", "add", &blocked, "--blocked-by", suffix].as_ref(),
    );
    assert_eq!(dependency["data"]["dependency"]["blocker_id"], task.as_str());
}

#[test]
fn input_that_starts_an_id_is_refused_and_a_capitalized_full_id_keeps_its_error() {
    let directory = initialized_directory();
    let dir = directory.path();
    let task = create_task(dir, "Task", None);
    let ulid = task.strip_prefix("arcl-t-").expect("a task ID");

    let refused = arcl_in(dir)
        .args(["task", "complete", &ulid[..4]])
        .output()
        .expect("complete runs");
    assert_eq!(refused.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("a short ID is the end of one"), "{stderr}");
    assert!(
        stderr.contains(&format!("{}  {task}  Task", &ulid[ulid.len() - 4..])),
        "{stderr}"
    );
    let shown = run_json(dir, ["--json", "task", "show", &task].as_ref());
    assert_eq!(
        shown["data"]["task"]["status"], "pending",
        "the refused command changed nothing"
    );

    let whole = run_json(dir, ["--json", "task", "show", ulid].as_ref());
    assert_eq!(
        whole["data"]["task"]["id"],
        task.as_str(),
        "a whole ULID still resolves"
    );

    let capitalized = task.to_uppercase();
    let refused = arcl_in(dir)
        .args(["task", "show", &capitalized])
        .output()
        .expect("show runs");
    assert_eq!(refused.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("expected prefix `arcl-t-`"), "{stderr}");
}

/// Run `task start <task> --claim [name]` with `ARCL_AGENT` set to `agent`, or
/// unset.
fn claim_output(directory: &Path, task: &str, name: Option<&str>, agent: Option<&str>) -> std::process::Output {
    let mut command = arcl_in(directory);
    command.args(["--json", "task", "start", task, "--claim"]);
    command.args(name);
    match agent {
        Some(agent) => command.env("ARCL_AGENT", agent),
        None => command.env_remove("ARCL_AGENT"),
    };
    command.output().expect("start runs")
}

#[test]
fn task_start_claim_sets_the_assignee_and_refuses_another_holder() {
    let directory = initialized_directory();
    let task = create_task(directory.path(), "Claimed", None);

    let claimed = claim_output(directory.path(), &task, Some("codex"), None);
    assert_success(&claimed);
    let claimed = json(&claimed.stdout);
    assert_eq!(claimed["data"]["task"]["status"], "in_progress");
    assert_eq!(claimed["data"]["task"]["assignee"], "codex");

    let again = claim_output(directory.path(), &task, Some("codex"), None);
    assert_success(&again);
    assert_eq!(json(&again.stdout)["data"]["task"]["assignee"], "codex");

    let refused = claim_output(directory.path(), &task, Some("claude"), None);
    assert_eq!(refused.status.code(), Some(4));
    assert!(refused.stdout.is_empty());
    let error = json(&refused.stderr);
    assert_eq!(error["error"]["code"], "conflict");
    assert!(
        error["error"]["message"]
            .as_str()
            .expect("error message")
            .contains("claimed by `codex`")
    );

    let shown = run_json(directory.path(), ["--json", "show", &task].as_ref());
    assert_eq!(shown["data"]["task"]["assignee"], "codex");
    let human = arcl_in(directory.path())
        .args(["--color", "never", "show", &task])
        .output()
        .expect("show runs");
    assert!(String::from_utf8_lossy(&human.stdout).contains("Assignee: codex"));

    let unclaimed = create_task(directory.path(), "Unclaimed", None);
    let shown = run_json(directory.path(), ["--json", "show", &unclaimed].as_ref());
    assert!(shown["data"]["task"].get("assignee").is_none());
}

#[test]
fn task_start_claim_takes_the_name_from_arcl_agent_then_git() {
    let directory = initialized_directory();
    let from_agent = create_task(directory.path(), "From agent", None);
    let from_git = create_task(directory.path(), "From Git", None);
    let unnamed = create_task(directory.path(), "Unnamed", None);

    let claimed = claim_output(directory.path(), &from_agent, None, Some("agent-7"));
    assert_success(&claimed);
    assert_eq!(json(&claimed.stdout)["data"]["task"]["assignee"], "agent-7");

    let refused = claim_output(directory.path(), &unnamed, None, None);
    assert_eq!(refused.status.code(), Some(3));
    let error = json(&refused.stderr);
    assert_eq!(error["error"]["code"], "invalid_request");
    assert!(
        error["error"]["message"]
            .as_str()
            .expect("error message")
            .contains("ARCL_AGENT")
    );
    let shown = run_json(directory.path(), ["--json", "show", &unnamed].as_ref());
    assert_eq!(shown["data"]["task"]["status"], "pending");

    let config = directory.path().join("gitconfig");
    fs::write(&config, "[user]\n\tname = Git Person\n").expect("Git config writes");
    let mut command = arcl_in(directory.path());
    let claimed = command
        .args(["--json", "task", "start", &from_git, "--claim"])
        .env_remove("ARCL_AGENT")
        .env("GIT_CONFIG_GLOBAL", &config)
        .output()
        .expect("start runs");
    assert_success(&claimed);
    assert_eq!(json(&claimed.stdout)["data"]["task"]["assignee"], "Git Person");
}

#[test]
fn park_handoff_complete_and_cancel_release_the_claim() {
    let directory = initialized_directory();
    for action in ["park", "handoff", "complete", "cancel"] {
        let task = create_task(directory.path(), action, None);
        assert_success(&claim_output(directory.path(), &task, Some("codex"), None));
        let mut args = vec!["--json", "task", action, task.as_str()];
        if action == "handoff" {
            args.extend(["--note", "Resume here"]);
        }
        let released = run_json(directory.path(), &args);
        assert!(released["data"]["task"].get("assignee").is_none(), "{action}");
        let shown = run_json(directory.path(), ["--json", "show", &task].as_ref());
        assert!(shown["data"]["task"].get("assignee").is_none(), "{action}");
    }
}

#[test]
fn list_filters_tasks_by_assignee() {
    let directory = initialized_directory();
    let mine = create_task(directory.path(), "Mine", None);
    let theirs = create_task(directory.path(), "Theirs", None);
    create_task(directory.path(), "Nobody's", None);
    assert_success(&claim_output(directory.path(), &mine, Some("codex"), None));
    assert_success(&claim_output(directory.path(), &theirs, Some("claude"), None));
    run_json(directory.path(), ["--json", "capture", "create", "Idea"].as_ref());

    let listed = run_json(directory.path(), ["--json", "list", "--assignee", "codex"].as_ref());
    let ids: Vec<&str> = listed["data"]["records"]
        .as_array()
        .expect("records")
        .iter()
        .map(|record| record["id"].as_str().expect("record ID"))
        .collect();
    assert_eq!(ids, vec![mine.as_str()]);

    let refused = arcl_in(directory.path())
        .args(["--json", "list", "--kind", "note", "--assignee", "codex"])
        .output()
        .expect("list runs");
    assert_eq!(refused.status.code(), Some(3));
}

#[test]
fn the_assignee_round_trips_through_the_workspace() {
    let source = tempdir().expect("temporary directory can be created");
    run_json(source.path(), ["--json", "init", "--workspace"].as_ref());
    let task = create_task(source.path(), "Claimed", None);
    assert_success(&claim_output(source.path(), &task, Some("codex"), None));
    assert_success(
        &arcl_in(source.path())
            .args(["workspace", "export"])
            .output()
            .expect("export runs"),
    );
    let record_path = format!(".arcl/workspace/tasks/{task}.md");
    let record = fs::read_to_string(source.path().join(&record_path)).expect("task record is exported");
    assert!(record.contains("assignee = \"codex\""), "record: {record}");

    let target = tempdir().expect("temporary directory can be created");
    run_json(target.path(), ["--json", "init", "--workspace"].as_ref());
    for file in [".arcl/workspace/manifest.toml", record_path.as_str()] {
        fs::copy(source.path().join(file), target.path().join(file)).expect("workspace file copies");
    }
    assert_success(
        &arcl_in(target.path())
            .args(["workspace", "import"])
            .output()
            .expect("import runs"),
    );
    let shown = run_json(target.path(), ["--json", "show", &task].as_ref());
    assert_eq!(shown["data"]["task"]["assignee"], "codex");
    assert_eq!(shown["data"]["task"]["status"], "in_progress");
}

#[test]
fn next_start_starts_the_task_next_shows() {
    let directory = initialized_directory();
    let dir = directory.path();
    let first = create_task(dir, "First", None);
    let second = create_task(dir, "Second", None);
    let shown = run_json(dir, ["--json", "next"].as_ref());
    assert_eq!(shown["data"]["task"]["id"], first.as_str());

    let started = run_json(dir, ["--json", "next", "--start"].as_ref());
    assert_eq!(started["data"]["task"]["id"], first.as_str());
    assert_eq!(started["data"]["task"]["status"], "in_progress");
    assert!(started["data"]["task"].get("assignee").is_none());

    let human = arcl_in(dir)
        .args(["--color", "never", "next", "--start"])
        .output()
        .expect("next runs");
    assert_success(&human);
    assert_eq!(
        String::from_utf8_lossy(&human.stdout).trim(),
        format!("Started task `{second}`: Second")
    );
    let shown = run_json(dir, ["--json", "show", &second].as_ref());
    assert_eq!(shown["data"]["task"]["status"], "in_progress");
}

#[test]
fn next_start_claim_claims_the_task_as_task_start_does() {
    let directory = initialized_directory();
    let dir = directory.path();
    let named = create_task(dir, "Named", None);
    let from_agent = create_task(dir, "From agent", None);

    let claimed = arcl_in(dir)
        .args(["--json", "next", "--start", "--claim", "codex"])
        .env_remove("ARCL_AGENT")
        .output()
        .expect("next runs");
    assert_success(&claimed);
    let claimed = json(&claimed.stdout);
    assert_eq!(claimed["data"]["task"]["id"], named.as_str());
    assert_eq!(claimed["data"]["task"]["assignee"], "codex");

    let claimed = arcl_in(dir)
        .args(["--json", "next", "--start", "--claim"])
        .env("ARCL_AGENT", "agent-7")
        .output()
        .expect("next runs");
    assert_success(&claimed);
    let claimed = json(&claimed.stdout);
    assert_eq!(claimed["data"]["task"]["id"], from_agent.as_str());
    assert_eq!(claimed["data"]["task"]["assignee"], "agent-7");

    let refused = arcl_in(dir)
        .args(["next", "--claim", "codex"])
        .output()
        .expect("next runs");
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("--start"));
}

#[test]
fn next_start_reports_nothing_to_start_and_exits_zero() {
    let directory = initialized_directory();
    let dir = directory.path();
    let blocker = create_task(dir, "Blocker", None);
    let blocked = create_task(dir, "Blocked", None);
    block_start_task(dir, &blocked, &blocker);
    run_json(dir, ["--json", "task", "start", &blocker].as_ref());

    let human = arcl_in(dir)
        .args(["--color", "never", "next", "--start"])
        .output()
        .expect("next runs");
    assert_success(&human);
    assert_eq!(
        String::from_utf8_lossy(&human.stdout).trim(),
        "Nothing to start: no task is ready."
    );

    let plain = arcl_in(dir)
        .args(["--plain", "next", "--start", "--claim", "codex"])
        .output()
        .expect("next runs");
    assert_success(&plain);
    assert!(plain.stdout.is_empty());

    let empty = run_json(dir, ["--json", "next", "--start"].as_ref());
    assert_eq!(empty["format_version"], 1);
    assert!(empty["data"]["task"].is_null());
    let shown = run_json(dir, ["--json", "show", &blocked].as_ref());
    assert_eq!(shown["data"]["task"]["status"], "pending");
}

#[test]
fn next_start_honours_the_next_filters() {
    let directory = initialized_directory();
    let dir = directory.path();
    let normal = create_task(dir, "Normal", None);
    let low = run_json(dir, ["--json", "task", "create", "Low", "--priority", "low"].as_ref())["data"]["task"]["id"]
        .as_str()
        .expect("task ID")
        .to_owned();

    let started = run_json(dir, ["--json", "next", "--start", "--priority", "low"].as_ref());
    assert_eq!(started["data"]["task"]["id"], low.as_str());
    let empty = run_json(dir, ["--json", "next", "--start", "--priority", "low"].as_ref());
    assert!(empty["data"]["task"].is_null());
    let shown = run_json(dir, ["--json", "show", &normal].as_ref());
    assert_eq!(shown["data"]["task"]["status"], "pending");
}

/// Spawn `count` `next --start --json` processes at once in `directory` and
/// return each one's started task ID, or `None` when it found nothing.
fn concurrent_next_starts(directory: &Path, count: usize) -> Vec<Option<String>> {
    let children: Vec<_> = (0..count)
        .map(|_| {
            isolated_command(assert_cmd::cargo::cargo_bin("arcl"), directory)
                .args(["--json", "next", "--start"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .expect("next spawns")
        })
        .collect();
    children
        .into_iter()
        .map(|child| {
            let output = child.wait_with_output().expect("next finishes");
            assert_success(&output);
            json(&output.stdout)["data"]["task"]["id"].as_str().map(str::to_owned)
        })
        .collect()
}

#[test]
fn concurrent_next_starts_never_start_the_same_task() {
    let directory = initialized_directory();
    let dir = directory.path();
    let first = create_task(dir, "First", None);
    let second = create_task(dir, "Second", None);
    let mut started: Vec<String> = concurrent_next_starts(dir, 2)
        .into_iter()
        .map(|id| id.expect("each call starts a task"))
        .collect();
    started.sort();
    let mut expected = vec![first, second];
    expected.sort();
    assert_eq!(started, expected);

    // With one ready task, one call starts it and the other finds nothing.
    let only = create_task(dir, "Only", None);
    let results = concurrent_next_starts(dir, 2);
    assert_eq!(results.iter().flatten().collect::<Vec<_>>(), vec![&only]);
    assert_eq!(results.iter().filter(|id| id.is_none()).count(), 1);
}
