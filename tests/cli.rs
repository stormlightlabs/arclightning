use std::{fs, path::Path};

use arcl::{
    domain::{IdeaId, ReleaseId, TaskId},
    storage::{CURRENT_VERSION, Database},
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
    record_id_from_json(output, "idea")
}

fn record_id_from_json(output: &[u8], record: &str) -> String {
    serde_json::from_slice::<Value>(output).expect("JSON output parses")["data"][record]["id"]
        .as_str()
        .expect("JSON output contains a record ID")
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
fn project_discovery_uses_the_git_worktree_for_nested_initialization() {
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
    assert_eq!(
        database.schema_version().expect("schema version is readable"),
        CURRENT_VERSION
    );
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
    let snapshot_directory = arcl_directory.join("snapshot");
    assert_eq!(
        fs::read_to_string(snapshot_directory.join("manifest.toml")).expect("manifest is readable"),
        "format-version = 1\n"
    );
    for directory in ["ideas", "releases", "epics", "milestones", "tasks"] {
        assert!(snapshot_directory.join(directory).is_dir());
    }

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
        .expect("custom database data exists");
    assert_eq!(preserved, "preserve-me");
}

#[test]
fn snapshot_export_is_deterministic_and_records_the_successful_base() {
    let repository = worktree("ref: refs/heads/main\n");
    let initialized = arcl_in(repository.path())
        .args(["--color", "never", "init", "--snapshot"])
        .output()
        .expect("arcl init --snapshot runs");
    assert!(initialized.status.success());

    let created = arcl_in(repository.path())
        .args(["--json", "idea", "create", "Exported idea", "--description", "Details"])
        .output()
        .expect("idea create runs");
    assert!(created.status.success());
    let idea_id = idea_id_from_json(&created.stdout);

    let first = arcl_in(repository.path())
        .args(["--quiet", "snapshot", "export"])
        .output()
        .expect("snapshot export runs");
    assert!(
        first.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );

    let snapshot = repository.path().join(".arcl/snapshot");
    let manifest = fs::read(snapshot.join("manifest.toml")).expect("manifest is readable");
    let idea = fs::read(snapshot.join("ideas").join(format!("{idea_id}.md"))).expect("idea record is readable");
    let base = Database::open(repository.path().join(".arcl/arcl.db"))
        .expect("database reopens")
        .snapshot_base()
        .expect("snapshot base is readable");
    assert!(
        base.iter()
            .any(|file| file.path == "manifest.toml" && file.content == manifest)
    );
    assert!(
        base.iter()
            .any(|file| file.path == format!("ideas/{idea_id}.md") && file.content == idea)
    );

    let second = arcl_in(repository.path())
        .args(["--quiet", "snapshot", "export"])
        .output()
        .expect("second snapshot export runs");
    assert!(
        second.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(
        fs::read(snapshot.join("manifest.toml")).expect("manifest is readable"),
        manifest
    );
    assert_eq!(
        fs::read(snapshot.join("ideas").join(format!("{idea_id}.md"))).expect("idea record is readable"),
        idea
    );
}

#[test]
fn project_discovery_supports_non_git_projects_from_descendants() {
    let directory = isolated_directory();
    let initialized = arcl_in(directory.path())
        .args(["--color", "never", "init"])
        .output()
        .expect("arcl init runs");
    assert!(
        initialized.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&initialized.stderr)
    );
    assert!(directory.path().join(".arcl/config.toml").is_file());
    assert!(directory.path().join(".arcl/arcl.db").is_file());

    let nested = directory.path().join("src/nested");
    fs::create_dir_all(&nested).expect("nested directory can be created");
    fs::write(nested.join("feature.md"), "# Feature\n").expect("spec can be written");

    let idea = arcl_in(&nested)
        .args(["--json", "idea", "create", "A local thought"])
        .output()
        .expect("idea create runs");
    assert!(
        idea.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&idea.stderr)
    );

    let release = arcl_in(&nested)
        .args(["--json", "release", "create", "Local release"])
        .output()
        .expect("release create runs");
    assert!(
        release.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&release.stderr)
    );
    let release_id = record_id_from_json(&release.stdout, "release");

    let epic = arcl_in(&nested)
        .args([
            "--json",
            "epic",
            "create",
            "Local epic",
            "--spec",
            "feature.md",
            "--release",
            &release_id,
        ])
        .output()
        .expect("epic create runs");
    assert!(
        epic.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&epic.stderr)
    );
    let epic_id = record_id_from_json(&epic.stdout, "epic");

    let milestone = arcl_in(&nested)
        .args(["--json", "milestone", "create", "Local milestone", "--epic", &epic_id])
        .output()
        .expect("milestone create runs");
    assert!(
        milestone.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&milestone.stderr)
    );
    let milestone_id = record_id_from_json(&milestone.stdout, "milestone");

    let task = arcl_in(&nested)
        .args(["--json", "task", "create", "Local task", "--milestone", &milestone_id])
        .output()
        .expect("task create runs");
    assert!(
        task.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&task.stderr)
    );

    let ready = arcl_in(&nested).args(["--json", "ready"]).output().expect("ready runs");
    assert!(
        ready.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&ready.stderr)
    );
    let ready_json: Value = serde_json::from_slice(&ready.stdout).expect("ready JSON parses");
    assert_eq!(
        ready_json["data"]["tasks"].as_array().expect("ready tasks exist").len(),
        1
    );
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
    assert_eq!(fs::read_to_string(&spec).expect("spec is readable"), spec_contents);

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
        .expect("epic is readable");
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
    assert!(String::from_utf8_lossy(&output.stderr).contains("outside the project root"));
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

