//! `arcl serve`: the web app and its endpoints over the CLI's commands.
//!
//! Each read opens the database and runs the same read as the CLI, with no
//! workspace sync. A read never changes records, though opening the database
//! can run its migrations. Each mutation runs the same `arcl task` or
//! `arcl capture` command as the CLI, with the same sync around it: it takes
//! the sync lock, syncs, changes the record and syncs again to export it. The
//! server holds the lock only during a mutation, so other arcl commands run
//! while it serves.

use arcl_serve::{Completion, Handoff, Mutations, NewCapture, NewTask, Reads, Reply, Server, TaskEdit, TaskStart};

use super::reads::*;
use super::*;

/// The endpoints, answering with the CLI's `--json` envelopes.
struct ServeApp {
    renderer: Renderer,
    /// How `arcl serve` prints a mutation's sync warnings to stderr.
    mode: OutputMode,
}

impl ServeApp {
    /// Run a task command as the CLI runs a write, and print the warnings
    /// its syncs give to the server's stderr.
    fn write(&self, command: TaskCommand) -> Reply {
        self.synced(|| run_task(command, &self.renderer))
    }

    /// Run `command` under the CLI's write sync, printing its sync warnings.
    fn synced(&self, command: impl FnOnce() -> Result<Option<String>, ApplicationError>) -> Reply {
        match synced_write(command) {
            Ok((body, warnings)) => {
                write_warnings(self.mode, warnings);
                Reply::success(body.unwrap_or_default())
            }
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
}

impl Mutations for ServeApp {
    fn create_task(&self, input: NewTask) -> Reply {
        self.write(TaskCommand::Create {
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
        self.write(TaskCommand::Update {
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
        self.write(TaskCommand::Start { id: id.to_owned(), force: input.force })
    }

    fn park_task(&self, id: &str) -> Reply {
        self.write(TaskCommand::Park { id: id.to_owned() })
    }

    fn unpark_task(&self, id: &str) -> Reply {
        self.write(TaskCommand::Unpark { id: id.to_owned() })
    }

    fn hand_off_task(&self, id: &str, input: Handoff) -> Reply {
        self.write(TaskCommand::Handoff { id: id.to_owned(), note: input.note, note_file: None })
    }

    fn complete_task(&self, id: &str, input: Completion) -> Reply {
        if input.allow_open_children && input.with_subtasks {
            return Reply::invalid_request("`allow_open_children` and `with_subtasks` cannot be used together");
        }
        self.write(TaskCommand::Complete {
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
        self.synced(|| run_capture(command, &self.renderer))
    }
}

/// Bind the server, print its URL and serve until interrupted. The database
/// opens first, so outside a project `serve` fails like other commands
/// instead of serving endpoints that all fail.
pub(super) fn exec_serve(port: Option<u16>, renderer: &Renderer) -> anyhow::Result<()> {
    drop(open_database().map_err(ApplicationError::from_command)?);
    let server = Server::bind(port).context("could not start the web app server")?;
    write_output(renderer.render_serve(&server.url())?)?;
    let app = ServeApp { renderer: Renderer::new(OutputMode::Json, ColorChoice::Never), mode: renderer.mode() };
    server.run(&app, &app).context("the web app server stopped")?;
    Ok(())
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
