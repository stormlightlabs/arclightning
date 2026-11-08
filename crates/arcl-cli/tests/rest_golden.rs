//! Golden tests for the `arcl serve` reads and `arcl web export`.
//!
//! `fixtures/rest/project` holds a project's workspace with every record
//! kind, blockers (inherited and cancelled among them), gates, task notes,
//! claims, a parked task with a handoff, captures that were promoted and
//! discarded, and a release. Each test imports it into a fresh directory and
//! compares the bytes each read returns with the files in
//! `fixtures/rest/golden`. Set `ARCL_BLESS=1` to rewrite those files. Each
//! read the CLI also makes with `--json` must print the same bytes as
//! `arcl serve` returns. `arcl roadmap` prints the page in
//! `fixtures/rest/golden/roadmap.md`.

use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use tempfile::{TempDir, tempdir};

const SPEC: &str = "arcl-s-01M6ESM3HXTX7F827GYCBK76G9";
const PROMOTED_SPEC: &str = "arcl-s-01M6ESM4PDAEXXXQW2ZWVM32ZA";
const PLAN: &str = "arcl-pl-01M6ESM3K3C7SKW1SCDMMXY8CZ";
const PHASE: &str = "arcl-ph-01M6ESM3K3R0ARF6QWGPWNCBWD";
const COMPLETED: &str = "arcl-t-01M6ESM3K3YMT4YR4KYY844N16";
const BLOCKED: &str = "arcl-t-01M6ESM3K37MMJGWJF7WH4QZ8B";
const SUBTASK: &str = "arcl-t-01M6ESM3K3FWTMZH0X0AA4GMN5";
const LOOSE: &str = "arcl-t-01M6ESM3TNSAKEPPBK3QJJXBYY";
const CANCELLED: &str = "arcl-t-01M6ESM3VH935DBQTCKSQS7J51";
const WAITER: &str = "arcl-t-01M6ESM3WY149V8190PNBE4C3Y";
const PARENT: &str = "arcl-t-01M6ESM3XVW978H7R43R037PQS";
const CHILD: &str = "arcl-t-01M6ESM3YTVS70QR6RGWNKFGB7";
const GATED: &str = "arcl-t-01M6ESM3ZQ5AST3EQ9T52E0E87";
const PARKED: &str = "arcl-t-01M6ESM4CY6E640477815T3K09";
const CLAIMED: &str = "arcl-t-01M6ESM4FEF285TNJBPGR9Q3Y1";
const PROMOTED_TASK: &str = "arcl-t-01M6ESM4Q70068VAHSVYC6XEHF";
const CAPTURED: &str = "arcl-c-01M6ESM4H6XRTTCM5G83JXE7BX";
const PROMOTED_CAPTURE: &str = "arcl-c-01M6ESM4KB90H8AF31MWRD02PX";
const DISCARDED: &str = "arcl-c-01M6ESM4NEB5KCSJEANEARDR1F";
const PROMOTED_NOTE: &str = "arcl-n-01M6ESM4R3XPQ0W25K2ME7JR5Y";
const NOTE: &str = "arcl-n-01M6ESM4SJK690E6QDTA9C4AJ1";
const RELEASE: &str = "arcl-r-01M6ESM4TGX12A1W96QCNMW432";
const OPEN_GATE: &str = "arcl-g-01M6ESM40QGDEYYWYKAKAQ1EJC";
const MET_GATE: &str = "arcl-g-01M6ESM41DMGD9WMME1WQ7970X";
const MISSING_TASK: &str = "arcl-t-00000000000000000000000000";

/// A running `arcl serve`, stopped when dropped.
struct Served {
    child: Child,
    port: u16,
    token: String,
}