#[test]
fn lifecycle_commands_enforce_transitions_guards_and_non_cascading_overrides() {
    let repository = initialized_repository();
    fs::write(repository.path().join("feature.md"), "# Feature\n").expect("spec can be written");

    let release = arcl_in(repository.path())
        .args(["--json", "release", "create", "Release"])
        .output()
        .expect("release create runs");
    let release_id = release_id_from_json(&release.stdout);
    let epic = arcl_in(repository.path())
        .args([
            "--json",
            "epic",
            "create",
            "Epic",
            "--spec",
            "feature.md",
            "--release",
            &release_id,
        ])
        .output()
        .expect("epic create runs");
    let epic_id = epic_id_from_json(&epic.stdout);
    let milestone = arcl_in(repository.path())
        .args(["--json", "milestone", "create", "Milestone", "--epic", &epic_id])
        .output()
        .expect("milestone create runs");
    let milestone_id = milestone_id_from_json(&milestone.stdout);
    let task = arcl_in(repository.path())
        .args(["--json", "task", "create", "Task", "--milestone", &milestone_id])
        .output()
        .expect("task create runs");
    let task_id = task_id_from_json(&task.stdout);

    for (command, expected_action, expected_status) in [
        ("park", "parked", "parked"),
        ("unpark", "unparked", "pending"),
        ("start", "started", "in_progress"),
        ("complete", "completed", "completed"),
        ("complete", "completed", "completed"),
    ] {
        let output = arcl_in(repository.path())
            .args(["--json", "task", command, &task_id])
            .output()
            .expect("task lifecycle command runs");
        assert!(
            output.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let payload: Value = serde_json::from_slice(&output.stdout).expect("lifecycle JSON parses");
        assert_eq!(payload["data"]["action"], expected_action);
        assert_eq!(payload["data"]["task"]["status"], expected_status);
    }
    let terminal_change = arcl_in(repository.path())
        .args(["task", "cancel", &task_id])
        .output()
        .expect("invalid terminal change runs");
    assert_eq!(terminal_change.status.code(), Some(3));

    let parent = arcl_in(repository.path())
        .args(["--json", "task", "create", "Parent", "--milestone", &milestone_id])
        .output()
        .expect("parent create runs");
    let parent_id = task_id_from_json(&parent.stdout);
    let child = arcl_in(repository.path())
        .args([
            "--json",
            "task",
            "create",
            "Child",
            "--milestone",
            &milestone_id,
            "--parent",
            &parent_id,
        ])
        .output()
        .expect("child create runs");
    let child_id = task_id_from_json(&child.stdout);

    let guarded = arcl_in(repository.path())
        .args(["task", "cancel", &parent_id])
        .output()
        .expect("guarded cancellation runs");
    assert_eq!(guarded.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&guarded.stderr).contains("non-terminal descendants"));
    let overridden = arcl_in(repository.path())
        .args(["--json", "task", "cancel", &parent_id, "--allow-open-children"])
        .output()
        .expect("override cancellation runs");
    assert!(overridden.status.success());

    for (kind, action, id) in [
        ("milestone", "complete", milestone_id.as_str()),
        ("epic", "cancel", epic_id.as_str()),
        ("release", "complete", release_id.as_str()),
    ] {
        let guarded = arcl_in(repository.path())
            .args([kind, action, id])
            .output()
            .expect("guarded container transition runs");
        assert_eq!(guarded.status.code(), Some(3));
        let overridden = arcl_in(repository.path())
            .args(["--json", kind, action, id, "--allow-open-children"])
            .output()
            .expect("container override runs");
        assert!(
            overridden.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&overridden.stderr)
        );
    }

    let database = rusqlite::Connection::open(repository.path().join(".arcl/arcl.db")).expect("database opens");
    let child_status: String = database
        .query_row("SELECT status FROM tasks WHERE id = ?1", [&child_id], |row| row.get(0))
        .expect("child status reads");
    assert_eq!(child_status, "pending");
}

