use std::{fs, path::Path};

use arcl::{
    domain::ReleaseId,
    storage::Database,
    vcs::{GixVcs, Vcs},
};
use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn arcl_in(directory: &Path) -> Command {
    let mut command = Command::cargo_bin("arcl").expect("Cargo exposes the arcl binary");
    command.current_dir(directory);
    command
}

fn isolated_directory() -> TempDir {
    tempfile::tempdir().expect("temporary test directory can be created")
}

fn worktree(head: &str) -> TempDir {
    let directory = isolated_directory();
    let git_directory = directory.path().join(".git");
    fs::create_dir_all(git_directory.join("refs/heads")).expect("Git refs directory can be created");
    fs::create_dir_all(git_directory.join("objects/info")).expect("Git object directory can be created");
    fs::create_dir_all(git_directory.join("objects/pack")).expect("Git pack directory can be created");
    fs::write(
        git_directory.join("config"),
        "[core]\n\trepositoryformatversion = 0\n\tbare = false\n",
    )
    .expect("Git config can be written");
    fs::write(git_directory.join("HEAD"), head).expect("Git HEAD can be written");
    directory
}

fn bare_repository() -> TempDir {
    let directory = isolated_directory();
    fs::create_dir_all(directory.path().join("refs/heads")).expect("bare refs directory can be created");
    fs::create_dir_all(directory.path().join("objects/info")).expect("bare object directory can be created");
    fs::create_dir_all(directory.path().join("objects/pack")).expect("bare pack directory can be created");
    fs::write(
        directory.path().join("config"),
        "[core]\n\trepositoryformatversion = 0\n\tbare = true\n",
    )
    .expect("bare Git config can be written");
    fs::write(directory.path().join("HEAD"), "ref: refs/heads/main\n").expect("bare Git HEAD can be written");
    directory
}

fn initialized_repository() -> TempDir {
    let repository = worktree("ref: refs/heads/main\n");
    let output = arcl_in(repository.path())
        .args(["--color", "never", "init"])
        .output()
        .expect("arcl init runs");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    repository
}

fn idea_id_from_json(output: &[u8]) -> String {
    serde_json::from_slice::<Value>(output).expect("JSON output parses")["data"]["idea"]["id"]
        .as_str()
        .expect("JSON output contains an idea ID")
        .to_owned()
}

#[test]
fn help_succeeds_from_an_isolated_directory() {
    let directory = isolated_directory();
    let output = arcl_in(directory.path())
        .arg("--help")
        .output()
        .expect("arcl --help runs");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Arc Lightning"));
}

