//! `arcl web export` and `arcl web apply`: keep a claude.ai artifact of the
//! web app in step with the project.
//!
//! The artifact cannot reach `arcl serve`, so its database holds the JSON the
//! read endpoints return, one document per read, and an agent copies the
//! files `export` writes into it. Both commands run the same [`ServeApp`] as
//! `arcl serve`, so the documents have the endpoints' shapes and an applied
//! edit gets the same validation, sync lock, export and warnings as a change
//! made in the local app.
//!
//! | Document           | Same JSON as                                    |
//! | ------------------ | ----------------------------------------------- |
//! | `views/work`       | `GET /api/work`                                 |
//! | `views/tree`       | `GET /api/tree`                                 |
//! | `views/tasks`      | `GET /api/records?kind=task`                    |
//! | `views/captures`   | `GET /api/records?kind=capture&status=captured` |
//! | `records/<id>`     | `GET /api/records/<id>`, per task and capture   |
//! | `sources/<id>`     | `GET /api/records/<id>/source`, for each record |
//! | `meta/sync`        | the commit, time and revision of the export     |
//!
//! The page edits a record's file from its `sources/<id>` document and queues
//! the save as `update_source` with the revision that document holds.
//!
//! A document whose JSON is over [`MAX_DOCUMENT_BYTES`] is stored as a
//! header, `{"chunked": {"parts": n, "digest": "..."}}`, and `n` parts at
//! `chunks/<collection>~<id>~<index>`, each `{"part": "..."}`, whose texts
//! join into the document's JSON. A document needs at most [`MAX_PARTS`]
//! parts, the most the page reads.

use arcl_serve::{Reads, Reply};
use serde::Deserialize;
use serde_json::{Value, json};

use super::serve::ServeApp;
use super::support::{project_root_config, read_stdin, write_output};
use super::workspace::discover_vcs;
use super::*;

/// The largest document body `export` writes whole. The artifact database
/// refuses a document over 256 KiB; the rest is headroom for the store's
/// own encoding.
const MAX_DOCUMENT_BYTES: usize = 200 * 1024;

/// The most parts a chunked document may have. The page's reader refuses a
/// header with more, so `export` fails instead of writing one.
const MAX_PARTS: usize = 64;

/// How long an applied edit waits for the project's sync lock. A person runs
/// `apply` and can wait longer than the local app's user waits on a button.
const APPLY_LOCK_WAIT: Duration = Duration::from_secs(60);

/// The file, under the export directory, that lists the documents.
const MANIFEST_FILE: &str = "manifest.json";

/// The collections `export` writes. A path from an older manifest outside
/// them is never deleted.
const COLLECTIONS: &[&str] = &["chunks", "views", "records", "sources", "meta"];

/// The path of the document that describes the export.
const META_DOCUMENT: &str = "meta/sync";

/// One edit the artifact queued, as the `queue/<id>` document holds it with
/// its ID added. Every field the page writes is named, so an entry with any
/// other field fails without running.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct QueuedEdit {
    /// The `queue` document's ID.
    id: String,
    /// One of [`arcl_serve::operations`].
    op: String,
    /// The record the operation changes, for an operation that names one.
    #[serde(default)]
    target: Option<String>,
    /// The JSON object of the command's options, as its endpoint takes it.
    #[serde(default)]
    input: Option<Value>,
    /// When the page queued it, as the page's clock gave it.
    #[serde(default)]
    #[allow(dead_code, reason = "the page records it; apply only checks its shape")]
    created_at: Option<String>,
    /// The artifact user ID of the person who queued it.
    #[serde(default)]
    #[allow(dead_code, reason = "the page records it; apply only checks its shape")]
    created_by: Option<String>,
    /// `queued` until an agent claims it, `applying` while one applies it,
    /// and `failed` after a refusal. Only `queued` entries run; an entry
    /// without a status is skipped, as the page does not list it.
    #[serde(default)]
    status: Option<String>,
    /// The error of an earlier failed apply.
    #[serde(default)]
    #[allow(dead_code, reason = "an earlier apply records it; apply only checks its shape")]
    error: Option<Value>,
}

