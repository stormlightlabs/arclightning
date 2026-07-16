use std::path::Path;

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
