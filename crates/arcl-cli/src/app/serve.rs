//! `arcl serve`: the web app and its endpoints over the CLI's commands.
//!
//! Each `GET /api` read opens the database and runs one named GraphQL query
//! (see [`rest`]), with no workspace sync, and answers the envelope the CLI's
//! `--json` output gives for the same read. `POST /graphql` is a read too: it
//! runs the same query as `arcl query`, with the same response and limits,
//! but without the workspace sync `arcl query` runs first. A read never
//! changes records, though opening the database can run its migrations. Each
//! mutation runs the same `arcl task`, `arcl capture`, `arcl spec` or
//! `arcl plan` command as the CLI, with the same sync around it: it takes the
//! sync lock, syncs, changes the record and syncs again to export it. The
//! server holds the lock only during a mutation, so other arcl commands run
//! while it serves.
//!
//! A mutation waits at most [`LOCK_WAIT`] for the lock. When another command
//! holds it longer, the mutation fails with `application_error`, exit code 1,
//! and status 503 with `Retry-After`, and changes nothing. A mutation's reply
//! lists the warnings its syncs gave in `warnings`, as objects with the
//! `code` and `message` the CLI prints to stderr under `--json`.

use std::ffi::OsString;

use arcl_serve::{
    CapturePromotion, Completion, GraphqlRequest, Handoff, Mutations, NewCapture, NewPlan, NewSpec, NewTask, PlanInput,
    Reads, RecordSourceEdit, Reply, ServeError, ServeOptions, Server, TaskEdit, TaskStart,
};

use super::graphql::run_query;
use super::rest;
use super::support::{RecordScope, parse_plan_document, parse_plan_id};
use super::*;

/// How long a mutation waits for the project's sync lock. The CLI waits as
/// long as it takes; the web app's user is waiting on a button instead.
const LOCK_WAIT: Duration = Duration::from_secs(3);

/// The environment variable that sets the token of `arcl serve`, unless
/// `--token-file` is given.
const TOKEN_VARIABLE: &str = "ARCL_SERVE_TOKEN";

/// The most bytes `--token-file` may hold, so a device such as `/dev/zero`
/// or a stray large file fails instead of filling memory.
const TOKEN_FILE_LIMIT: u64 = 4096;

/// Where a token the user supplied came from.
#[derive(Debug, Eq, PartialEq)]
enum TokenSource {
    /// The `--token-file` at this path.
    File(PathBuf),
    /// [`TOKEN_VARIABLE`].
    Variable,
}

impl TokenSource {
    /// The source as startup output names it: the file's path or the
    /// variable's name.
    fn name(&self) -> String {
        match self {
            Self::File(path) => path.display().to_string(),
            Self::Variable => TOKEN_VARIABLE.to_owned(),
        }
    }

    /// The source as an error names it.
    fn described(&self) -> String {
        match self {
            Self::File(path) => format!("the token in `{}`", path.display()),
            Self::Variable => TOKEN_VARIABLE.to_owned(),
        }
    }
}

/// The endpoints, answering with the CLI's `--json` envelopes. `arcl web`
/// runs the same reads and mutations for a claude.ai artifact.
pub(super) struct ServeApp {
    renderer: Renderer,
    /// How a mutation's sync warnings print to stderr.
    mode: OutputMode,
    /// How long a mutation waits for the project's sync lock.
    lock_wait: Duration,
}

impl ServeApp {
    /// Reads and mutations that print sync warnings to stderr as `mode` does
    /// and wait at most `lock_wait` for the sync lock.
    pub(super) fn new(mode: OutputMode, lock_wait: Duration) -> Self {
        Self { renderer: Renderer::new(OutputMode::Json, ColorChoice::Never), mode, lock_wait }
    }

    /// Run a task command as the CLI runs a write.
    fn task(&self, command: TaskCommand) -> Reply {
        self.write(|| run_task(command, &self.renderer))
    }

    /// Run `command` as the CLI runs a write. The warnings its syncs give go
    /// in the reply and to the server's stderr.
    fn write(&self, command: impl FnOnce() -> Result<Option<String>, ApplicationError>) -> Reply {
        let scoped = || {
            let _records = RecordScope::enter();
            command()
        };
        match synced_write(self.lock_wait, scoped) {
            Ok((body, warnings)) => {
                let body = with_warnings(body.unwrap_or_default(), &warnings);
                write_warnings(self.mode, warnings);
                Reply::success(body)
            }
            Err(error) if error.lock_busy() => Reply::busy(error.json()),
            Err(error) => Reply::failure(error.exit_code(), error.json()),
        }
    }