/// The 64-bit FNV-1a hash, which names a revision of the documents. It
/// tells versions apart; it is not a security boundary.
struct Fnv(u64);

impl Fnv {
    const fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
        // Separate one write from the next, so `ab`+`c` differs from `a`+`bc`.
        self.0 ^= 0xff;
        self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
    }

    fn hex(&self) -> String {
        format!("{:016x}", self.0)
    }
}

/// Run one `web` subcommand and render its result.
pub(super) fn exec_web(command: WebCommand, renderer: &Renderer) -> anyhow::Result<()> {
    match command {
        WebCommand::Export { dir, full } => {
            let app = ServeApp::new(OutputMode::Quiet, APPLY_LOCK_WAIT);
            let manifest = export(&app, &dir, full).map_err(ApplicationError::from_command)?;
            write_output(renderer.render_web_export(&dir, &manifest)?)
        }
        WebCommand::Apply { file } => {
            let entries = read_queue(&file).map_err(ApplicationError::from_command)?;
            let app = ServeApp::new(OutputMode::Quiet, APPLY_LOCK_WAIT);
            let results = entries
                .into_iter()
                .enumerate()
                .map(|(index, entry)| match parse_entry(index, entry) {
                    Ok(edit) => apply_edit(&app, edit),
                    Err(failed) => *failed,
                })
                .collect::<Vec<_>>();
            write_output(renderer.render_web_apply(&results)?)
        }
    }
}