#[test]
fn dependency_and_ready_commands_compute_and_render_derived_work() {
    let repository = initialized_repository();
    fs::write(repository.path().join("feature.md"), "# Feature\n").expect("spec can be written");

    let epic = arcl_in(repository.path())
        .args(["--json", "epic", "create", "Epic", "--spec", "feature.md"])
        .output()
        .expect("epic create runs");
    assert!(epic.status.success());
    let epic_id = epic_id_from_json(&epic.stdout);
    let milestone = arcl_in(repository.path())
        .args(["--json", "milestone", "create", "Milestone", "--epic", &epic_id])
        .output()
        .expect("milestone create runs");
    assert!(milestone.status.success());
    let milestone_id = milestone_id_from_json(&milestone.stdout);

    let blocker = arcl_in(repository.path())
        .args(["--json", "task", "create", "Blocker", "--milestone", &milestone_id])
        .output()
        .expect("blocker create runs");
    assert!(blocker.status.success());
    let blocker_id = task_id_from_json(&blocker.stdout);
    let blocked = arcl_in(repository.path())
        .args(["--json", "task", "create", "Blocked", "--milestone", &milestone_id])
        .output()
        .expect("blocked task create runs");
    assert!(blocked.status.success());
    let blocked_id = task_id_from_json(&blocked.stdout);

    let added = arcl_in(repository.path())
        .args(["--json", "dependency", "add", &blocked_id, "--blocked-by", &blocker_id])
        .output()
        .expect("dependency add runs");
    assert!(
        added.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&added.stderr)
    );
    let added_payload: Value = serde_json::from_slice(&added.stdout).expect("dependency JSON parses");
    assert_eq!(added_payload["data"]["action"], "added");
    assert_eq!(added_payload["data"]["dependency"]["task_id"], blocked_id);

    let duplicate = arcl_in(repository.path())
        .args(["dependency", "add", &blocked_id, "--blocked-by", &blocker_id])
        .output()
        .expect("duplicate dependency command runs");
    assert_eq!(duplicate.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("already blocked"));

    let ready_before = arcl_in(repository.path())
        .args(["--plain", "ready"])
        .output()
        .expect("ready query runs");
    assert!(ready_before.status.success());
    assert_eq!(String::from_utf8_lossy(&ready_before.stdout).trim(), blocker_id);

    let next_before = arcl_in(repository.path())
        .args(["--json", "next"])
        .output()
        .expect("next query runs");
    assert!(next_before.status.success());
    let next_payload: Value = serde_json::from_slice(&next_before.stdout).expect("next JSON parses");
    assert_eq!(next_payload["data"]["task"]["id"], blocker_id);

    let removed = arcl_in(repository.path())
        .args([
            "--json",
            "dependency",
            "remove",
            &blocked_id,
            "--blocked-by",
            &blocker_id,
        ])
        .output()
        .expect("dependency remove runs");
    assert!(removed.status.success());
    let removed_payload: Value = serde_json::from_slice(&removed.stdout).expect("remove JSON parses");
    assert_eq!(removed_payload["data"]["action"], "removed");

    let readded = arcl_in(repository.path())
        .args(["dependency", "add", &blocked_id, "--blocked-by", &blocker_id])
        .output()
        .expect("dependency re-add runs");
    assert!(readded.status.success());
    let completed = arcl_in(repository.path())
        .args(["task", "complete", &blocker_id])
        .output()
        .expect("blocker completion runs");
    assert!(completed.status.success());
    let ready_after = arcl_in(repository.path())
        .args(["--plain", "ready"])
        .output()
        .expect("ready query after completion runs");
    assert_eq!(String::from_utf8_lossy(&ready_after.stdout).trim(), blocked_id);

    let completed = arcl_in(repository.path())
        .args(["task", "complete", &blocked_id])
        .output()
        .expect("blocked task completion runs");
    assert!(completed.status.success());
    let empty_next = arcl_in(repository.path())
        .args(["--json", "next"])
        .output()
        .expect("empty next query runs");
    assert!(empty_next.status.success());
    let empty_payload: Value = serde_json::from_slice(&empty_next.stdout).expect("empty next JSON parses");
    assert!(empty_payload["data"]["task"].is_null());
}