impl Served {
    fn start(directory: &Path) -> Self {
        let mut child = arcl(directory)
            .args(["--plain", "serve"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("arcl serve starts");
        let mut line = String::new();
        BufReader::new(child.stdout.take().expect("stdout is piped"))
            .read_line(&mut line)
            .expect("arcl serve prints its URL");
        let rest = line
            .trim()
            .strip_prefix("http://127.0.0.1:")
            .expect("the URL names 127.0.0.1");
        let (port, token) = rest.split_once("/?token=").expect("the URL carries the token");
        Self { child, port: port.parse().expect("the URL has a port"), token: token.to_owned() }
    }

    /// GET `path` with the right `Host` and token, and return the status and
    /// the body's bytes.
    fn get(&self, path: &str) -> (u16, String) {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).expect("arcl serve accepts");
        let request = format!(
            "GET {path} HTTP/1.1\r\nConnection: close\r\nHost: 127.0.0.1:{}\r\nX-Arcl-Token: {}\r\n\r\n",
            self.port, self.token
        );
        stream.write_all(request.as_bytes()).expect("the request writes");
        let mut response = String::new();
        stream.read_to_string(&mut response).expect("the response reads");
        let (head, body) = response.split_once("\r\n\r\n").expect("the response has a body");
        let status = head.split(' ').nth(1).and_then(|status| status.parse().ok());
        (status.expect("a numeric status"), body.to_owned())
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Every read and the golden file that holds its answer: the status on the
/// first line, then the body.
fn cases() -> Vec<(String, String)> {
    let mut cases = vec![
        ("records-all".to_owned(), "/api/records".to_owned()),
        (
            "records-status-many".to_owned(),
            "/api/records?kind=task&status=pending&status=parked".to_owned(),
        ),
        (
            "records-priority".to_owned(),
            "/api/records?priority=high&priority=critical".to_owned(),
        ),
        ("records-release".to_owned(), format!("/api/records?release={RELEASE}")),
        ("records-spec".to_owned(), format!("/api/records?spec={SPEC}")),
        ("records-plan".to_owned(), format!("/api/records?plan={PLAN}")),
        ("records-phase".to_owned(), format!("/api/records?phase={PHASE}")),
        ("records-parent".to_owned(), format!("/api/records?parent={BLOCKED}")),
        ("records-short-spec".to_owned(), "/api/records?spec=76G9".to_owned()),
        (
            "records-captured".to_owned(),
            "/api/records?kind=capture&status=captured".to_owned(),
        ),
        ("ready-all".to_owned(), "/api/ready".to_owned()),
        ("ready-priority".to_owned(), "/api/ready?priority=low".to_owned()),
        ("ready-spec".to_owned(), format!("/api/ready?spec={SPEC}")),
        ("ready-plan".to_owned(), format!("/api/ready?plan={PLAN}")),
        ("ready-phase".to_owned(), format!("/api/ready?phase={PHASE}")),
        ("ready-parent".to_owned(), format!("/api/ready?parent={BLOCKED}")),
        ("tree".to_owned(), "/api/tree".to_owned()),
        ("work".to_owned(), "/api/work".to_owned()),
        ("show-short-task".to_owned(), "/api/records/3K09".to_owned()),
        ("show-short-gate".to_owned(), "/api/records/1EJC".to_owned()),
        ("show-open-gate".to_owned(), format!("/api/records/{OPEN_GATE}")),
        ("show-met-gate".to_owned(), format!("/api/records/{MET_GATE}")),
        ("source-short".to_owned(), "/api/records/E7BX/source".to_owned()),
        // Errors.
        ("error-records-kind".to_owned(), "/api/records?kind=nothing".to_owned()),
        (
            "error-records-filter".to_owned(),
            "/api/records?assignee=alice".to_owned(),
        ),
        (
            "error-records-twice".to_owned(),
            "/api/records?kind=task&kind=spec".to_owned(),
        ),
        (
            "error-records-status".to_owned(),
            "/api/records?kind=spec&status=parked".to_owned(),
        ),
        (
            "error-records-priority".to_owned(),
            "/api/records?priority=huge".to_owned(),
        ),
        (
            "error-records-spec".to_owned(),
            "/api/records?spec=arcl-s-bad".to_owned(),
        ),
        (
            "error-records-spec-kind".to_owned(),
            format!("/api/records?spec={PLAN}"),
        ),
        (
            "error-records-missing-spec".to_owned(),
            "/api/records?spec=arcl-s-00000000000000000000000000".to_owned(),
        ),
        ("error-ready-filter".to_owned(), "/api/ready?status=pending".to_owned()),
        ("error-ready-priority".to_owned(), "/api/ready?priority=huge".to_owned()),
        ("error-ready-phase".to_owned(), "/api/ready?phase=ZZZZ".to_owned()),
        ("error-show-missing".to_owned(), format!("/api/records/{MISSING_TASK}")),
        ("error-show-bad".to_owned(), "/api/records/arcl-t-bad".to_owned()),
        ("error-show-short".to_owned(), "/api/records/ZZZZ".to_owned()),
        ("error-show-unknown".to_owned(), "/api/records/nothing".to_owned()),
        (
            "error-show-missing-gate".to_owned(),
            "/api/records/arcl-g-00000000000000000000000000".to_owned(),
        ),
        (
            "error-source-missing".to_owned(),
            format!("/api/records/{MISSING_TASK}/source"),
        ),
        (
            "error-source-gate".to_owned(),
            format!("/api/records/{OPEN_GATE}/source"),
        ),
        (
            "error-explain-missing".to_owned(),
            format!("/api/tasks/{MISSING_TASK}/explain"),
        ),
        ("error-explain-spec".to_owned(), format!("/api/tasks/{SPEC}/explain")),
        (
            "error-explain-bad".to_owned(),
            "/api/tasks/arcl-t-bad/explain".to_owned(),
        ),
        (
            "error-context-missing".to_owned(),
            format!("/api/tasks/{MISSING_TASK}/context"),
        ),
        ("error-context-short".to_owned(), "/api/tasks/ZZZZ/context".to_owned()),
        ("error-tree-query".to_owned(), "/api/tree?all=true".to_owned()),
        ("error-work-query".to_owned(), "/api/work?x=1".to_owned()),
    ];
    for kind in ["capture", "release", "spec", "plan", "phase", "task", "note"] {
        cases.push((format!("records-kind-{kind}"), format!("/api/records?kind={kind}")));
    }
    let records = [
        SPEC,
        PROMOTED_SPEC,
        PLAN,
        PHASE,
        COMPLETED,
        BLOCKED,
        SUBTASK,
        LOOSE,
        CANCELLED,
        WAITER,
        PARENT,
        CHILD,
        GATED,
        PARKED,
        CLAIMED,
        PROMOTED_TASK,
        CAPTURED,
        PROMOTED_CAPTURE,
        DISCARDED,
        PROMOTED_NOTE,
        NOTE,
        RELEASE,
    ];
    for id in records {
        cases.push((format!("show-{id}"), format!("/api/records/{id}")));
        cases.push((format!("source-{id}"), format!("/api/records/{id}/source")));
    }
    for id in [
        COMPLETED,
        BLOCKED,
        SUBTASK,
        LOOSE,
        WAITER,
        PARENT,
        CHILD,
        GATED,
        PARKED,
        CLAIMED,
        PROMOTED_TASK,
    ] {
        cases.push((format!("explain-{id}"), format!("/api/tasks/{id}/explain")));
        cases.push((format!("context-{id}"), format!("/api/tasks/{id}/context")));
    }
    cases.push(("explain-short".to_owned(), "/api/tasks/3K09/explain".to_owned()));
    cases.push(("context-short".to_owned(), "/api/tasks/3K09/context".to_owned()));
    cases
}

#[test]
fn serve_reads_return_the_golden_envelopes() {
    let project = fixture_project();
    let served = Served::start(project.path());
    let mut differ = Vec::new();
    for (name, path) in cases() {
        let (status, body) = served.get(&path);
        if !matches_golden(&golden_dir().join(format!("{name}.txt")), &format!("{status}\n{body}")) {
            differ.push(format!("{name}: GET {path}"));
        }
    }
    assert!(
        differ.is_empty(),
        "reads differ from their golden files:\n{}",
        differ.join("\n")
    );
}

/// Cases whose `arcl` form is not the same request: the CLI takes
/// `--assignee` and `--all`, and clap rejects a repeated `--kind` and a
/// `--status` on `ready` before arcl sees them.
const NOT_CLI_EQUIVALENT: [&str; 5] = [
    "error-records-filter",
    "error-records-twice",
    "error-ready-filter",
    "error-tree-query",
    "error-work-query",
];

#[test]
fn cli_json_reads_match_the_serve_reads() {
    let project = fixture_project();
    let served = Served::start(project.path());
    let mut compared = 0;
    let mut differ = Vec::new();
    for (name, path) in cases() {
        if NOT_CLI_EQUIVALENT.contains(&name.as_str()) {
            continue;
        }
        let Some(args) = cli_args(&path) else { continue };
        let (status, body) = served.get(&path);
        let output = arcl(project.path())
            .arg("--json")
            .args(&args)
            .output()
            .expect("arcl runs");
        // A read prints its envelope on stdout; a failure prints its error
        // envelope on stderr and exits non-zero.
        let (succeeded, printed) = if output.status.success() { (true, output.stdout) } else { (false, output.stderr) };
        let printed = String::from_utf8_lossy(&printed);
        if succeeded != (status == 200) || printed.trim_end_matches('\n') != body {
            differ.push(format!("{name}: arcl --json {} against GET {path}", args.join(" ")));
        }
        compared += 1;
    }
    assert!(
        differ.is_empty(),
        "CLI reads differ from the serve reads:\n{}",
        differ.join("\n")
    );
    assert!(compared >= 90, "only {compared} cases have a CLI form");
}

/// The `arcl` arguments that make the same read as the `/api` request
/// `path`, if the CLI has one. `source` and `work` have no CLI command.
fn cli_args(path: &str) -> Option<Vec<String>> {
    let (route, query) = path.split_once('?').unwrap_or((path, ""));
    let segments: Vec<&str> = route.strip_prefix("/api/")?.split('/').collect();
    let mut args: Vec<String> = match segments.as_slice() {
        ["records"] => vec!["list".to_owned()],
        ["records", id] => vec!["show".to_owned(), (*id).to_owned()],
        ["ready"] => vec!["ready".to_owned()],
        ["tree"] => vec!["tree".to_owned()],
        ["tasks", id, "explain"] => vec!["explain".to_owned(), (*id).to_owned()],
        ["tasks", id, "context"] => vec!["context".to_owned(), (*id).to_owned()],
        _ => return None,
    };
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=')?;
        args.push(format!("--{key}"));
        args.push(value.to_owned());
    }
    Some(args)
}

#[test]
fn web_export_writes_the_golden_documents() {
    let project = fixture_project();
    let out = tempdir().expect("an export directory");
    let output = arcl(project.path())
        .args(["--json", "web", "export"])
        .arg(out.path())
        .output()
        .expect("arcl web export runs");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let mut files = Vec::new();
    collect_files(out.path(), out.path(), &mut files);
    files.sort();
    let mut differ = Vec::new();
    for relative in &files {
        let text = fs::read_to_string(out.path().join(relative)).expect("the document reads");
        // `manifest` and `meta/sync` name the time of the export, so that one
        // field is blanked and the rest of each document compared whole.
        let text = match relative.as_str() {
            "meta/sync.json" | "manifest.json" => blank_synced_at(&text),
            _ => text,
        };
        if !matches_golden(&golden_dir().join("export").join(relative), &text) {
            differ.push(relative.clone());
        }
    }
    assert!(
        differ.is_empty(),
        "exported documents differ from their golden files:\n{}",
        differ.join("\n")
    );
    let mut golden = Vec::new();
    collect_files(&golden_dir().join("export"), &golden_dir().join("export"), &mut golden);
    golden.sort();
    assert_eq!(files, golden, "the export writes the golden set of documents");
}

/// Run `arcl roadmap` with `args` in `directory` and return its stdout.
fn roadmap(directory: &Path, args: &[&str]) -> Vec<u8> {
    let output = arcl(directory).args(args).output().expect("arcl roadmap runs");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    output.stdout
}

#[test]
fn roadmap_prints_the_golden_page_the_same_way_each_time() {
    let project = fixture_project();
    let first = roadmap(project.path(), &["roadmap"]);
    let page = String::from_utf8(first.clone()).expect("the page is UTF-8");
    assert!(
        matches_golden(&golden_dir().join("roadmap.md"), &page),
        "`arcl roadmap` differs from roadmap.md:\n{page}"
    );
    assert_eq!(
        roadmap(project.path(), &["roadmap"]),
        first,
        "a second run prints the same bytes"
    );
    assert_eq!(
        roadmap(project.path(), &["--plain", "roadmap"]),
        first,
        "--plain prints the same page"
    );
}

#[test]
fn roadmap_output_writes_the_bytes_it_prints() {
    let project = fixture_project();
    let printed = roadmap(project.path(), &["roadmap"]);
    let confirmation = roadmap(project.path(), &["roadmap", "--output", "docs/roadmap.md"]);
    assert_eq!(
        String::from_utf8_lossy(&confirmation),
        "Wrote the roadmap to `docs/roadmap.md`.\n"
    );
    let path = project.path().join("docs/roadmap.md");
    assert_eq!(fs::read(&path).expect("the roadmap file reads"), printed);

    // A second write replaces the file whole and leaves no temporary file.
    fs::write(&path, "stale and longer than nothing").expect("the file writes");
    let confirmation = roadmap(project.path(), &["--json", "roadmap", "-o", "docs/roadmap.md"]);
    let value: serde_json::Value = serde_json::from_slice(&confirmation).expect("JSON");
    assert_eq!(
        value,
        serde_json::json!({ "format_version": 1, "data": { "path": "docs/roadmap.md" } })
    );
    assert_eq!(fs::read(&path).expect("the roadmap file reads"), printed);
    let mut files = Vec::new();
    collect_files(&project.path().join("docs"), &project.path().join("docs"), &mut files);
    assert_eq!(files, ["roadmap.md"]);
}

#[test]
fn roadmap_output_refuses_a_path_that_is_not_a_file() {
    let project = fixture_project();
    fs::create_dir(project.path().join("docs")).expect("the directory is created");
    for path in [".", "docs"] {
        let output = arcl(project.path())
            .args(["roadmap", "--output", path])
            .output()
            .expect("arcl roadmap runs");
        assert_eq!(output.status.code(), Some(3));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(&format!(
                "`{path}` is not a file; `--output` needs the path of a file to write"
            )),
            "{stderr}"
        );
    }
}

#[cfg(unix)]
#[test]
fn roadmap_output_writes_through_a_symbolic_link() {
    let project = fixture_project();
    let printed = roadmap(project.path(), &["roadmap"]);
    let site = project.path().join("site");
    fs::create_dir(&site).expect("the directory is created");
    fs::write(site.join("roadmap.md"), "old").expect("the target writes");
    std::os::unix::fs::symlink("site/roadmap.md", project.path().join("link.md")).expect("the link is created");
    // A link to a file that does not exist yet creates that file.
    std::os::unix::fs::symlink("site/new.md", project.path().join("dangling.md")).expect("the link is created");

    for (link, target) in [("link.md", "roadmap.md"), ("dangling.md", "new.md")] {
        roadmap(project.path(), &["roadmap", "-o", link]);
        let metadata = fs::symlink_metadata(project.path().join(link)).expect("the link stays");
        assert!(metadata.file_type().is_symlink(), "{link} is still a link");
        assert_eq!(fs::read(site.join(target)).expect("the target reads"), printed);
    }
    let mut files = Vec::new();
    collect_files(&site, &site, &mut files);
    files.sort();
    assert_eq!(files, ["new.md", "roadmap.md"]);
}

#[test]
fn roadmap_json_carries_the_page_data() {
    let project = fixture_project();
    let value: serde_json::Value =
        serde_json::from_slice(&roadmap(project.path(), &["--json", "roadmap"])).expect("JSON");
    assert_eq!(value["format_version"], 1);
    let spec = &value["data"]["specs"][0];
    assert_eq!(spec["id"], SPEC);
    assert_eq!(spec["plans"][0]["id"], PLAN);
    assert_eq!(spec["plans"][0]["phases"][0]["id"], PHASE);
    assert!(spec["plans"][0]["tasks_outside_phases"].is_null());
    assert!(spec["tasks_outside_plans"].is_null());
    assert_eq!(value["data"]["releases"][0]["id"], RELEASE);
    assert!(value["data"].get("captures").is_none() && value["data"].get("notes").is_none());
}

/// `text` with the value of its top-level `synced_at` replaced by
/// `<synced_at>`.
fn blank_synced_at(text: &str) -> String {
    let value: serde_json::Value = serde_json::from_str(text).expect("JSON");
    let synced_at = value["synced_at"].as_str().expect("a `synced_at` string");
    assert!(
        synced_at.len() == 20 && synced_at.ends_with('Z') && synced_at.as_bytes()[10] == b'T',
        "`synced_at` is a UTC timestamp: {synced_at}"
    );
    text.replacen(&format!("\"{synced_at}\""), "\"<synced_at>\"", 1)
}

/// The golden files' directory.
fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rest/golden")
}