    /// Replace a record's file under the sync lock, between the two syncs a
    /// write runs. The blocker rule warnings the edit gives join the syncs'.
    fn edit(&self, id: &str, input: RecordSourceEdit) -> Reply {
        let edited = synced_write(self.lock_wait, || {
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            let edit = edit_record(&mut database, id, &input.source, &input.revision)
                .map_err(ApplicationError::from_command)?;
            let body = self.renderer.render_record_source(&edit.source).map_err(|error| {
                ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
            })?;
            Ok((body, edit.warnings))
        });
        match edited {
            Ok(((body, found), mut warnings)) => {
                warnings.splice(0..0, Warning::blocker_edges(found));
                let body = with_warnings(body.unwrap_or_default(), &warnings);
                write_warnings(self.mode, warnings);
                Reply::success(body)
            }
            Err(error) if error.lock_busy() => Reply::busy(error.json()),
            Err(error) => Reply::failure(error.exit_code(), error.json()),
        }
    }
}

impl Reads for ServeApp {
    fn list(&self, query: &[(String, String)]) -> Reply {
        match list_args(query) {
            Ok(filters) => reply(rest::records(&filters, &self.renderer)),
            Err(message) => Reply::invalid_request(&message),
        }
    }

    fn show(&self, id: &str) -> Reply {
        reply(rest::record(id, &self.renderer))
    }

    fn source(&self, id: &str) -> Reply {
        reply(rest::source(id, &self.renderer))
    }

    fn ready(&self, query: &[(String, String)]) -> Reply {
        match ready_args(query) {
            Ok(filters) => reply(rest::ready(&filters, &self.renderer)),
            Err(message) => Reply::invalid_request(&message),
        }
    }

    fn explain(&self, task_id: &str) -> Reply {
        reply(rest::explain(task_id, &self.renderer))
    }

    fn context(&self, task_id: &str) -> Reply {
        reply(rest::context(task_id, &self.renderer))
    }

    fn tree(&self) -> Reply {
        reply(rest::tree(&self.renderer))
    }

    fn work(&self) -> Reply {
        reply(rest::work(&self.renderer))
    }

    fn graphql(&self, input: GraphqlRequest) -> Reply {
        let mut request = arcl_graphql::Request::new(input.query);
        if let Some(variables) = input.variables {
            request = request.variables(arcl_graphql::Variables::from_json(variables));
        }
        if let Some(operation) = input.operation_name {
            request = request.operation_name(operation);
        }
        reply(run_query(request).map(|(response, _)| Some(response)))
    }
}

impl Mutations for ServeApp {
    fn create_task(&self, input: NewTask) -> Reply {
        self.task(TaskCommand::Create {
            title: input.title,
            spec: input.spec,
            plan: input.plan,
            phase: input.phase,
            parent: input.parent,
            priority: input.priority.unwrap_or_else(|| "normal".to_owned()),
            position: 0,
            blocked_by: input.blocked_by,
            body: MarkdownArgs { body: input.body, body_file: None },
        })
    }

    fn update_task(&self, id: &str, input: TaskEdit) -> Reply {
        self.task(TaskCommand::Update {
            id: id.to_owned(),
            title: input.title,
            body: MarkdownArgs { body: input.body, body_file: None },
            priority: input.priority,
            position: None,
            spec: None,
            no_spec: false,
            plan: None,
            no_plan: false,
            phase: None,
            no_phase: false,
            parent: None,
            no_parent: false,
        })
    }

    fn start_task(&self, id: &str, input: TaskStart) -> Reply {
        self.task(TaskCommand::Start { id: id.to_owned(), force: input.force, claim: None })
    }

    fn park_task(&self, id: &str) -> Reply {
        self.task(TaskCommand::Park { id: id.to_owned() })
    }

    fn unpark_task(&self, id: &str) -> Reply {
        self.task(TaskCommand::Unpark { id: id.to_owned() })
    }

    fn hand_off_task(&self, id: &str, input: Handoff) -> Reply {
        self.task(TaskCommand::Handoff { id: id.to_owned(), note: input.note, note_file: None })
    }

    fn complete_task(&self, id: &str, input: Completion) -> Reply {
        if input.allow_open_children && input.with_subtasks {
            return Reply::invalid_request("`allow_open_children` and `with_subtasks` cannot be used together");
        }
        self.task(TaskCommand::Complete {
            id: id.to_owned(),
            allow_open_children: input.allow_open_children,
            with_subtasks: input.with_subtasks,
            evidence: input.evidence,
            evidence_file: None,
        })
    }