#[test]
fn version_succeeds_from_an_isolated_directory() {
    let directory = isolated_directory();
    let output = arcl_in(directory.path())
        .arg("--version")
        .output()
        .expect("arcl --version runs");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        format!("arcl {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn init_discovers_the_worktree_from_a_nested_directory() {
    let repository = worktree("ref: refs/heads/main\n");
    let nested = repository.path().join("src/nested");
    fs::create_dir_all(&nested).expect("nested directory can be created");
    let root_ignore = "root-specific-ignore\n";
    fs::write(repository.path().join(".gitignore"), root_ignore).expect("root ignore file can be written");

    let output = arcl_in(&nested)
        .args(["--color", "never", "init"])
        .output()
        .expect("arcl init runs");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Initialized Arc Lightning"));
    assert_eq!(
        fs::read_to_string(repository.path().join(".gitignore")).expect("root ignore is readable"),
        root_ignore
    );

    let arcl_directory = repository.path().join(".arcl");
    assert!(arcl_directory.join("config.toml").is_file());
    assert!(arcl_directory.join(".gitignore").is_file());
    assert!(arcl_directory.join("arcl.db").is_file());
    assert_eq!(
        fs::read_to_string(arcl_directory.join(".gitignore")).expect("scoped ignore is readable"),
        "/arcl.db\n/arcl.db-*\n/*.tmp\n/conflicts/\n"
    );

    let database = Database::open(arcl_directory.join("arcl.db")).expect("initialized database reopens");
    assert_eq!(database.schema_version().expect("schema version is readable"), 5);
}

#[test]
fn init_snapshot_option_is_persisted_without_replacing_existing_state() {
    let repository = worktree("ref: refs/heads/main\n");
    let output = arcl_in(repository.path())
        .args(["--color", "never", "init", "--snapshot"])
        .output()
        .expect("arcl init --snapshot runs");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let arcl_directory = repository.path().join(".arcl");
    let config_path = arcl_directory.join("config.toml");
    let config_before = fs::read_to_string(&config_path).expect("config is readable");
    assert!(config_before.contains("enabled = true"));

    let gitignore_path = arcl_directory.join(".gitignore");
    let mut gitignore_before = fs::read_to_string(&gitignore_path).expect("scoped ignore is readable");
    gitignore_before.insert_str(0, "# local customization\n");
    fs::write(&gitignore_path, &gitignore_before).expect("custom scoped ignore is writable");

    let database_path = arcl_directory.join("arcl.db");
    let connection = rusqlite::Connection::open(&database_path).expect("database is readable");
    connection
        .execute("INSERT INTO meta (key, value) VALUES ('custom', 'preserve-me')", [])
        .expect("custom database data can be written");
    drop(connection);

    let output = arcl_in(repository.path())
        .args(["--color", "never", "init"])
        .output()
        .expect("repeated arcl init runs");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(&config_path).expect("config is readable"),
        config_before
    );
    assert_eq!(
        fs::read_to_string(&gitignore_path).expect("scoped ignore is readable"),
        gitignore_before
    );

    let connection = rusqlite::Connection::open(database_path).expect("database is readable");
    let preserved: String = connection
        .query_row("SELECT value FROM meta WHERE key = 'custom'", [], |row| row.get(0))
        .expect("custom database data remains");
    assert_eq!(preserved, "preserve-me");
}

#[test]
fn init_outside_git_reports_an_invalid_project() {
    let directory = isolated_directory();
    let output = arcl_in(directory.path()).arg("init").output().expect("arcl init runs");

    assert_eq!(output.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&output.stderr).contains("could not discover a Git worktree"));
    assert!(!directory.path().join(".arcl").exists());
}

#[test]
fn init_in_a_bare_repository_reports_an_invalid_project() {
    let repository = bare_repository();
    let output = arcl_in(repository.path()).arg("init").output().expect("arcl init runs");

    assert_eq!(output.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&output.stderr).contains("is bare"));
    assert!(!repository.path().join(".arcl").exists());
}

#[test]
fn head_id_and_branch_name_handle_unborn_and_detached_heads() {
    let unborn = worktree("ref: refs/heads/main\n");
    let unborn_vcs = GixVcs::discover(unborn.path()).expect("unborn repository is discoverable");
    assert_eq!(unborn_vcs.head_id().expect("unborn HEAD is readable"), None);
    assert_eq!(
        unborn_vcs.branch_name().expect("unborn branch is readable").as_deref(),
        Some("main")
    );

    let detached = worktree("0123456789abcdef0123456789abcdef01234567\n");
    let detached_vcs = GixVcs::discover(detached.path()).expect("detached repository is discoverable");
    assert_eq!(
        detached_vcs.head_id().expect("detached HEAD is readable").as_deref(),
        Some("0123456789abcdef0123456789abcdef01234567")
    );
    assert_eq!(detached_vcs.branch_name().expect("detached branch is readable"), None);
}