/// Write every document under `dir` and return the manifest, which lists
/// the documents that changed since the export the old manifest describes.
///
/// `changed` compares against the files under `dir`, not against the
/// artifact's database, which `export` cannot read. With `full`, every
/// document is listed as changed, so a sync after an upload that did not
/// finish, or into a database the directory never described, writes them
/// all.
fn export(app: &ServeApp, dir: &Path, full: bool) -> CResult<WebManifest> {
    let mut documents = vec![
        ("views/work".to_owned(), read(app.work(), "views/work")?),
        ("views/tree".to_owned(), read(app.tree(), "views/tree")?),
    ];
    let tasks = read(app.list(&query(&[("kind", "task")])), "views/tasks")?;
    let captures = read(
        app.list(&query(&[("kind", "capture"), ("status", "captured")])),
        "views/captures",
    )?;
    let ids = [&tasks, &captures]
        .into_iter()
        .flat_map(|listing| listing["data"]["records"].as_array().cloned().unwrap_or_default())
        .filter_map(|record| record["id"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    documents.push(("views/tasks".to_owned(), tasks));
    documents.push(("views/captures".to_owned(), captures));
    for id in ids {
        let path = format!("records/{id}");
        let shown = read(app.show(&id), &path)?;
        documents.push((path, shown));
        let path = format!("sources/{id}");
        let source = read(app.source(&id), &path)?;
        documents.push((path, source));
    }

    let mut files = Vec::new();
    let mut revision = Fnv::new();
    for (path, value) in documents {
        let text = value.to_string();
        revision.write(path.as_bytes());
        revision.write(text.as_bytes());
        files.extend(split(&path, text)?);
    }
    let revision = revision.hex();
    let meta = json!({
        "format_version": 1,
        "commit": head_commit(),
        "synced_at": utc_now(),
        "revision": revision,
    });
    files.push((META_DOCUMENT.to_owned(), meta.to_string()));

    let previous = read_manifest(dir)?;
    let mut changed = Vec::new();
    for (path, text) in &files {
        let file = document_file(dir, path);
        let same = fs::read(&file).ok().as_deref() == Some(text.as_bytes());
        if !same {
            write_file(&file, text.as_bytes())?;
        }
        if full || !same {
            changed.push(path.clone());
        }
    }
    let documents = files.into_iter().map(|(path, _)| path).collect::<Vec<_>>();
    let mut removed = Vec::new();
    for path in previous.map(|manifest| manifest.documents).unwrap_or_default() {
        if documents.contains(&path) || !exported_path(&path) {
            continue;
        }
        let file = document_file(dir, &path);
        match fs::remove_file(&file) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(source) => return Err(CommandError::WriteWebFile { path: file, source }),
        }
        removed.push(path);
    }

    let manifest = WebManifest {
        format_version: 1,
        commit: meta["commit"].as_str().map(str::to_owned),
        synced_at: meta["synced_at"].as_str().unwrap_or_default().to_owned(),
        revision,
        documents,
        changed,
        removed,
    };
    let text = serde_json::to_string_pretty(&manifest)
        .map_err(|error| CommandError::WebExport { document: MANIFEST_FILE.to_owned(), message: error.to_string() })?;
    write_file(&dir.join(MANIFEST_FILE), format!("{text}\n").as_bytes())?;
    Ok(manifest)
}

/// Query pairs for a read, as `arcl serve` parses them from a URL.
fn query(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

/// Parse a successful read's envelope, or fail with its error message.
fn read(reply: Reply, document: &str) -> CResult<Value> {
    let parsed = serde_json::from_str::<Value>(reply.body()).unwrap_or(Value::Null);
    if reply.status() != 200 || !parsed.is_object() {
        let message = parsed["error"]["message"].as_str().unwrap_or(reply.body()).to_owned();
        return Err(CommandError::WebExport { document: document.to_owned(), message });
    }
    Ok(parsed)
}

/// Return `text` as the document at `path`, or, when it is over
/// [`MAX_DOCUMENT_BYTES`], as its parts followed by its header.
fn split(path: &str, text: String) -> CResult<Vec<(String, String)>> {
    if !valid_path(path) {
        return Err(CommandError::WebExport {
            document: path.to_owned(),
            message: "the path is not a valid artifact database path".to_owned(),
        });
    }
    if text.len() <= MAX_DOCUMENT_BYTES {
        return Ok(vec![(path.to_owned(), text)]);
    }
    let mut digest = Fnv::new();
    digest.write(text.as_bytes());
    let name = path.replace('/', "~");
    let mut parts = Vec::new();
    let mut part = String::new();
    let mut size = 0;
    for character in text.chars() {
        // A part is stored as a JSON string, where some characters take more
        // bytes than they do in the document.
        let encoded = match character {
            '"' | '\\' | '\n' | '\r' | '\t' | '\u{8}' | '\u{c}' => 2,
            character if character < ' ' => 6,
            character => character.len_utf8(),
        };
        if size + encoded > MAX_DOCUMENT_BYTES {
            parts.push(std::mem::take(&mut part));
            size = 0;
        }
        part.push(character);
        size += encoded;
    }
    parts.push(part);
    if parts.len() > MAX_PARTS {
        return Err(CommandError::WebExport {
            document: path.to_owned(),
            message: format!(
                "the document is {} bytes, more than the {MAX_PARTS} parts of {} KiB the page reads",
                text.len(),
                MAX_DOCUMENT_BYTES / 1024
            ),
        });
    }
    let header = json!({ "chunked": { "parts": parts.len(), "digest": digest.hex() } });
    let mut documents = parts
        .into_iter()
        .enumerate()
        .map(|(index, part)| (format!("chunks/{name}~{index}"), json!({ "part": part }).to_string()))
        .collect::<Vec<_>>();
    documents.push((path.to_owned(), header.to_string()));
    Ok(documents)
}

/// Whether `path` is a document path the artifact database accepts: an even
/// number of segments of letters, digits and `_ - . ~ : @ +`, none of them
/// `.` or `..`.
fn valid_path(path: &str) -> bool {
    let segments = path.split('/').collect::<Vec<_>>();
    segments.len() % 2 == 0
        && segments.len() <= 16
        && path.len() <= 1000
        && segments.iter().all(|segment| {
            !segment.is_empty()
                && segment.len() <= 200
                && *segment != "."
                && *segment != ".."
                && segment
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || "_-.~:@+".contains(character))
        })
}

/// Whether `path` is a valid document path in one of [`COLLECTIONS`], so an
/// export may delete its file.
fn exported_path(path: &str) -> bool {
    valid_path(path)
        && path
            .split_once('/')
            .is_some_and(|(collection, _)| COLLECTIONS.contains(&collection))
}

/// The file under `dir` that holds the document at `path`.
fn document_file(dir: &Path, path: &str) -> PathBuf {
    dir.join(format!("{path}.json"))
}

/// The manifest of the last export to `dir`, if there is one that parses.
fn read_manifest(dir: &Path) -> CResult<Option<WebManifest>> {
    let path = dir.join(MANIFEST_FILE);
    match fs::read(&path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).ok()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(CommandError::ReadWebFile { path, source }),
    }
}