/// Whether `actual` matches the golden file at `path`. With `ARCL_BLESS` set,
/// write `actual` there instead.
fn matches_golden(path: &Path, actual: &str) -> bool {
    if std::env::var_os("ARCL_BLESS").is_some() {
        fs::create_dir_all(path.parent().expect("a parent")).expect("the golden directory creates");
        fs::write(path, actual).expect("the golden file writes");
        return true;
    }
    fs::read_to_string(path).is_ok_and(|expected| expected == actual)
}

/// The fixture project imported into a fresh directory.
fn fixture_project() -> TempDir {
    let directory = tempdir().expect("a project directory");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rest/project");
    let arcl_dir = directory.path().join(".arcl");
    copy_dir(&source.join("workspace"), &arcl_dir.join("workspace"));
    fs::copy(source.join("config.toml"), arcl_dir.join("config.toml")).expect("the config copies");
    for args in [&["init"][..], &["list"][..]] {
        let output = arcl(directory.path()).args(args).output().expect("arcl runs");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    }
    directory
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("the directory creates");
    for entry in fs::read_dir(from).expect("the directory reads") {
        let entry = entry.expect("an entry");
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("the file copies");
        }
    }
}

fn collect_files(root: &Path, directory: &Path, files: &mut Vec<String>) {
    for entry in fs::read_dir(directory).expect("the directory reads") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            collect_files(root, &path, files);
        } else {
            let relative = path.strip_prefix(root).expect("under the root");
            files.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// An `arcl` command in `directory` that sees no host Git state.
fn arcl(directory: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_arcl"));
    for variable in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_CEILING_DIRECTORIES",
        "ARCL_AGENT",
    ] {
        command.env_remove(variable);
    }
    let home = Path::new(env!("CARGO_TARGET_TMPDIR")).join("rest-golden-home");
    fs::create_dir_all(&home).expect("the home creates");
    command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .current_dir(directory);
    command
}