#[test]
fn ideas_round_trip_through_create_update_discard_and_list() {
    let repository = initialized_repository();
    let created = arcl_in(repository.path())
        .args([
            "--json",
            "idea",
            "create",
            "First thought",
            "--description",
            "**Markdown**",
        ])
        .output()
        .expect("idea create runs");
    assert!(
        created.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&created.stderr)
    );

    let payload: Value = serde_json::from_slice(&created.stdout).expect("create JSON parses");
    assert_eq!(payload["format_version"], 1);
    assert_eq!(payload["data"]["action"], "created");
    assert_eq!(payload["data"]["idea"]["status"], "captured");
    assert_eq!(payload["data"]["idea"]["description"], "**Markdown**");
    let id = idea_id_from_json(&created.stdout);

    let database = rusqlite::Connection::open(repository.path().join(".arcl/arcl.db")).expect("database opens");
    let stored_description: String = database
        .query_row("SELECT description FROM ideas WHERE id = ?1", [&id], |row| row.get(0))
        .expect("created idea is stored");
    assert_eq!(stored_description, "**Markdown**");

    let description_path = repository.path().join("idea.md");
    let file_description = "# Updated\n\nUnicode: café 🚀\n";
    fs::write(&description_path, file_description).expect("description file writes");
    let updated = arcl_in(repository.path())
        .args([
            "--json",
            "idea",
            "update",
            &id,
            "--title",
            "Updated thought",
            "--description-file",
            description_path.to_str().expect("path is UTF-8"),
        ])
        .output()
        .expect("idea update runs");
    assert!(
        updated.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&updated.stderr)
    );
    let updated_payload: Value = serde_json::from_slice(&updated.stdout).expect("update JSON parses");
    assert_eq!(updated_payload["data"]["action"], "updated");
    assert_eq!(updated_payload["data"]["idea"]["description"], file_description);

    let discarded = arcl_in(repository.path())
        .args(["--quiet", "idea", "discard", &id])
        .output()
        .expect("idea discard runs");
    assert!(
        discarded.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&discarded.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&discarded.stdout).trim(), id);

    let repeated = arcl_in(repository.path())
        .args(["--plain", "idea", "discard", &id])
        .output()
        .expect("repeated idea discard runs");
    assert!(
        repeated.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&repeated.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&repeated.stdout).trim(), id);

    let listed = arcl_in(repository.path())
        .args(["--json", "idea", "list"])
        .output()
        .expect("idea list runs");
    assert!(
        listed.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let listed_payload: Value = serde_json::from_slice(&listed.stdout).expect("list JSON parses");
    assert_eq!(listed_payload["format_version"], 1);
    assert_eq!(listed_payload["data"]["ideas"][0]["id"], id);
    assert_eq!(listed_payload["data"]["ideas"][0]["status"], "discarded");
}

#[test]
fn idea_descriptions_accept_exact_utf8_stdin_and_invalid_inputs_do_not_write() {
    let repository = initialized_repository();
    let stdin_description = "## Piped\n\nMultiline café 🚀\n";
    let created = arcl_in(repository.path())
        .args(["--json", "idea", "create", "Piped thought", "--description-file", "-"])
        .write_stdin(stdin_description)
        .output()
        .expect("stdin idea create runs");
    assert!(
        created.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&created.stderr)
    );

    let id = idea_id_from_json(&created.stdout);
    let database_path = repository.path().join(".arcl/arcl.db");
    let database = rusqlite::Connection::open(&database_path).expect("database opens");
    let count_before: i64 = database
        .query_row("SELECT count(*) FROM ideas", [], |row| row.get(0))
        .expect("idea count is readable");
    let stored: String = database
        .query_row("SELECT description FROM ideas WHERE id = ?1", [&id], |row| row.get(0))
        .expect("stdin idea is stored");
    assert_eq!(stored, stdin_description);
    drop(database);

    let empty_title = arcl_in(repository.path())
        .args(["idea", "create", "   ", "--description", "should not write"])
        .output()
        .expect("invalid title command runs");
    assert_eq!(empty_title.status.code(), Some(3));

    let malformed_id = arcl_in(repository.path())
        .args(["idea", "discard", "arcl-i-not-a-ulid"])
        .output()
        .expect("malformed ID command runs");
    assert_eq!(malformed_id.status.code(), Some(3));

    let ambiguous = arcl_in(repository.path())
        .args([
            "idea",
            "create",
            "Ambiguous",
            "--description",
            "inline",
            "--description-file",
            "-",
        ])
        .output()
        .expect("ambiguous description command runs");
    assert_eq!(ambiguous.status.code(), Some(2));

    let database = rusqlite::Connection::open(database_path).expect("database reopens");
    let count_after: i64 = database
        .query_row("SELECT count(*) FROM ideas", [], |row| row.get(0))
        .expect("idea count is readable");
    assert_eq!(count_after, count_before);
}

