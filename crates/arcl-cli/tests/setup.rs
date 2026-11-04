//! CLI tests for `arcl setup claude|codex|pi`, each in a scratch project.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use assert_cmd::Command;
use serde_json::Value;
use tempfile::{TempDir, tempdir};

/// The agents `arcl setup` takes.
const AGENTS: [&str; 3] = ["claude", "codex", "pi"];

/// The skill paths every agent gets.
const SKILL_PATHS: [&str; 2] = [
    ".claude/skills/arclightning/SKILL.md",
    ".agents/skills/arclightning/SKILL.md",
];

/// The command each hook runs.
const HOOK_COMMAND: &str = "if command -v arcl >/dev/null; then arcl prime; fi";

/// The instruction file and the hook file `arcl setup <agent>` writes.
fn agent_paths(agent: &str) -> (&'static str, &'static str) {
    match agent {
        "claude" => ("CLAUDE.md", ".claude/settings.json"),
        "codex" => ("AGENTS.md", ".codex/hooks.json"),
        "pi" => ("AGENTS.md", ".pi/extensions/arcl-prime.ts"),
        _ => unreachable!("unknown agent {agent}"),
    }
}

/// An `arcl` command in `directory` with no Git state or home from the host.
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

/// A scratch project after `arcl init`, and a separate empty home.
struct Project {
    dir: TempDir,
    home: TempDir,
}

impl Project {
    fn new() -> Self {
        let project = Self { dir: tempdir().expect("project directory"), home: tempdir().expect("home directory") };
        let output = project.arcl(&["--quiet", "init"]).output().expect("init runs");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        project
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn arcl(&self, args: &[&str]) -> Command {
        let mut command = arcl_in(self.dir.path(), self.home.path());
        command.args(args);
        command
    }

    /// Run `arcl --json setup <args>`, assert success, and return the envelope.
    fn setup_json(&self, args: &[&str]) -> Value {
        let output = self
            .arcl(&[&["--json", "setup"], args].concat())
            .output()
            .expect("setup runs");
        assert!(
            output.status.success(),
            "setup {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("JSON output parses")
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.path().join(relative);
        fs::create_dir_all(path.parent().expect("parent")).expect("parent creates");
        fs::write(path, text).expect("file writes");
    }

    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.path().join(relative)).expect("file reads")
    }

    /// Every file outside `.arcl/`, with its contents and modification time.
    fn snapshot(&self) -> BTreeMap<PathBuf, (Vec<u8>, std::time::SystemTime)> {
        fn walk(base: &Path, directory: &Path, found: &mut BTreeMap<PathBuf, (Vec<u8>, std::time::SystemTime)>) {
            for entry in fs::read_dir(directory).expect("directory reads") {
                let path = entry.expect("entry reads").path();
                if path.file_name().is_some_and(|name| name == ".arcl") {
                    continue;
                }
                if path.is_dir() {
                    walk(base, &path, found);
                } else {
                    let meta = fs::metadata(&path).expect("metadata");
                    let relative = path.strip_prefix(base).expect("under base").to_owned();
                    found.insert(
                        relative,
                        (fs::read(&path).expect("reads"), meta.modified().expect("mtime")),
                    );
                }
            }
        }
        let mut found = BTreeMap::new();
        walk(self.path(), self.path(), &mut found);
        found
    }
}

