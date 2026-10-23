use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::{fs, fs::OpenOptions};

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
mod query;
mod support;
mod workspace;

use commands::{
    CaptureResult, exec_capture, exec_dependency, exec_note, exec_plan, exec_release, exec_spec, exec_task,
};
pub use error::ApplicationError;
use error::{CResult, CommandError};
use init::initialize;
use query::{connected_tree, list_connected, resolve_ready_filter, show_record};
use support::{open_database, parse_task_id, write_output};
use workspace::exec_workspace;

use auto_sync::{Access, auto_sync, command_access, workspace_roots};

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
    let (roots, synced) = match access {
        Access::Untracked => (None, Ok(())),
        _ => match workspace_roots() {
            Ok(Some(roots)) => {
                let synced = auto_sync(&roots);
                (Some(roots), synced)
            }
            Ok(None) => (None, Ok(())),
            Err(error) => (None, Err(error)),
        },
    };
    match synced {
        // A missing database is the command's own error to report, once.
        Ok(()) | Err(CommandError::NotInitialized { .. }) => {}
        Err(error) if access == Access::Write => {
            let error = match error {
                CommandError::WorkspaceSync(error)
                    if !matches!(*error, WorkspaceSyncError::Storage(StorageError::ChangedSinceRead)) =>
                {
                    CommandError::AutoSync(error)
                }
                other => other,
            };
            return Err(ApplicationError::from_command(error).into());
        }
        Err(error) => {
            eprintln!("warning: the workspace did not sync, so this shows the database as it is: {error}");
        }
    }

    dispatch(cli.command, &renderer)?;

    if access == Access::Write
        && let Some(roots) = &roots
        && let Err(error) = auto_sync(roots)
    {
        eprintln!(
            "warning: the change is saved, but the workspace was not updated: {error}; the next command updates it"
        );
    }
    Ok(())
}

/// Run one command with no workspace sync around it.
fn dispatch(command: Option<Command>, renderer: &Renderer) -> anyhow::Result<()> {
    match command {
        Some(Command::Init { workspace }) => {
            let start = std::env::current_dir().context("could not determine the current directory")?;
            let initialization = initialize(&start, workspace).map_err(ApplicationError::from_init)?;
            write_output(renderer.render_init(&initialization.root, initialization.workspace_enabled)?)
        }
        Some(Command::Capture { command }) => {
            let result = exec_capture(command).map_err(ApplicationError::from_command)?;
            let message = match result {
                CaptureResult::Mutation { action, capture } => renderer.render_capture(action, &capture),
                CaptureResult::Promotion(result) => renderer.render_capture_promotion(&result),
                CaptureResult::List(captures) => renderer.render_captures(&captures),
            }?;
            write_output(message)
        }
        Some(Command::Release { command }) => exec_release(command, renderer),
        Some(Command::Spec { command }) => exec_spec(command, renderer),
        Some(Command::Plan { command }) => exec_plan(command, renderer),
        Some(Command::Task { command }) => exec_task(command, renderer),
        Some(Command::Dependency { command }) => exec_dependency(command, renderer),
        Some(Command::Note { command }) => exec_note(command, renderer),
        Some(Command::Show { id }) => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            let message = show_record(&database, &id, renderer).map_err(ApplicationError::from_command)?;
            write_output(message)
        }
        Some(Command::List { filters }) => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            let summaries = list_connected(&database, filters).map_err(ApplicationError::from_command)?;
            write_output(renderer.render_connected_summaries(&summaries)?)
        }
        Some(Command::Tree { id }) => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            let graph = database.connected_graph().map_err(ApplicationError::from_command)?;
            let nodes = connected_tree(&graph, id.as_deref()).map_err(ApplicationError::from_command)?;
            write_output(renderer.render_connected_tree(&nodes)?)
        }
        Some(Command::Explain { task_id }) => {
            let id = parse_task_id(&task_id).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            let view = database
                .planning_task_view(id)
                .map_err(ApplicationError::from_command)?;
            write_output(renderer.render_planning_task_view(&view)?)
        }
        Some(Command::Context { task_id }) => {
            let id = parse_task_id(&task_id).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            let context = database.planning_context(id).map_err(ApplicationError::from_command)?;
            write_output(renderer.render_planning_context(&context)?)
        }
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
            match exec_workspace(command).map_err(ApplicationError::from_command)? {
                Some(status) => write_output(renderer.render_workspace_status(&status)?),
                None => write_output(None),
            }
        }
        Some(Command::Ready { filters }) => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            let filter = resolve_ready_filter(filters).map_err(ApplicationError::from_command)?;
            let tasks = database
                .ready_planning_tasks_filtered(&filter)
                .map_err(ApplicationError::from_command)?;
            write_output(renderer.render_planning_ready_tasks(&tasks)?)
        }
        Some(Command::Next { filters }) => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            let filter = resolve_ready_filter(filters).map_err(ApplicationError::from_command)?;
            let task = database
                .ready_planning_tasks_filtered(&filter)
                .map_err(ApplicationError::from_command)?
                .into_iter()
                .next();
            write_output(renderer.render_next_planning_task(task.as_ref())?)
        }
        None => write_output(renderer.render_startup()?),
    }
}