#[test]
fn promotion_is_idempotent_and_query_commands_expose_provenance() {
    let repository = initialized_repository();
    fs::write(repository.path().join("feature.md"), "# Feature\n").expect("spec can be written");
    let idea = arcl_in(repository.path())
        .args(["--json", "idea", "create", "Promotable"])
        .output()
        .expect("idea create runs");
    let idea_id = idea_id_from_json(&idea.stdout);
    let first = arcl_in(repository.path())
        .args(["--json", "idea", "promote", &idea_id, "--spec", "feature.md"])
        .output()
        .expect("promotion runs");
    assert!(
        first.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_payload: Value = serde_json::from_slice(&first.stdout).expect("promotion JSON parses");
    let epic_id = first_payload["data"]["epic"]["id"]
        .as_str()
        .expect("epic ID")
        .to_owned();
    let second = arcl_in(repository.path())
        .args(["--json", "idea", "promote", &idea_id, "--spec", "feature.md"])
        .output()
        .expect("repeated promotion runs");
    assert!(second.status.success());
    let second_payload: Value = serde_json::from_slice(&second.stdout).expect("promotion JSON parses");
    assert_eq!(second_payload["data"]["epic"]["id"], epic_id);
    assert_eq!(second_payload["data"]["idea"]["promoted_to"], epic_id);

    let shown_idea = arcl_in(repository.path())
        .args(["--json", "show", &idea_id])
        .output()
        .expect("idea show runs");
    assert_eq!(
        serde_json::from_slice::<Value>(&shown_idea.stdout).expect("show JSON parses")["data"]["record"]["promoted_to"],
        epic_id
    );
    let shown_epic = arcl_in(repository.path())
        .args(["--json", "show", &epic_id])
        .output()
        .expect("epic show runs");
    assert_eq!(
        serde_json::from_slice::<Value>(&shown_epic.stdout).expect("show JSON parses")["data"]["record"]["source_idea"],
        idea_id
    );
    let listed = arcl_in(repository.path())
        .args(["--json", "list", "--kind", "epic"])
        .output()
        .expect("list runs");
    let listed_payload: Value = serde_json::from_slice(&listed.stdout).expect("list JSON parses");
    assert_eq!(listed_payload["data"]["records"].as_array().expect("records").len(), 1);
    assert_eq!(listed_payload["data"]["records"][0]["source_idea"], idea_id);
}

#[test]
fn query_tree_explain_and_check_report_derived_graph_state() {
    let repository = initialized_repository();
    fs::write(repository.path().join("feature.md"), "# Feature\n").expect("spec can be written");
    let epic = arcl_in(repository.path())
        .args(["--json", "epic", "create", "Epic", "--spec", "feature.md"])
        .output()
        .expect("epic create runs");
    let epic_id = epic_id_from_json(&epic.stdout);
    let milestone = arcl_in(repository.path())
        .args(["--json", "milestone", "create", "Milestone", "--epic", &epic_id])
        .output()
        .expect("milestone create runs");
    let milestone_id = milestone_id_from_json(&milestone.stdout);
    let blocker = arcl_in(repository.path())
        .args(["--json", "task", "create", "Blocker", "--milestone", &milestone_id])
        .output()
        .expect("blocker create runs");
    let blocker_id = task_id_from_json(&blocker.stdout);
    let task = arcl_in(repository.path())
        .args(["--json", "task", "create", "Blocked", "--milestone", &milestone_id])
        .output()
        .expect("task create runs");
    let task_id = task_id_from_json(&task.stdout);
    arcl_in(repository.path())
        .args(["dependency", "add", &task_id, "--blocked-by", &blocker_id])
        .output()
        .expect("dependency add runs");

    let explain = arcl_in(repository.path())
        .args(["--json", "explain", &task_id])
        .output()
        .expect("explain runs");
    let explain_payload: Value = serde_json::from_slice(&explain.stdout).expect("explain JSON parses");
    assert!(
        !explain_payload["data"]["readiness"]["ready"]
            .as_bool()
            .expect("ready field")
    );
    assert!(
        explain_payload["data"]["readiness"]["reasons"]
            .as_array()
            .expect("reasons")
            .iter()
            .any(|reason| reason.as_str().unwrap_or_default().contains(&blocker_id))
    );

    let tree = arcl_in(repository.path())
        .args(["--json", "tree", &epic_id])
        .output()
        .expect("tree runs");
    let tree_payload: Value = serde_json::from_slice(&tree.stdout).expect("tree JSON parses");
    assert_eq!(
        tree_payload["data"]["nodes"][0]["children"][0]["children"][0]["id"],
        blocker_id
    );

    let checked = arcl_in(repository.path())
        .args(["--json", "check"])
        .output()
        .expect("check runs");
    assert!(
        checked.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let check_payload: Value = serde_json::from_slice(&checked.stdout).expect("check JSON parses");
    assert!(check_payload["data"]["valid"].as_bool().expect("valid field"));
    assert!(
        check_payload["data"]["warnings"]
            .as_array()
            .expect("warnings")
            .iter()
            .any(|warning| warning.as_str().unwrap_or_default().contains("duplicate task position"))
    );
}

#[test]
fn query_failures_distinguish_invalid_filters_from_missing_records() {
    let repository = initialized_repository();

    let invalid_kind = arcl_in(repository.path())
        .args(["list", "--kind", "unknown"])
        .output()
        .expect("invalid kind query runs");
    assert_eq!(invalid_kind.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&invalid_kind.stderr).contains("invalid list filter"));

    let malformed_release = arcl_in(repository.path())
        .args(["list", "--release", "not-a-release-id"])
        .output()
        .expect("malformed release query runs");
    assert_eq!(malformed_release.status.code(), Some(3));

    let missing_release = ReleaseId::new().to_string();
    let missing = arcl_in(repository.path())
        .args(["list", "--release", &missing_release])
        .output()
        .expect("missing release query runs");
    assert_eq!(missing.status.code(), Some(5));
    assert!(String::from_utf8_lossy(&missing.stderr).contains(&missing_release));

    let missing_ready = arcl_in(repository.path())
        .args(["ready", "--release", &missing_release])
        .output()
        .expect("missing ready target query runs");
    assert_eq!(missing_ready.status.code(), Some(5));
    assert!(String::from_utf8_lossy(&missing_ready.stderr).contains(&missing_release));
}

#[test]
fn association_filters_exclude_unrelated_record_kinds() {
    let repository = initialized_repository();
    fs::write(repository.path().join("feature.md"), "# Feature\n").expect("spec can be written");
    arcl_in(repository.path())
        .args(["idea", "create", "Unrelated idea"])
        .output()
        .expect("idea create runs");
    let release = arcl_in(repository.path())
        .args(["--json", "release", "create", "Release"])
        .output()
        .expect("release create runs");
    let release_payload: Value = serde_json::from_slice(&release.stdout).expect("release JSON parses");
    let release_id = release_payload["data"]["release"]["id"].as_str().expect("release ID");
    let epic = arcl_in(repository.path())
        .args([
            "--json",
            "epic",
            "create",
            "Epic",
            "--spec",
            "feature.md",
            "--release",
            release_id,
        ])
        .output()
        .expect("epic create runs");
    let epic_id = epic_id_from_json(&epic.stdout);
    let milestone = arcl_in(repository.path())
        .args(["--json", "milestone", "create", "Milestone", "--epic", &epic_id])
        .output()
        .expect("milestone create runs");
    let milestone_id = milestone_id_from_json(&milestone.stdout);
    let task = arcl_in(repository.path())
        .args(["--json", "task", "create", "Task", "--milestone", &milestone_id])
        .output()
        .expect("task create runs");
    let task_id = task_id_from_json(&task.stdout);

    let by_epic = arcl_in(repository.path())
        .args(["--json", "list", "--epic", &epic_id])
        .output()
        .expect("epic-filtered list runs");
    let payload: Value = serde_json::from_slice(&by_epic.stdout).expect("list JSON parses");
    let kinds = payload["data"]["records"]
        .as_array()
        .expect("records")
        .iter()
        .map(|record| record["kind"].as_str().expect("record kind"))
        .collect::<Vec<_>>();
    assert_eq!(kinds, ["epic", "milestone", "task"]);

    let by_milestone = arcl_in(repository.path())
        .args(["--json", "list", "--milestone", &milestone_id])
        .output()
        .expect("milestone-filtered list runs");
    let payload: Value = serde_json::from_slice(&by_milestone.stdout).expect("list JSON parses");
    let kinds = payload["data"]["records"]
        .as_array()
        .expect("records")
        .iter()
        .map(|record| record["kind"].as_str().expect("record kind"))
        .collect::<Vec<_>>();
    assert_eq!(kinds, ["milestone", "task"]);

    let shown = arcl_in(repository.path())
        .args(["--plain", "show", &milestone_id])
        .output()
        .expect("plain milestone show runs");
    assert!(String::from_utf8_lossy(&shown.stdout).contains(&task_id));
}

#[test]
fn malformed_graphs_do_not_appear_ready_or_overflow_tree_rendering() {
    let repository = initialized_repository();
    fs::write(repository.path().join("feature.md"), "# Feature\n").expect("spec can be written");
    let epic = arcl_in(repository.path())
        .args(["--json", "epic", "create", "Epic", "--spec", "feature.md"])
        .output()
        .expect("epic create runs");
    let epic_id = epic_id_from_json(&epic.stdout);
    let milestone = arcl_in(repository.path())
        .args(["--json", "milestone", "create", "Milestone", "--epic", &epic_id])
        .output()
        .expect("milestone create runs");
    let milestone_id = milestone_id_from_json(&milestone.stdout);
    let parent = arcl_in(repository.path())
        .args(["--json", "task", "create", "Parent", "--milestone", &milestone_id])
        .output()
        .expect("parent create runs");
    let parent_id = task_id_from_json(&parent.stdout);
    let child = arcl_in(repository.path())
        .args([
            "--json",
            "task",
            "create",
            "Child",
            "--milestone",
            &milestone_id,
            "--parent",
            &parent_id,
        ])
        .output()
        .expect("child create runs");
    let child_id = task_id_from_json(&child.stdout);

    let database = rusqlite::Connection::open(repository.path().join(".arcl/arcl.db")).expect("database opens");
    database
        .pragma_update(None, "foreign_keys", false)
        .expect("foreign keys can be disabled for corruption fixture");
    let missing_blocker = TaskId::new().to_string();
    database
        .execute(
            "INSERT INTO task_dependencies (task_id, blocker_id) VALUES (?1, ?2)",
            [&child_id, &missing_blocker],
        )
        .expect("malformed dependency can be injected");
    let ready = arcl_in(repository.path())
        .args(["--plain", "ready"])
        .output()
        .expect("ready query runs");
    assert!(ready.status.success());
    assert!(!String::from_utf8_lossy(&ready.stdout).contains(&child_id));

    database
        .execute("UPDATE tasks SET parent_id = ?1 WHERE id = ?2", [&child_id, &parent_id])
        .expect("parent cycle can be injected");
    drop(database);
    let tree = arcl_in(repository.path())
        .args(["tree", &parent_id])
        .output()
        .expect("tree query returns without overflowing");
    assert_eq!(tree.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&tree.stderr).contains("cycle"));
}