fn release_id_from_json(output: &[u8]) -> String {
    serde_json::from_slice::<Value>(output).expect("JSON output parses")["data"]["release"]["id"]
        .as_str()
        .expect("JSON output contains a release ID")
        .to_owned()
}

fn epic_id_from_json(output: &[u8]) -> String {
    serde_json::from_slice::<Value>(output).expect("JSON output parses")["data"]["epic"]["id"]
        .as_str()
        .expect("JSON output contains an epic ID")
        .to_owned()
}

fn milestone_id_from_json(output: &[u8]) -> String {
    serde_json::from_slice::<Value>(output).expect("JSON output parses")["data"]["milestone"]["id"]
        .as_str()
        .expect("JSON output contains a milestone ID")
        .to_owned()
}

fn task_id_from_json(output: &[u8]) -> String {
    serde_json::from_slice::<Value>(output).expect("JSON output parses")["data"]["task"]["id"]
        .as_str()
        .expect("JSON output contains a task ID")
        .to_owned()
}

#[test]
fn releases_and_epics_round_trip_from_a_nested_directory() {
    let repository = worktree("ref: refs/heads/main\n");
    let specs = repository.path().join("specs");
    fs::create_dir_all(&specs).expect("spec directory can be created");
    let spec = specs.join("feature.md");
    let spec_contents = "# Feature\n\nKeep this file unchanged.\n";
    fs::write(&spec, spec_contents).expect("spec can be written");

    let initialized = arcl_in(repository.path())
        .args(["--color", "never", "init"])
        .output()
        .expect("arcl init runs");
    assert!(initialized.status.success());

    let release = arcl_in(repository.path())
        .args([
            "--json",
            "release",
            "create",
            "Spring release",
            "--description",
            "Ship it.",
        ])
        .output()
        .expect("release create runs");
    assert!(
        release.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&release.stderr)
    );
    let release_id = release_id_from_json(&release.stdout);

    let release_update = arcl_in(repository.path())
        .args(["--json", "release", "update", &release_id, "--title", "Updated release"])
        .output()
        .expect("release update runs");
    assert!(
        release_update.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&release_update.stderr)
    );
    let release_payload: Value = serde_json::from_slice(&release_update.stdout).expect("release update JSON parses");
    assert_eq!(release_payload["data"]["release"]["title"], "Updated release");
    assert_eq!(release_payload["data"]["release"]["description"], "Ship it.");

    let epic = arcl_in(&specs)
        .args([
            "--json",
            "epic",
            "create",
            "Feature epic",
            "--spec",
            "feature.md",
            "--release",
            &release_id,
            "--description",
            "Track the feature.",
        ])
        .output()
        .expect("epic create runs");
    assert!(
        epic.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&epic.stderr)
    );
    let epic_payload: Value = serde_json::from_slice(&epic.stdout).expect("epic JSON parses");
    let epic_id = epic_id_from_json(&epic.stdout);
    assert_eq!(epic_payload["data"]["epic"]["spec_path"], "specs/feature.md");
    assert_eq!(epic_payload["data"]["epic"]["release_id"], release_id);

    let updated = arcl_in(&specs)
        .args([
            "--json",
            "epic",
            "update",
            &epic_id,
            "--title",
            "Renamed feature epic",
            "--description",
            "Updated tracker text.",
            "--no-release",
        ])
        .output()
        .expect("epic update runs");
    assert!(
        updated.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&updated.stderr)
    );
    let updated_payload: Value = serde_json::from_slice(&updated.stdout).expect("epic update JSON parses");
    assert_eq!(updated_payload["data"]["epic"]["title"], "Renamed feature epic");
    assert_eq!(updated_payload["data"]["epic"]["description"], "Updated tracker text.");
    assert!(updated_payload["data"]["epic"]["release_id"].is_null());
    assert_eq!(updated_payload["data"]["epic"]["spec_path"], "specs/feature.md");
    assert_eq!(fs::read_to_string(&spec).expect("spec remains readable"), spec_contents);

    let database = rusqlite::Connection::open(repository.path().join(".arcl/arcl.db")).expect("database opens");
    let stored_path: String = database
        .query_row("SELECT spec_path FROM epics WHERE id = ?1", [&epic_id], |row| {
            row.get(0)
        })
        .expect("epic spec path is stored");
    assert_eq!(stored_path, "specs/feature.md");
}

