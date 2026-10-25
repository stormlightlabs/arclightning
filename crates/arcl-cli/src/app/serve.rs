//! `arcl serve`: the web app and its endpoints over the CLI's commands.
//!
//! Each read opens the database and runs the same read as the CLI, with no
//! workspace sync. A read never changes records, though opening the database
//! can run its migrations. Each mutation runs the same `arcl task` or
//! `arcl capture`, `arcl spec` or `arcl plan` command as the CLI, with the
//! same sync around it: it takes the sync lock, syncs, changes the record and
//! syncs again to export it. The server holds the lock only during a
//! mutation, so other arcl commands run while it serves.
//!
//! A mutation waits at most [`LOCK_WAIT`] for the lock. When another command
//! holds it longer, the mutation fails with `application_error`, exit code 1,
//! and status 503 with `Retry-After`, and changes nothing. A mutation's reply
//! lists the warnings its syncs gave in `warnings`, as objects with the
//! `code` and `message` the CLI prints to stderr under `--json`.

use arcl_serve::{
    CapturePromotion, Completion, Handoff, Mutations, NewCapture, NewPlan, NewSpec, NewTask, PlanInput, Reads, Reply,
    Server, TaskEdit, TaskStart,
};

use super::reads::*;
use super::support::{parse_plan_document, parse_plan_id};
use super::*;

/// How long a mutation waits for the project's sync lock. The CLI waits as
/// long as it takes; the web app's user is waiting on a button instead.
const LOCK_WAIT: Duration = Duration::from_secs(3);

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
        match synced_write(self.lock_wait, command) {
            Ok((body, warnings)) => {
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
            Ok(filters) => reply(read_list(filters, &self.renderer)),
            Err(message) => Reply::invalid_request(&message),
        }
    }

    fn show(&self, id: &str) -> Reply {
        reply(read_show(id, &self.renderer))
    }

    fn ready(&self, query: &[(String, String)]) -> Reply {
        match ready_args(query) {
            Ok(filters) => reply(read_ready(filters, &self.renderer)),
            Err(message) => Reply::invalid_request(&message),
        }
    }

    fn explain(&self, task_id: &str) -> Reply {
        reply(read_explain(task_id, &self.renderer))
    }

    fn context(&self, task_id: &str) -> Reply {
        reply(read_context(task_id, &self.renderer))
    }

    fn tree(&self) -> Reply {
        reply(read_tree(None, &self.renderer))
    }

    fn work(&self) -> Reply {
        reply(read_work(&self.renderer))
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
        self.task(TaskCommand::Start { id: id.to_owned(), force: input.force })
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
        let command =
            CaptureCommand::Create { title: input.title, body: MarkdownArgs { body: input.body, body_file: None } };
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
}

/// Bind the server, print its URL and serve until interrupted. The database
/// opens first, so outside a project `serve` fails like other commands
/// instead of serving endpoints that all fail.
pub(super) fn exec_serve(port: Option<u16>, renderer: &Renderer) -> anyhow::Result<()> {
    drop(open_database().map_err(ApplicationError::from_command)?);
    let server = Server::bind(port).context("could not start the web app server")?;
    write_output(renderer.render_serve(&server.url())?)?;
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
