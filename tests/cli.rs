use std::{fs, path::Path};

use arcl::{
    storage::Database,
    vcs::{GixVcs, Vcs},
};
use assert_cmd::Command;
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
    assert_eq!(database.schema_version().expect("schema version is readable"), 1);
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