#[test]
fn epic_release_association_failures_roll_back_mutations() {
    let repository = initialized_repository();
    let spec = repository.path().join("feature.md");
    fs::write(&spec, "# Feature\n").expect("spec can be written");
    let missing_release = ReleaseId::new().to_string();

    let create = arcl_in(repository.path())
        .args([
            "epic",
            "create",
            "Feature",
            "--spec",
            "feature.md",
            "--release",
            &missing_release,
        ])
        .output()
        .expect("invalid epic create runs");
    assert_eq!(create.status.code(), Some(5));
    assert!(String::from_utf8_lossy(&create.stderr).contains("release"));

    let database_path = repository.path().join(".arcl/arcl.db");
    let database = rusqlite::Connection::open(&database_path).expect("database opens");
    let count: i64 = database
        .query_row("SELECT count(*) FROM epics", [], |row| row.get(0))
        .expect("epic count is readable");
    assert_eq!(count, 0);
    drop(database);

    let release = arcl_in(repository.path())
        .args(["--json", "release", "create", "Release"])
        .output()
        .expect("release create runs");
    assert!(release.status.success());
    let release_id = release_id_from_json(&release.stdout);
    let created = arcl_in(repository.path())
        .args([
            "--json",
            "epic",
            "create",
            "Feature",
            "--spec",
            "feature.md",
            "--release",
            &release_id,
        ])
        .output()
        .expect("epic create runs");
    assert!(
        created.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&created.stderr)
    );
    let epic_id = epic_id_from_json(&created.stdout);

    let failed_update = arcl_in(repository.path())
        .args([
            "epic",
            "update",
            &epic_id,
            "--title",
            "Must not be saved",
            "--release",
            &missing_release,
        ])
        .output()
        .expect("invalid epic update runs");
    assert_eq!(failed_update.status.code(), Some(5));

    let database = rusqlite::Connection::open(database_path).expect("database reopens");
    let (title, stored_release): (String, String) = database
        .query_row("SELECT title, release_id FROM epics WHERE id = ?1", [&epic_id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .expect("epic remains readable");
    assert_eq!(title, "Feature");
    assert_eq!(stored_release, release_id);
}

#[test]
fn epic_rejects_invalid_spec_paths_and_duplicates() {
    let repository = initialized_repository();
    let spec = repository.path().join("feature.md");
    let text_file = repository.path().join("notes.txt");
    fs::write(&spec, "# Feature\n").expect("spec can be written");
    fs::write(&text_file, "not a spec\n").expect("text file can be written");

    let absolute = arcl_in(repository.path())
        .args([
            "epic",
            "create",
            "Absolute",
            "--spec",
            spec.to_str().expect("spec path is UTF-8"),
        ])
        .output()
        .expect("absolute spec command runs");
    assert_eq!(absolute.status.code(), Some(3));

    let traversal = arcl_in(repository.path())
        .args(["epic", "create", "Traversal", "--spec", "../feature.md"])
        .output()
        .expect("traversal spec command runs");
    assert_eq!(traversal.status.code(), Some(3));

    let non_markdown = arcl_in(repository.path())
        .args(["epic", "create", "Text", "--spec", "notes.txt"])
        .output()
        .expect("non-Markdown spec command runs");
    assert_eq!(non_markdown.status.code(), Some(3));

    let missing = arcl_in(repository.path())
        .args(["epic", "create", "Missing", "--spec", "missing.md"])
        .output()
        .expect("missing spec command runs");
    assert_eq!(missing.status.code(), Some(3));

    let created = arcl_in(repository.path())
        .args(["--json", "epic", "create", "First", "--spec", "feature.md"])
        .output()
        .expect("first epic create runs");
    assert!(
        created.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&created.stderr)
    );

    let duplicate = arcl_in(repository.path())
        .args(["epic", "create", "Duplicate", "--spec", "./feature.md"])
        .output()
        .expect("duplicate spec command runs");
    assert_eq!(duplicate.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("already linked"));

    let database = rusqlite::Connection::open(repository.path().join(".arcl/arcl.db")).expect("database opens");
    let count: i64 = database
        .query_row("SELECT count(*) FROM epics", [], |row| row.get(0))
        .expect("epic count is readable");
    assert_eq!(count, 1);
}