#[test]
fn snapshot_import_rebuilds_a_missing_database_and_normalizes_records() {
    let repository = worktree("ref: refs/heads/main\n");
    let initialized = arcl_in(repository.path())
        .args(["--color", "never", "init", "--snapshot"])
        .output()
        .expect("arcl init --snapshot runs");
    assert!(initialized.status.success());
    fs::write(repository.path().join("feature.md"), "# Feature\n").expect("spec writes");

    let snapshot = repository.path().join(".arcl/snapshot");
    let epic_id = "arcl-e-01K0B2ZWTX7JX9PH7W5G1S6A9Q";
    let milestone_id = "arcl-m-01K0B2ZWTX7JX9PH7W5G1S6A9Q";
    let task_id = "arcl-t-01K0B31M6VGK4YH8VKT4C0D2DR";
    fs::write(
        snapshot.join("epics").join(format!("{epic_id}.md")),
        format!(
            r#"+++
id = "{epic_id}"
title = "Feature"
status = "open"
spec-path = "feature.md"
+++
"#,
        ),
    )
    .expect("epic snapshot writes");
    fs::write(
        snapshot.join("milestones").join(format!("{milestone_id}.md")),
        format!(
            r#"+++
id = "{milestone_id}"
title = "Foundation"
status = "open"
epic = "{epic_id}"
position = 0
plan-key = "foundation"
+++
"#,
        ),
    )
    .expect("milestone snapshot writes");
    fs::write(
        snapshot.join("tasks").join(format!("{task_id}.md")),
        format!(
            r#"+++
id = "{task_id}"
title = "Build feature"
status = "pending"
priority = "high"
milestone = "{milestone_id}"
position = 0
plan-key = "build-feature"
+++
"#,
        ),
    )
    .expect("task snapshot writes");

    fs::remove_file(repository.path().join(".arcl/arcl.db")).expect("local database removes");
    let imported = arcl_in(repository.path())
        .args(["--quiet", "snapshot", "import"])
        .output()
        .expect("snapshot import runs");
    assert!(
        imported.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&imported.stderr)
    );

    let database = Database::open(repository.path().join(".arcl/arcl.db")).expect("rebuilt database opens");
    let graph = database.graph().expect("rebuilt graph loads");
    assert_eq!(graph.epics.len(), 1);
    assert_eq!(graph.milestones[0].plan_key.as_deref(), Some("foundation"));
    assert_eq!(graph.tasks[0].plan_key.as_deref(), Some("build-feature"));
    assert!(
        database
            .snapshot_base()
            .expect("base loads")
            .iter()
            .all(|file| { file.content == fs::read(snapshot.join(&file.path)).expect("base file reads") })
    );
}

