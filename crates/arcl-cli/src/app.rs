use std::fs::{self, File, OpenOptions};
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;
use thiserror::Error;

use arcl_core::domain::*;
use arcl_repo::vcs::{GixVcs, Vcs, VcsError};
use arcl_repo::workspace::*;
use arcl_store::{
    CaptureTaskPromotion, CaptureUpdate, ConnectedGraph, Database, NoteUpdate, PhaseUpdate, PlanUpdate,
    PlanningReadyFilter, PlanningTaskCreate, PlanningTaskUpdate, SpecUpdate, StorageError,
};

use crate::{cli::*, output::*};

const ARCL_DIRECTORY: &str = ".arcl";
const CONFIG_FILE: &str = "config.toml";
const DATABASE_FILE: &str = "arcl.db";
const GITIGNORE_FILE: &str = ".gitignore";
const REQUIRED_GITIGNORE_ENTRIES: &[&str] = &["/arcl.db", "/arcl.db-*", "/*.tmp", "/conflicts/"];
const WORKSPACE_DIRECTORIES: &[&str] = &["captures", "releases", "specs", "plans", "phases", "tasks", "notes"];

mod auto_sync;
mod commands;
mod error;
mod init;
mod prime;
mod query;
mod reads;
mod serve;
mod skill;
mod support;
mod web;
mod workspace;

use commands::{
    apply_plan_document, claim_name, exec_dependency, exec_gate, exec_note, exec_plan, exec_release, exec_spec,
    exec_task, run_capture, run_plan, run_spec, run_task,
};
pub use error::ApplicationError;
use error::{CResult, CommandError};
use init::initialize;
use prime::{PRIME_LOCK_WAIT, read_prime};
use query::resolve_ready_filter;
use reads::{read_context, read_explain, read_list, read_ready, read_show, read_tree};
use serve::exec_serve;
use skill::exec_skill;
use support::{RecordScope, open_database, write_output};
use web::exec_web;
use workspace::exec_workspace;

use auto_sync::{Access, auto_sync, command_access, export_after_write, refuse_write, synced_write, workspace_roots};

/// One warning for stderr. `code` names its kind under `--json`.
struct Warning {
    code: &'static str,
    message: String,
}

impl Warning {
    /// A sync or export that did not run, while the command went on.
    fn workspace_sync(message: String) -> Self {
        Self { code: "workspace_sync", message }
    }

    /// A blocker rule violation a workspace import loaded because it did not
    /// introduce it.
    fn blocker_edges(messages: Vec<String>) -> impl Iterator<Item = Self> {
        messages
            .into_iter()
            .map(|message| Self { code: "blocker_edge", message })
    }
}

/// Run one parsed CLI invocation at the application boundary.
pub fn run(cli: Cli) -> anyhow::Result<()> {
    let mode = if cli.json {
        OutputMode::Json
    } else if cli.plain {
        OutputMode::Plain
    } else if cli.quiet {
        OutputMode::Quiet
    } else {
        OutputMode::Human
    };
    let renderer = Renderer::new(mode, cli.color);

    let access = command_access(cli.command.as_ref());
    // `web export` stamps its documents with the time, so it must not export
    // a database the sync did not bring up to date.
    let needs_sync = matches!(cli.command, Some(Command::Web { command: WebCommand::Export { .. } }));
    // `prime` runs at session start, so it waits a bounded time for the lock
    // and, failing that, shows the database without a sync.
    let prime = matches!(cli.command, Some(Command::Prime));
    // The lock is held from the first sync until `run` returns.
    let (roots, synced, _lock) = match access {
        Access::Untracked => (None, Ok(Vec::new()), None),
        _ => match workspace_roots() {
            Ok(Some(roots)) => match if prime { roots.lock_within(PRIME_LOCK_WAIT) } else { roots.lock() } {
                Ok(lock) => {
                    let synced = auto_sync(&roots);
                    (Some(roots), synced, Some(lock))
                }
                Err(error) => (None, Err(error), None),
            },
            Ok(None) => (None, Ok(Vec::new()), None),
            Err(error) => (None, Err(error), None),
        },
    };
    let mut warnings = Vec::new();
    match synced {
        Ok(found) => warnings.extend(Warning::blocker_edges(found)),
        // A missing database is the command's own error to report, once.
        Err(CommandError::NotInitialized { .. }) => {}
        Err(error) if access == Access::Write => return Err(refuse_write(error).into()),
        Err(error) if needs_sync => {
            return Err(ApplicationError::from_command(CommandError::ExportSync(Box::new(error))).into());
        }
        Err(error) => warnings.push(Warning::workspace_sync(format!(
            "the workspace did not sync, so this shows the database as it is: {error}"
        ))),
    }
    // Under `--json` warnings wait for the command to succeed, so stderr
    // holds at most one JSON value.
    if mode != OutputMode::Json {
        write_warnings(mode, std::mem::take(&mut warnings));
    }

    dispatch(cli.command, &renderer, &mut warnings)?;

    if access == Access::Write
        && let Some(roots) = &roots
    {
        export_after_write(roots, &mut warnings);
    }
    write_warnings(mode, warnings);
    Ok(())
}