    fn create_capture(&self, input: NewCapture) -> Reply {
        let command = CaptureCommand::Create {
            title: input.title,
            body: MarkdownArgs { body: input.body, body_file: None },
            from: None,
        };
        self.write(|| run_capture(command, &self.renderer))
    }

    fn promote_capture(&self, id: &str, input: CapturePromotion) -> Reply {
        let command = CaptureCommand::Promote {
            id: id.to_owned(),
            target: None,
            to: Some(input.to),
            title: input.title,
            body: MarkdownArgs { body: input.body, body_file: None },
            acceptance_criteria: input.acceptance_criteria,
            acceptance_criteria_file: None,
            spec: input.spec,
            plan: input.plan,
            phase: input.phase,
            parent: input.parent,
            priority: input.priority.unwrap_or_else(|| "normal".to_owned()),
            position: 0,
        };
        self.write(|| run_capture(command, &self.renderer))
    }

    fn discard_capture(&self, id: &str) -> Reply {
        let command = CaptureCommand::Discard { id: id.to_owned() };
        self.write(|| run_capture(command, &self.renderer))
    }

    fn create_spec(&self, input: NewSpec) -> Reply {
        let command = SpecCommand::Create {
            title: input.title,
            body: MarkdownArgs { body: input.body, body_file: None },
            acceptance: AcceptanceArgs {
                acceptance_criteria: input.acceptance_criteria,
                acceptance_criteria_file: None,
            },
        };
        self.write(|| run_spec(command, &self.renderer))
    }

    fn create_plan(&self, input: NewPlan) -> Reply {
        let command = PlanCommand::Create {
            title: input.title,
            spec: input.spec,
            body: MarkdownArgs { body: input.body, body_file: None },
            input: None,
            no_input: true,
        };
        self.write(|| run_plan(command, &self.renderer))
    }

    fn apply_plan(&self, id: &str, input: PlanInput) -> Reply {
        self.write(|| {
            let id = parse_plan_id(id).map_err(ApplicationError::from_command)?;
            let document = parse_plan_document(&input.document).map_err(ApplicationError::from_command)?;
            apply_plan_document(id, &document, &self.renderer)
        })
    }

    fn edit_record(&self, id: &str, input: RecordSourceEdit) -> Reply {
        self.edit(id, input)
    }
}

/// Bind the server, print its URL and serve until interrupted. With
/// `--project`, the server runs in that directory, as if started there. A
/// relative `--token-file` is resolved against the directory `serve` started
/// in, before it enters the project, so a file in the project cannot stand in
/// for it. The database opens first, so outside a project `serve` fails like
/// other commands instead of serving endpoints that all fail.
///
/// The token comes from `--token-file`, then [`TOKEN_VARIABLE`], and is never
/// printed. Without either, each run mints one, and binding an address off
/// loopback fails. With `--api`, the server answers only `/api`, `/graphql`
/// and `/healthz`.
pub(super) fn exec_serve(options: ServeArgs, renderer: &Renderer) -> anyhow::Result<()> {
    let ServeArgs { port, api, bind, token_file, allow_hosts, project } = options;
    let token_file = token_file
        .map(|path| {
            std::path::absolute(&path)
                .with_context(|| format!("could not resolve the token file path `{}`", path.display()))
        })
        .transpose()?;
    if let Some(project) = &project {
        std::env::set_current_dir(project)
            .with_context(|| format!("could not enter the project directory `{}`", project.display()))?;
    }
    drop(open_database().map_err(ApplicationError::from_command)?);
    let supplied = supplied_token(token_file.as_deref(), std::env::var_os(TOKEN_VARIABLE))?;
    let (token, source) = supplied.map_or((None, None), |(token, source)| (Some(token), Some(source)));
    let other_hosts_refused = bind.is_unspecified() && allow_hosts.is_empty();
    let options = ServeOptions { address: bind, port, token, api_only: api, allowed_hosts: allow_hosts };
    let server = Server::bind_with(options).map_err(|error| match (error, &source) {
        (error @ ServeError::WeakToken, Some(source)) => anyhow::anyhow!("{} is not valid: {error}", source.described()),
        (ServeError::OpenBind(address), _) => anyhow::anyhow!(
            "--bind {address} needs a token from --token-file or {TOKEN_VARIABLE}; arcl serve mints one only on loopback"
        ),
        (error @ ServeError::AllowedHost(_), _) => anyhow::anyhow!("--allow-host: {error}"),
        (error, _) => anyhow::Error::new(error).context("could not start the web app server"),
    })?;
    let name = source.as_ref().map(TokenSource::name);
    let (url, token) = match &name {
        Some(name) => (server.origin(), ServeToken::Supplied(name)),
        None => (server.url(), ServeToken::Minted(server.token())),
    };
    let start = ServeStart { url: &url, port: server.port(), token, api, other_hosts_refused };
    write_output(renderer.render_serve(&start)?)?;
    let app = ServeApp::new(renderer.mode(), LOCK_WAIT);
    server.run(&app, &app).context("the web app server stopped")?;
    Ok(())
}

