use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::{fs, fs::OpenOptions};

use anyhow::Context;
use thiserror::Error;

use arcl_core::domain::*;
use arcl_repo::snapshot::*;
use arcl_repo::vcs::{GixVcs, Vcs, VcsError};
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
const SNAPSHOT_DIRECTORIES: &[&str] = &["captures", "releases", "specs", "plans", "phases", "tasks", "notes"];

mod commands;
mod error;
mod init;
mod query;
mod snapshot;
mod support;

use commands::{
    CaptureResult, exec_capture, exec_dependency, exec_note, exec_plan, exec_release, exec_spec, exec_task,
};
pub use error::ApplicationError;
use error::{CResult, CommandError};
use init::initialize;
use query::{connected_tree, list_connected, resolve_ready_filter, show_record};
use snapshot::exec_snapshot;
use support::{open_database, parse_task_id, write_output};

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

    match cli.command {
        Some(Command::Init { snapshot }) => {
            let start = std::env::current_dir().context("could not determine the current directory")?;
            let initialization = initialize(&start, snapshot).map_err(ApplicationError::from_init)?;
            write_output(renderer.render_init(&initialization.root, initialization.snapshot_enabled)?)
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
        Some(Command::Release { command }) => exec_release(command, &renderer),
        Some(Command::Spec { command }) => exec_spec(command, &renderer),
        Some(Command::Plan { command }) => exec_plan(command, &renderer),
        Some(Command::Task { command }) => exec_task(command, &renderer),
        Some(Command::Dependency { command }) => exec_dependency(command, &renderer),
        Some(Command::Note { command }) => exec_note(command, &renderer),
        Some(Command::Show { id }) => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            let message = show_record(&database, &id, &renderer).map_err(ApplicationError::from_command)?;
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
        Some(Command::Snapshot { command }) => {
            exec_snapshot(command).map_err(ApplicationError::from_command)?;
            write_output(None)
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