/// Write warnings to stderr: one plain line each, a single one-line JSON
/// envelope under `--json` with a `warnings` array of `code` and `message`
/// objects, or nothing under `--quiet`.
fn write_warnings(mode: OutputMode, warnings: Vec<Warning>) {
    if warnings.is_empty() {
        return;
    }
    match mode {
        OutputMode::Quiet => {}
        OutputMode::Json => {
            let warnings = warnings
                .iter()
                .map(|warning| serde_json::json!({ "code": warning.code, "message": warning.message }))
                .collect::<Vec<_>>();
            eprintln!("{}", serde_json::json!({ "format_version": 1, "warnings": warnings }));
        }
        OutputMode::Human | OutputMode::Plain => {
            for warning in warnings {
                eprintln!("warning: {}", warning.message);
            }
        }
    }
}

/// Run one command with no workspace sync around it. A workspace command
/// adds its warnings to `warnings`.
fn dispatch(command: Option<Command>, renderer: &Renderer, warnings: &mut Vec<Warning>) -> anyhow::Result<()> {
    // Short ID arguments resolve against one read of the records. `serve`
    // and `web` run many reads and writes, so each of their writes opens its
    // own scope instead.
    let _records = (!matches!(command, Some(Command::Serve { .. } | Command::Web { .. }))).then(RecordScope::enter);
    match command {
        Some(Command::Init { workspace }) => {
            let start = std::env::current_dir().context("could not determine the current directory")?;
            let initialization = initialize(&start, workspace).map_err(ApplicationError::from_init)?;
            write_output(renderer.render_init(&initialization.root, initialization.workspace_enabled)?)
        }
        Some(Command::Capture { command }) => write_output(run_capture(command, renderer)?),
        Some(Command::Release { command }) => exec_release(command, renderer),
        Some(Command::Spec { command }) => exec_spec(command, renderer),
        Some(Command::Plan { command }) => exec_plan(command, renderer),
        Some(Command::Task { command }) => exec_task(command, renderer),
        Some(Command::Dependency { command }) => exec_dependency(command, renderer),
        Some(Command::Gate { command }) => exec_gate(command, renderer),
        Some(Command::Note { command }) => exec_note(command, renderer),
        Some(Command::Skill { command }) => exec_skill(command, renderer),
        Some(Command::Show { id }) => write_output(read_show(&id, renderer)?),
        Some(Command::List { filters }) => write_output(read_list(filters, renderer)?),
        Some(Command::Tree { id, options }) => write_output(read_tree(id.as_deref(), options, renderer)?),
        Some(Command::Explain { task_id }) => write_output(read_explain(&task_id, renderer)?),
        Some(Command::Context { task_id }) => write_output(read_context(&task_id, renderer)?),
        Some(Command::Prime) => write_output(read_prime(renderer)?),
        Some(Command::Check) => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            let report = database.check().map_err(ApplicationError::from_command)?;
            let valid = report.valid;
            let details = report.errors.join("; ");
            write_output(renderer.render_check(&report)?)?;
            if valid {
                Ok(())
            } else {
                Err(ApplicationError::from_command(CommandError::Integrity { message: details }).into())
            }
        }
        Some(Command::Workspace { command }) => {
            let (status, found) = exec_workspace(command).map_err(ApplicationError::from_command)?;
            warnings.extend(Warning::blocker_edges(found));
            match status {
                Some(status) => write_output(renderer.render_workspace_status(&status)?),
                None => write_output(None),
            }
        }
        Some(Command::Ready { filters }) => write_output(read_ready(filters, renderer)?),
        Some(Command::Next { filters, start: true, claim }) => {
            let claim = claim
                .map(claim_name)
                .transpose()
                .map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            let filter = resolve_ready_filter(filters).map_err(ApplicationError::from_command)?;
            let task = database
                .start_next_planning_task(&filter, claim.as_deref())
                .map_err(ApplicationError::from_command)?;
            write_output(renderer.render_started_next_planning_task(task.as_ref())?)
        }
        Some(Command::Next { filters, .. }) => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            let filter = resolve_ready_filter(filters).map_err(ApplicationError::from_command)?;
            let task = database
                .ready_planning_tasks_filtered(&filter)
                .map_err(ApplicationError::from_command)?
                .into_iter()
                .next();
            write_output(renderer.render_next_planning_task(task.as_ref())?)
        }
        Some(Command::Serve { port }) => exec_serve(port, renderer),
        Some(Command::Web { command }) => exec_web(command, renderer),
        None => write_output(renderer.render_startup()?),
    }
}
