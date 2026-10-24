//! `arcl serve`: the web app and its read endpoints over the read commands.
//!
//! Each request opens the database and runs the same read as the CLI, with
//! no workspace sync, so a GET never writes. The server holds no sync lock,
//! so other arcl commands run while it serves.

use arcl_serve::{Reads, Reply, Server};

use super::reads::*;
use super::*;

/// The read endpoints, answering with the CLI's `--json` envelopes.
struct ServeReads {
    renderer: Renderer,
}

impl Reads for ServeReads {
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

/// Bind the server, print its URL and serve until interrupted.
pub(super) fn exec_serve(port: Option<u16>, renderer: &Renderer) -> anyhow::Result<()> {
    let server = Server::bind(port).context("could not start the web app server")?;
    write_output(renderer.render_serve(&server.url())?)?;
    let reads = ServeReads { renderer: Renderer::new(OutputMode::Json, ColorChoice::Never) };
    server.run(&reads).context("the web app server stopped")?;
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