/// Add `warnings` to the JSON envelope `body` as a `warnings` array of
/// `{"code", "message"}` objects. With no warnings, `body` is unchanged.
fn with_warnings(body: String, warnings: &[Warning]) -> String {
    if warnings.is_empty() {
        return body;
    }
    let Ok(serde_json::Value::Object(mut envelope)) = serde_json::from_str(&body) else {
        return body;
    };
    let warnings = warnings
        .iter()
        .map(|warning| serde_json::json!({ "code": warning.code, "message": warning.message }))
        .collect();
    envelope.insert("warnings".to_owned(), serde_json::Value::Array(warnings));
    serde_json::Value::Object(envelope).to_string()
}

/// Turn a read's result into a reply, with the error envelope `main` prints
/// under `--json` for a failure.
fn reply(result: anyhow::Result<Option<String>>) -> Reply {
    match result {
        Ok(body) => Reply::success(body.unwrap_or_default()),
        Err(error) => match error.downcast_ref::<ApplicationError>() {
            Some(error) => Reply::failure(error.exit_code(), error.json()),
            None => {
                let body = serde_json::json!({
                    "format_version": 1,
                    "error": { "code": "application_error", "message": format!("{error:#}") },
                });
                Reply::failure(1, body.to_string())
            }
        },
    }
}

/// `arcl list` filters from query pairs named like its flags.
fn list_args(query: &[(String, String)]) -> Result<ListArgs, String> {
    let mut args = ListArgs::default();
    for (name, value) in query {
        match name.as_str() {
            "kind" => set_once(&mut args.kind, name, value)?,
            "status" => args.status.push(value.clone()),
            "priority" => args.priority.push(value.clone()),
            "release" => set_once(&mut args.release, name, value)?,
            "spec" => set_once(&mut args.spec, name, value)?,
            "plan" => set_once(&mut args.plan, name, value)?,
            "phase" => set_once(&mut args.phase, name, value)?,
            "parent" => set_once(&mut args.parent, name, value)?,
            _ => return Err(format!("unknown filter `{name}`")),
        }
    }
    Ok(args)
}

/// `arcl ready` filters from query pairs named like its flags.
fn ready_args(query: &[(String, String)]) -> Result<ReadyArgs, String> {
    let mut args = ReadyArgs::default();
    for (name, value) in query {
        match name.as_str() {
            "priority" => args.priority.push(value.clone()),
            "spec" => set_once(&mut args.spec, name, value)?,
            "plan" => set_once(&mut args.plan, name, value)?,
            "phase" => set_once(&mut args.phase, name, value)?,
            "parent" => set_once(&mut args.parent, name, value)?,
            _ => return Err(format!("unknown filter `{name}`")),
        }
    }
    Ok(args)
}

/// Set a single-valued filter, refusing a second value as clap does.
fn set_once(slot: &mut Option<String>, name: &str, value: &str) -> Result<(), String> {
    if slot.is_some() {
        return Err(format!("the `{name}` filter takes one value"));
    }
    *slot = Some(value.to_owned());
    Ok(())
}

/// The token the user supplied, and where from: the file at `token_file`
/// when given, else `variable`, the value of [`TOKEN_VARIABLE`], when set.
/// The file's trailing newline is dropped. A file that cannot be read, is
/// not a regular file, holds more than [`TOKEN_FILE_LIMIT`] bytes, or holds
/// nothing but whitespace, fails. A variable that is not UTF-8 counts as
/// empty, which the server refuses as an invalid token.
fn supplied_token(
    token_file: Option<&Path>, variable: Option<OsString>,
) -> anyhow::Result<Option<(String, TokenSource)>> {
    if let Some(path) = token_file {
        let content = read_token_file(path)?;
        let token = content.trim_end_matches(['\n', '\r']);
        if token.trim().is_empty() {
            anyhow::bail!("the token file `{}` is empty", path.display());
        }
        return Ok(Some((token.to_owned(), TokenSource::File(path.to_owned()))));
    }
    Ok(variable.map(|value| (value.into_string().unwrap_or_default(), TokenSource::Variable)))
}