#[test]
fn invalid_snapshot_import_reports_source_and_leaves_database_and_files_unchanged() {
    let repository = initialized_repository();
    let enabled = arcl_in(repository.path())
        .args(["--quiet", "init", "--snapshot"])
        .output()
        .expect("snapshot enable runs");
    assert!(enabled.status.success());
    let created = arcl_in(repository.path())
        .args(["--json", "idea", "create", "Imported idea"])
        .output()
        .expect("idea creates");
    assert!(created.status.success());
    let id = idea_id_from_json(&created.stdout);
    assert!(
        arcl_in(repository.path())
            .args(["--quiet", "snapshot", "export"])
            .output()
            .expect("export runs")
            .status
            .success()
    );

    let path = repository.path().join(".arcl/snapshot/ideas").join(format!("{id}.md"));
    let before = fs::read(&path).expect("snapshot record reads");
    let invalid = String::from_utf8(before.clone())
        .expect("snapshot is UTF-8")
        .replace("title = \"Imported idea\"", "title = \"  \"");
    fs::write(&path, &invalid).expect("invalid snapshot writes");
    let failed = arcl_in(repository.path())
        .args(["snapshot", "import"])
        .output()
        .expect("snapshot import runs");
    assert_eq!(failed.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&failed.stderr);
    assert!(stderr.contains(&format!("ideas/{id}.md")));
    assert!(stderr.contains("front matter"));
    assert_eq!(
        fs::read(&path).expect("snapshot record is preserved"),
        invalid.into_bytes()
    );
    let database = Database::open(repository.path().join(".arcl/arcl.db")).expect("database reopens");
    assert_eq!(
        database
            .idea(IdeaId::parse(&id).expect("idea ID"))
            .expect("idea reads")
            .expect("idea exists")
            .title,
        "Imported idea"
    );
}