/// Write `bytes` to `path`, creating its directory.
fn write_file(path: &Path, bytes: &[u8]) -> CResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| CommandError::WriteWebFile { path: parent.to_owned(), source })?;
    }
    fs::write(path, bytes).map_err(|source| CommandError::WriteWebFile { path: path.to_owned(), source })
}

/// The commit HEAD names, when the project is in a Git worktree with commits.
fn head_commit() -> Option<String> {
    let (root, _) = project_root_config().ok()?;
    discover_vcs(&root).ok().flatten()?.head_id().ok().flatten()
}

/// The current time in UTC as RFC 3339, to the second.
fn utc_now() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    rfc3339(seconds)
}

/// `seconds` since the Unix epoch as an RFC 3339 UTC time.
fn rfc3339(seconds: u64) -> String {
    let (days, time) = (seconds / 86_400, seconds % 86_400);
    // Howard Hinnant's days-to-civil algorithm, for days since 1970-01-01.
    let shifted = days + 719_468;
    let era = shifted / 146_097;
    let day_of_era = shifted % 146_097;
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time / 3600,
        time % 3600 / 60,
        time % 60
    )
}

/// Read the queue file, or standard input for `-`, which must hold a JSON
/// array. Each element is parsed on its own by [`parse_entry`], so one
/// malformed entry does not stop the rest.
fn read_queue(path: &Path) -> CResult<Vec<Value>> {
    let input = if path == Path::new("-") {
        read_stdin()?.unwrap_or_default()
    } else {
        fs::read_to_string(path).map_err(|source| CommandError::ReadWebFile { path: path.to_owned(), source })?
    };
    match serde_json::from_str::<Value>(&input) {
        Ok(Value::Array(entries)) => Ok(entries),
        Ok(_) => {
            Err(CommandError::InvalidQueue { message: "the file is not a JSON array of queue entries".to_owned() })
        }
        Err(error) => Err(CommandError::InvalidQueue { message: error.to_string() }),
    }
}

/// Parse the queue entry at `index`, or return the `failed` result for an
/// entry that is not a [`QueuedEdit`]. The result names the entry by the
/// `id`, `op` and `target` it has as strings, and its error names the index.
fn parse_entry(index: usize, entry: Value) -> Result<QueuedEdit, Box<AppliedEdit>> {
    let field = |name: &str| entry.get(name).and_then(Value::as_str).map(str::to_owned);
    let (id, op, target) = (field("id"), field("op"), field("target"));
    let reason = match serde_json::from_value::<QueuedEdit>(entry.clone()) {
        Ok(edit) => return Ok(edit),
        Err(_) if !entry.is_object() => "it is not a JSON object".to_owned(),
        Err(error) => error.to_string(),
    };
    let named = id.as_deref().map(|id| format!(" (`{id}`)")).unwrap_or_default();
    let message = format!("queue entry {index}{named} is not a valid queue entry: {reason}");
    let mut envelope = serde_json::from_str::<Value>(Reply::invalid_request(&message).body()).unwrap_or(Value::Null);
    Err(Box::new(AppliedEdit {
        id,
        op,
        target,
        status: AppliedStatus::Failed,
        data: None,
        error: Some(envelope["error"].take()),
        warnings: Vec::new(),
    }))
}