/// The text of the token file at `path`. The file must be a regular file,
/// checked before it is opened so a FIFO cannot block the start, and is read
/// no further than one byte past [`TOKEN_FILE_LIMIT`].
fn read_token_file(path: &Path) -> anyhow::Result<String> {
    use std::io::Read as _;

    let unreadable = || format!("could not read the token file `{}`", path.display());
    let metadata = fs::metadata(path).with_context(unreadable)?;
    if !metadata.is_file() {
        anyhow::bail!("the token file `{}` is not a regular file", path.display());
    }
    let mut content = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(TOKEN_FILE_LIMIT + 1).read_to_end(&mut content))
        .with_context(unreadable)?;
    if content.len() as u64 > TOKEN_FILE_LIMIT {
        anyhow::bail!(
            "the token file `{}` is larger than {TOKEN_FILE_LIMIT} bytes",
            path.display()
        );
    }
    String::from_utf8(content).map_err(|_| anyhow::anyhow!("the token file `{}` is not UTF-8", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supplied_token_reads_the_file_first() {
        let directory = tempfile::tempdir().expect("a directory");
        let path = directory.path().join("token");
        let token = "Abc123".repeat(6);
        for content in [
            token.clone(),
            format!("{token}\n"),
            format!("{token}\r\n"),
            format!("{token}\n\n"),
        ] {
            fs::write(&path, &content).expect("the file writes");
            let supplied = supplied_token(Some(&path), Some(OsString::from("other"))).expect("a token");
            assert_eq!(
                supplied,
                Some((token.clone(), TokenSource::File(path.clone()))),
                "{content:?}"
            );
        }
        // Only the line ending goes; other whitespace stays for the server to refuse.
        fs::write(&path, format!("{token} \n")).expect("the file writes");
        let supplied = supplied_token(Some(&path), None).expect("a token");
        assert_eq!(supplied.map(|(token, _)| token), Some(format!("{token} ")));
    }

    #[test]
    fn supplied_token_refuses_an_empty_or_missing_file() {
        let directory = tempfile::tempdir().expect("a directory");
        let path = directory.path().join("token");
        for content in ["", "\n", "  \n", "\t\r\n"] {
            fs::write(&path, content).expect("the file writes");
            let error = supplied_token(Some(&path), Some(OsString::from("Abc123".repeat(6)))).expect_err(content);
            assert_eq!(
                error.to_string(),
                format!("the token file `{}` is empty", path.display())
            );
        }
        let missing = directory.path().join("missing");
        let error = supplied_token(Some(&missing), None).expect_err("a missing file");
        assert!(
            error
                .to_string()
                .starts_with(&format!("could not read the token file `{}`", missing.display())),
            "{error}"
        );
    }

    #[test]
    fn supplied_token_refuses_a_large_or_irregular_file() {
        let directory = tempfile::tempdir().expect("a directory");
        let path = directory.path().join("token");
        fs::write(&path, "a".repeat(4096)).expect("the file writes");
        assert!(supplied_token(Some(&path), None).expect("a token").is_some());
        fs::write(&path, "a".repeat(4097)).expect("the file writes");
        let error = supplied_token(Some(&path), None).expect_err("a large file");
        assert_eq!(
            error.to_string(),
            format!("the token file `{}` is larger than 4096 bytes", path.display())
        );
        for irregular in [directory.path(), Path::new("/dev/zero")] {
            let error = supplied_token(Some(irregular), None).expect_err("not a regular file");
            assert_eq!(
                error.to_string(),
                format!("the token file `{}` is not a regular file", irregular.display())
            );
        }
    }

    #[test]
    fn supplied_token_falls_back_to_the_variable() {
        let token = "Abc123".repeat(6);
        let supplied = supplied_token(None, Some(OsString::from(&token))).expect("a token");
        assert_eq!(supplied, Some((token, TokenSource::Variable)));
        let empty = supplied_token(None, Some(OsString::new())).expect("an empty token");
        assert_eq!(empty, Some((String::new(), TokenSource::Variable)));
        assert_eq!(supplied_token(None, None).expect("no token"), None);
    }
}
