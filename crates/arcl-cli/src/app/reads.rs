//! Read commands shared by the CLI and `arcl serve`, so both return the same
//! envelopes and errors.

use super::query::{connected_tree, list_connected, resolve_ready_filter, show_record};
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
    let mut summaries = list_connected(&database, filters).map_err(ApplicationError::from_command)?;
    let ids = display_ids(&database, renderer).map_err(ApplicationError::from_command)?;
    for summary in &mut summaries {
        summary.id = ids.short(&summary.id).to_owned();
    }
    Ok(renderer.render_connected_summaries(&summaries)?)
}

/// `arcl explain <task-id>`.
pub(super) fn read_explain(task_id: &str, renderer: &Renderer) -> anyhow::Result<Option<String>> {
    let id = parse_task_id(task_id).map_err(ApplicationError::from_command)?;
    let database = open_database().map_err(ApplicationError::from_command)?;
    let view = database
        .planning_task_view(id)
        .map_err(ApplicationError::from_command)?;
    let ids = display_ids(&database, renderer).map_err(ApplicationError::from_command)?;
    Ok(renderer.render_planning_task_view(&view, &ids)?)
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

/// `arcl tree`, from the record `root` names or of every record, shaped by
/// `options`.
pub(super) fn read_tree(root: Option<&str>, options: TreeArgs, renderer: &Renderer) -> anyhow::Result<Option<String>> {
    let root = root
        .map(|root| resolve_record_id(root, None))
        .transpose()
        .map_err(ApplicationError::from_command)?;
    let database = open_database().map_err(ApplicationError::from_command)?;
    let graph = database.connected_graph().map_err(ApplicationError::from_command)?;
    let ready = database
        .ready_planning_tasks()
        .map_err(ApplicationError::from_command)?
        .into_iter()
        .map(|task| task.id)
        .collect();
    let mut nodes = connected_tree(&graph, &ready, root.as_deref(), options).map_err(ApplicationError::from_command)?;
    let ids = display_ids(&database, renderer).map_err(ApplicationError::from_command)?;
    shorten_tree(&mut nodes, &ids);
    Ok(renderer.render_connected_tree(&nodes, root.is_some())?)
}

/// Replace each node's ID with its short ID.
fn shorten_tree(nodes: &mut [ConnectedTreeNode], ids: &ShortIds) {
    for node in nodes {
        node.id = ids.short(&node.id).to_owned();
        shorten_tree(&mut node.children, ids);
    }
}

/// The workspace file of the record `id`, for the web app's raw editor.
pub(super) fn read_source(id: &str, renderer: &Renderer) -> anyhow::Result<Option<String>> {
    let database = open_database().map_err(ApplicationError::from_command)?;
    let graph = database.connected_graph().map_err(ApplicationError::from_command)?;
    let source = record_source(&graph, id)
        .map_err(ApplicationError::from_command)?
        .ok_or_else(|| ApplicationError::from_command(RecordEditError::NotFound { id: id.to_owned() }))?;
    Ok(renderer.render_record_source(&source)?)
}
