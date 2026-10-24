//! Read commands shared by the CLI and `arcl serve`, so both return the same
//! envelopes and errors.

use super::query::{list_connected, resolve_ready_filter, show_record};
use super::support::*;
use super::*;

/// `arcl show <id>`.
pub(super) fn read_show(id: &str, renderer: &Renderer) -> anyhow::Result<Option<String>> {
    let database = open_database().map_err(ApplicationError::from_command)?;
    Ok(show_record(&database, id, renderer).map_err(ApplicationError::from_command)?)
}

/// `arcl list`.
pub(super) fn read_list(filters: ListArgs, renderer: &Renderer) -> anyhow::Result<Option<String>> {
    let database = open_database().map_err(ApplicationError::from_command)?;
    let summaries = list_connected(&database, filters).map_err(ApplicationError::from_command)?;
    Ok(renderer.render_connected_summaries(&summaries)?)
}

/// `arcl explain <task-id>`.
pub(super) fn read_explain(task_id: &str, renderer: &Renderer) -> anyhow::Result<Option<String>> {
    let id = parse_task_id(task_id).map_err(ApplicationError::from_command)?;
    let database = open_database().map_err(ApplicationError::from_command)?;
    let view = database
        .planning_task_view(id)
        .map_err(ApplicationError::from_command)?;
    Ok(renderer.render_planning_task_view(&view)?)
}

/// `arcl context <task-id>`.
pub(super) fn read_context(task_id: &str, renderer: &Renderer) -> anyhow::Result<Option<String>> {
    let id = parse_task_id(task_id).map_err(ApplicationError::from_command)?;
    let database = open_database().map_err(ApplicationError::from_command)?;
    let context = database.planning_context(id).map_err(ApplicationError::from_command)?;
    Ok(renderer.render_planning_context(&context)?)
}

/// `arcl ready`.
pub(super) fn read_ready(filters: ReadyArgs, renderer: &Renderer) -> anyhow::Result<Option<String>> {
    let database = open_database().map_err(ApplicationError::from_command)?;
    let filter = resolve_ready_filter(filters).map_err(ApplicationError::from_command)?;
    let tasks = database
        .ready_planning_tasks_filtered(&filter)
        .map_err(ApplicationError::from_command)?;
    Ok(renderer.render_planning_ready_tasks(&tasks)?)
}

/// Every task with its readiness and effective phase, and every phase, for
/// the web app's Work list.
pub(super) fn read_work(renderer: &Renderer) -> anyhow::Result<Option<String>> {
    let database = open_database().map_err(ApplicationError::from_command)?;
    let work = database.planning_work().map_err(ApplicationError::from_command)?;
    Ok(renderer.render_planning_work(&work)?)
}