/// Each file's `(path, status)` in a setup report.
fn statuses(report: &Value) -> Vec<(String, String)> {
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

/// The paths `arcl setup <agent>` reports, in order.
fn report_paths(agent: &str) -> Vec<String> {
    let (instructions, hook) = agent_paths(agent);
    SKILL_PATHS
        .iter()
        .chain([instructions, hook].iter())
        .map(|path| (*path).to_owned())
        .collect()
}

/// Every reported path paired with `status`.
fn all(agent: &str, status: &str) -> Vec<(String, String)> {
    report_paths(agent)
        .into_iter()
        .map(|path| (path, status.to_owned()))
        .collect()
}

/// Assert the hook file runs `arcl prime` once.
fn assert_hook(project: &Project, agent: &str) {
    let (_, hook) = agent_paths(agent);
    let text = project.read(hook);
    if agent == "pi" {
        assert!(text.contains("pi.exec(\"arcl\", [\"prime\"]"), "{text}");
        assert!(text.contains("pi.on(\"session_start\""), "{text}");
        return;
    }
    let settings: Value = serde_json::from_str(&text).expect("settings parse");
    let entries = settings["hooks"]["SessionStart"].as_array().expect("SessionStart");
    let commands = entries
        .iter()
        .flat_map(|entry| entry["hooks"].as_array().cloned().unwrap_or_default())
        .filter(|hook| hook["command"] == HOOK_COMMAND)
        .count();
    assert_eq!(commands, 1, "{text}");
    if agent == "codex" {
        assert_eq!(entries.last().expect("entry")["matcher"], "startup|resume");
    }
}

#[test]
fn setup_writes_the_skill_instructions_and_hook_in_a_fresh_project() {
    for agent in AGENTS {
        let project = Project::new();
        let report = project.setup_json(&[agent]);
        assert_eq!(report["format_version"], 1);
        assert_eq!(report["data"]["action"], "applied");
        assert_eq!(report["data"]["agent"], agent);
        assert_eq!(statuses(&report), all(agent, "created"), "{agent}");
        for path in SKILL_PATHS {
            assert!(
                project.read(path).starts_with("---\nname: arclightning\n"),
                "{agent} {path}"
            );
        }
        let (instructions, _) = agent_paths(agent);
        let text = project.read(instructions);
        assert!(text.starts_with("<!-- arcl-setup:begin -->\n"), "{agent}: {text}");
        assert!(text.ends_with("<!-- arcl-setup:end -->\n"), "{agent}: {text}");
        assert_hook(&project, agent);
    }
}

#[test]
fn setup_with_yes_changes_existing_files_and_keeps_their_content() {
    for agent in AGENTS {
        let project = Project::new();
        let (instructions, hook) = agent_paths(agent);
        project.write(instructions, "# Rules\n\nKeep this line.\n");
        let existing_hook = if agent == "pi" {
            "// an older extension\n"
        } else {
            "{\n  \"model\": \"x\",\n  \"hooks\": {\n    \"Stop\": []\n  }\n}\n"
        };
        project.write(hook, existing_hook);

        let report = project.setup_json(&[agent, "--yes"]);
        let mut expected = all(agent, "created");
        expected[2].1 = "updated".to_owned();
        expected[3].1 = "updated".to_owned();
        assert_eq!(statuses(&report), expected, "{agent}");

        let text = project.read(instructions);
        assert!(
            text.starts_with("# Rules\n\nKeep this line.\n\n<!-- arcl-setup:begin -->\n"),
            "{agent}: {text}"
        );
        assert_hook(&project, agent);
        if agent != "pi" {
            let settings = project.read(hook);
            assert!(
                settings.starts_with("{\n  \"model\": \"x\",\n  \"hooks\": {\n    \"Stop\": [],"),
                "{settings}"
            );
        }
    }
}

#[test]
fn setup_without_a_terminal_or_yes_refuses_to_change_existing_files() {
    for agent in AGENTS {
        let project = Project::new();
        let (instructions, _) = agent_paths(agent);
        project.write(instructions, "# Rules\n");
        let before = project.snapshot();

        let output = project.arcl(&["--json", "setup", agent]).output().expect("setup runs");
        assert_eq!(output.status.code(), Some(4), "{agent}");
        assert!(output.stdout.is_empty());
        let error: Value = serde_json::from_slice(&output.stderr).expect("error envelope");
        assert_eq!(error["error"]["code"], "conflict");
        let message = error["error"]["message"].as_str().expect("message");
        assert!(message.contains(instructions) && message.contains("--yes"), "{message}");
        assert_eq!(project.snapshot(), before, "{agent} wrote files without confirmation");
    }
}

#[test]
fn setup_dry_run_writes_nothing_and_prints_diffs() {
    for agent in AGENTS {
        let project = Project::new();
        let (instructions, _) = agent_paths(agent);
        project.write(instructions, "# Rules\n");
        let before = project.snapshot();

        let report = project.setup_json(&[agent, "--dry-run"]);
        assert_eq!(report["data"]["action"], "planned");
        let mut expected = all(agent, "created");
        expected[2].1 = "updated".to_owned();
        assert_eq!(statuses(&report), expected, "{agent}");
        let diff = report["data"]["files"][2]["diff"].as_str().expect("diff");
        assert!(
            diff.starts_with(&format!("--- a/{instructions}\n+++ b/{instructions}\n")),
            "{diff}"
        );
        assert!(diff.contains("+<!-- arcl-setup:begin -->\n"), "{diff}");

        let human = project
            .arcl(&["setup", agent, "--dry-run"])
            .output()
            .expect("setup runs");
        assert!(human.status.success());
        let text = String::from_utf8_lossy(&human.stdout);
        assert!(text.contains("dry run, nothing written"), "{text}");
        assert!(text.contains(&format!("  update {instructions}")), "{text}");
        assert_eq!(project.snapshot(), before, "{agent} --dry-run wrote files");
    }
}

#[test]
fn setup_run_twice_changes_nothing_the_second_time() {
    for agent in AGENTS {
        let project = Project::new();
        project.setup_json(&[agent]);
        let before = project.snapshot();

        // No `--yes`: a run with nothing to change has nothing to ask.
        let report = project.setup_json(&[agent]);
        assert_eq!(statuses(&report), all(agent, "unchanged"), "{agent}");
        let dry = project.setup_json(&[agent, "--dry-run"]);
        assert!(dry["data"]["files"][2].get("diff").is_none());
        assert_eq!(project.snapshot(), before, "{agent} rewrote files on a second run");

        let plain = project.arcl(&["--plain", "setup", agent]).output().expect("setup runs");
        let lines = report_paths(agent)
            .iter()
            .map(|path| format!("unchanged\t{path}\n"))
            .collect::<String>();
        assert_eq!(String::from_utf8_lossy(&plain.stdout), lines);
    }
}

#[cfg(unix)]
#[test]
fn setup_follows_a_linked_instruction_file() {
    let project = Project::new();
    project.write("AGENTS.md", "# Shared rules\n");
    std::os::unix::fs::symlink("AGENTS.md", project.path().join("CLAUDE.md")).expect("link creates");
    project.setup_json(&["claude", "--yes"]);
    assert!(
        fs::symlink_metadata(project.path().join("CLAUDE.md"))
            .expect("metadata")
            .file_type()
            .is_symlink()
    );
    assert!(project.read("AGENTS.md").contains("<!-- arcl-setup:begin -->"));
    let codex = project.setup_json(&["codex"]);
    assert_eq!(statuses(&codex)[2], ("AGENTS.md".to_owned(), "unchanged".to_owned()));
}

#[test]
fn setup_outside_a_project_fails() {
    let dir = tempdir().expect("directory");
    let home = tempdir().expect("home");
    let output = arcl_in(dir.path(), home.path())
        .args(["setup", "claude"])
        .output()
        .expect("setup runs");
    assert_eq!(output.status.code(), Some(3));
    assert!(fs::read_dir(dir.path()).expect("reads").next().is_none());
}