/// Apply one queued edit through `app`, unless its status is not `queued`.
fn apply_edit(app: &ServeApp, edit: QueuedEdit) -> AppliedEdit {
    let mut result = AppliedEdit {
        id: Some(edit.id),
        op: Some(edit.op),
        target: edit.target,
        status: AppliedStatus::Skipped,
        data: None,
        error: None,
        warnings: Vec::new(),
    };
    if edit.status.as_deref() != Some("queued") {
        return result;
    }
    let input = match edit.input {
        None => Value::Object(serde_json::Map::new()),
        Some(input) => input,
    };
    let op = result.op.as_deref().unwrap_or_default();
    let reply = if input.is_object() {
        arcl_serve::apply(op, result.target.as_deref(), input.to_string().as_bytes(), app)
    } else {
        Reply::invalid_request("the input is not a JSON object")
    };
    let mut envelope = serde_json::from_str::<Value>(reply.body()).unwrap_or(Value::Null);
    if reply.status() == 200 {
        result.status = AppliedStatus::Applied;
        result.data = Some(envelope["data"].take());
        result.warnings = envelope["warnings"].as_array().cloned().unwrap_or_default();
    } else {
        result.status = AppliedStatus::Failed;
        result.error = Some(envelope["error"].take());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_path_follows_the_artifact_database_grammar() {
        assert!(valid_path("records/arcl-t-01M5QXMQFGA7QPRKNWPCBVN21N"));
        assert!(valid_path("chunks/views~work~0"));
        for path in [
            "views",
            "views/work/extra",
            "records/..",
            "records/a b",
            "records/",
            "../x/y",
        ] {
            assert!(!valid_path(path), "{path}");
        }
        assert!(!exported_path("queue/q1"));
        assert!(exported_path("views/work"));
    }

    #[test]
    fn split_keeps_a_small_document_whole_and_parts_a_large_one() {
        assert_eq!(
            split("views/tree", "{}".to_owned()).expect("splits"),
            [("views/tree".to_owned(), "{}".to_owned())]
        );
        let body = "\"é\n".repeat(MAX_DOCUMENT_BYTES / 2);
        let text = json!({ "body": body }).to_string();
        let documents = split("records/r1", text.clone()).expect("splits");
        let (path, header) = documents.last().expect("a header");
        assert_eq!(path, "records/r1");
        let header: Value = serde_json::from_str(header).expect("JSON");
        let parts = &documents[..documents.len() - 1];
        assert_eq!(header["chunked"]["parts"], parts.len());
        let mut joined = String::new();
        for (index, (path, part)) in parts.iter().enumerate() {
            assert_eq!(*path, format!("chunks/records~r1~{index}"));
            assert!(part.len() <= MAX_DOCUMENT_BYTES + 16, "{path} is {} bytes", part.len());
            let part: Value = serde_json::from_str(part).expect("JSON");
            joined.push_str(part["part"].as_str().expect("a part"));
        }
        assert_eq!(joined, text);
    }

    #[test]
    fn split_refuses_a_document_over_the_part_limit() {
        let text = "a".repeat(MAX_DOCUMENT_BYTES * MAX_PARTS);
        assert_eq!(split("views/work", text.clone()).expect("splits").len(), MAX_PARTS + 1);
        let error = split("views/work", format!("{text}a")).expect_err("too many parts");
        assert!(error.to_string().contains("64 parts"), "{error}");
    }

    #[test]
    fn fnv_digest_matches_the_page() {
        // `artifact.test.ts` checks the page's `digest` against the same values.
        for (text, expected) in [
            ("", "af64724c8602eb6e"),
            ("{}", "c736581983dda06e"),
            ("{\"title\":\"é ✓\"}", "8e84190b8ddb4463"),
        ] {
            let mut digest = Fnv::new();
            digest.write(text.as_bytes());
            assert_eq!(digest.hex(), expected, "{text}");
        }
    }

    #[test]
    fn rfc3339_converts_epoch_seconds_to_a_utc_date() {
        for (seconds, expected) in [
            (0, "1970-01-01T00:00:00Z"),
            (951_782_400, "2000-02-29T00:00:00Z"),
            (951_868_799, "2000-02-29T23:59:59Z"),
            (978_307_199, "2000-12-31T23:59:59Z"),
            (1_709_251_199, "2024-02-29T23:59:59Z"),
            (1_761_373_800, "2025-10-25T06:30:00Z"),
            (4_107_542_400, "2100-03-01T00:00:00Z"),
        ] {
            assert_eq!(rfc3339(seconds), expected, "{seconds}");
        }
        assert_eq!(utc_now().len(), 20);
    }
}
