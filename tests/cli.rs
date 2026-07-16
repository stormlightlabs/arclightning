use std::{fs, path::Path};

use arcl::{
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
    assert_eq!(database.schema_version().expect("schema version is readable"), 2);
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