#[test]
fn snapshot_import_rejects_removing_a_record_in_the_stored_base() {
    let repository = initialized_repository();
    let enabled = arcl_in(repository.path())
        .args(["--quiet", "init", "--snapshot"])
        .output()
        .expect("snapshot enable runs");
    assert!(enabled.status.success());
    let created = arcl_in(repository.path())
        .args(["--json", "idea", "create", "Protected idea"])
        .output()
        .expect("idea creates");
    let id = idea_id_from_json(&created.stdout);
    assert!(
        arcl_in(repository.path())
            .args(["--quiet", "snapshot", "export"])
            .output()
            .expect("export runs")
            .status
            .success()
    );
    let path = repository.path().join(".arcl/snapshot/ideas").join(format!("{id}.md"));
    fs::remove_file(&path).expect("snapshot record removes");

    let failed = arcl_in(repository.path())
        .args(["snapshot", "import"])
        .output()
        .expect("snapshot import runs");
    assert_eq!(failed.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&failed.stderr).contains("removed or renamed"));
    assert!(
        Database::open(repository.path().join(".arcl/arcl.db"))
            .expect("database reopens")
            .idea(IdeaId::parse(&id).expect("idea ID"))
            .expect("idea reads")
            .is_some()
    );
}

#[test]
fn handoff_and_evidence_survive_lifecycle_and_context_output() {
    let repository = initialized_repository();
    fs::write(repository.path().join("feature.md"), "# Feature\n").expect("spec can be written");
    let epic = arcl_in(repository.path())
        .args(["--json", "epic", "create", "Epic", "--spec", "feature.md"])
        .output()
        .expect("epic create runs");
    let epic_id = epic_id_from_json(&epic.stdout);
    let milestone = arcl_in(repository.path())
        .args(["--json", "milestone", "create", "Milestone", "--epic", &epic_id])
        .output()
        .expect("milestone create runs");
    let milestone_id = milestone_id_from_json(&milestone.stdout);
    let task = arcl_in(repository.path())
        .args(["--json", "task", "create", "Task", "--milestone", &milestone_id])
        .output()
        .expect("task create runs");
    let task_id = task_id_from_json(&task.stdout);
    arcl_in(repository.path())
        .args(["task", "start", &task_id])
        .output()
        .expect("start runs");
    let handoff = arcl_in(repository.path())
        .args(["--json", "task", "handoff", &task_id, "--note", "resume here"])
        .output()
        .expect("handoff runs");
    let handoff_payload: Value = serde_json::from_slice(&handoff.stdout).expect("handoff JSON parses");
    assert_eq!(handoff_payload["data"]["task"]["status"], "parked");
    assert_eq!(handoff_payload["data"]["task"]["handoff"], "resume here");
    arcl_in(repository.path())
        .args(["task", "unpark", &task_id])
        .output()
        .expect("unpark runs");
    arcl_in(repository.path())
        .args(["task", "start", &task_id])
        .output()
        .expect("restart runs");
    let complete = arcl_in(repository.path())
        .args(["--json", "task", "complete", &task_id, "--evidence", "checked manually"])
        .output()
        .expect("complete runs");
    let complete_payload: Value = serde_json::from_slice(&complete.stdout).expect("complete JSON parses");
    assert_eq!(complete_payload["data"]["task"]["handoff"], "resume here");
    assert_eq!(complete_payload["data"]["task"]["evidence"], "checked manually");
    let context = arcl_in(repository.path())
        .args(["--json", "context", &task_id])
        .output()
        .expect("context runs");
    let context_payload: Value = serde_json::from_slice(&context.stdout).expect("context JSON parses");
    assert_eq!(context_payload["data"]["task"]["handoff"], "resume here");
    assert_eq!(context_payload["data"]["task"]["evidence"], "checked manually");

    let human_context = arcl_in(repository.path())
        .args(["context", &task_id])
        .output()
        .expect("human context runs");
    let human_context = String::from_utf8_lossy(&human_context.stdout);
    assert!(human_context.contains(&format!("Milestone: {milestone_id}")));
    assert!(human_context.contains(&format!("Epic: {epic_id}")));
    assert!(human_context.contains("Handoff: resume here"));
    assert!(human_context.contains("Evidence: checked manually"));
}