#[cfg(unix)]
#[test]
fn epic_rejects_a_symlink_escape() {
    use std::os::unix::fs::symlink;

    let repository = initialized_repository();
    let outside = tempfile::tempdir().expect("outside directory can be created");
    let outside_spec = outside.path().join("outside.md");
    fs::write(&outside_spec, "# Outside\n").expect("outside spec can be written");
    symlink(&outside_spec, repository.path().join("escape.md")).expect("symlink can be created");

    let output = arcl_in(repository.path())
        .args(["epic", "create", "Escape", "--spec", "escape.md"])
        .output()
        .expect("symlink escape command runs");
    assert_eq!(output.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&output.stderr).contains("outside the Git worktree"));
}

#[test]
fn milestones_tasks_and_subtasks_round_trip_with_atomic_moves() {
    let repository = initialized_repository();
    fs::write(repository.path().join("feature.md"), "# Feature\n").expect("spec can be written");
    let epic = arcl_in(repository.path())
        .args(["--json", "epic", "create", "Feature", "--spec", "feature.md"])
        .output()
        .expect("epic create runs");
    assert!(
        epic.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&epic.stderr)
    );
    let epic_id = epic_id_from_json(&epic.stdout);

    let milestone = arcl_in(repository.path())
        .args([
            "--json",
            "milestone",
            "create",
            "Foundation",
            "--epic",
            &epic_id,
            "--position",
            "10",
        ])
        .output()
        .expect("milestone create runs");
    assert!(
        milestone.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&milestone.stderr)
    );
    let first_milestone = milestone_id_from_json(&milestone.stdout);
    let second = arcl_in(repository.path())
        .args([
            "--json",
            "milestone",
            "create",
            "Follow-up",
            "--epic",
            &epic_id,
            "--position",
            "10",
        ])
        .output()
        .expect("second milestone create runs");
    assert!(
        second.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_milestone = milestone_id_from_json(&second.stdout);

    let parent = arcl_in(repository.path())
        .args([
            "--json",
            "task",
            "create",
            "Parent",
            "--milestone",
            &first_milestone,
            "--priority",
            "high",
            "--position",
            "10",
        ])
        .output()
        .expect("task create runs");
    assert!(
        parent.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&parent.stderr)
    );
    let parent_id = task_id_from_json(&parent.stdout);
    let child = arcl_in(repository.path())
        .args([
            "--json",
            "task",
            "create",
            "Child",
            "--milestone",
            &first_milestone,
            "--parent",
            &parent_id,
            "--description",
            "- [ ] prose only",
        ])
        .output()
        .expect("subtask create runs");
    assert!(
        child.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&child.stderr)
    );
    let child_id = task_id_from_json(&child.stdout);
    assert_eq!(
        serde_json::from_slice::<Value>(&child.stdout).expect("child JSON parses")["data"]["task"]["parent_id"],
        parent_id
    );

    let moved = arcl_in(repository.path())
        .args([
            "--json",
            "task",
            "update",
            &parent_id,
            "--milestone",
            &second_milestone,
            "--no-parent",
        ])
        .output()
        .expect("task subtree move runs");
    assert!(
        moved.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&moved.stderr)
    );
    let moved_payload: Value = serde_json::from_slice(&moved.stdout).expect("move JSON parses");
    assert_eq!(moved_payload["data"]["task"]["milestone_id"], second_milestone);

    let database = rusqlite::Connection::open(repository.path().join(".arcl/arcl.db")).expect("database opens");
    let moved_child_milestone: String = database
        .query_row("SELECT milestone_id FROM tasks WHERE id = ?1", [&child_id], |row| {
            row.get(0)
        })
        .expect("child milestone is readable");
    assert_eq!(moved_child_milestone, second_milestone);
    drop(database);

    let rejected = arcl_in(repository.path())
        .args(["task", "update", &parent_id, "--parent", &child_id])
        .output()
        .expect("cyclic reparent command runs");
    assert_eq!(rejected.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("cycle"));
}
