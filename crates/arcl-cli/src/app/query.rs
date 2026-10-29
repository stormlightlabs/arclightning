use super::support::*;
use super::*;

/// Show the record `id` names, a full or short ID. Human output shows short
/// IDs.
pub(super) fn show_record(database: &Database, id: &str, renderer: &Renderer) -> CResult<Option<String>> {
    let id = &database.resolve_record_id(id, None)?;
    let ids = display_ids(database, renderer)?;
    if id.starts_with(CaptureId::PREFIX) {
        let value = parse_capture_id(id)?;
        let record = database
            .capture(value)?
            .ok_or_else(|| StorageError::CaptureNotFound { id: id.to_owned() })?;
        let source = record
            .source_task_id
            .map(|task| SourceTask { id: task, ids: &ids, beside_record: false });
        return renderer
            .render_shown_from("capture", ids.short(id), &record.title, &record, source)
            .map_err(|error| CommandError::InvalidFilter { message: error.to_string() });
    }
    if id.starts_with(SpecId::PREFIX) {
        let value = parse_spec_id(id)?;
        let record = database
            .spec(value)?
            .ok_or_else(|| StorageError::SpecNotFound { id: id.to_owned() })?;
        let source =
            database
                .promoted_source_task(id)?
                .map(|task| SourceTask { id: task, ids: &ids, beside_record: true });
        return renderer
            .render_shown_from("spec", ids.short(id), &record.title, &record, source)
            .map_err(|error| CommandError::InvalidFilter { message: error.to_string() });
    }
    if id.starts_with(PlanId::PREFIX) {
        let value = parse_plan_id(id)?;
        let record = database
            .plan(value)?
            .ok_or_else(|| StorageError::PlanNotFound { id: id.to_owned() })?;
        return renderer
            .render_shown("plan", ids.short(id), &record.title, &record)
            .map_err(|error| CommandError::InvalidFilter { message: error.to_string() });
    }
    if id.starts_with(PhaseId::PREFIX) {
        let value = parse_phase_id(id)?;
        let record = database
            .phase(value)?
            .ok_or_else(|| StorageError::PhaseNotFound { id: id.to_owned() })?;
        return renderer
            .render_shown("phase", ids.short(id), &record.title, &record)
            .map_err(|error| CommandError::InvalidFilter { message: error.to_string() });
    }
    if id.starts_with(TaskId::PREFIX) {
        let value = parse_task_id(id)?;
        let record = database.planning_task_view(value)?;
        return renderer
            .render_planning_task_view(&record, &ids)
            .map_err(|error| CommandError::InvalidFilter { message: error.to_string() });
    }
    if id.starts_with(NoteId::PREFIX) {
        let value = parse_note_id(id)?;
        let record = database
            .note(value)?
            .ok_or_else(|| StorageError::NoteNotFound { id: id.to_owned() })?;
        let source =
            database
                .promoted_source_task(id)?
                .map(|task| SourceTask { id: task, ids: &ids, beside_record: true });
        return renderer
            .render_shown_from("note", ids.short(id), &record.title, &record, source)
            .map_err(|error| CommandError::InvalidFilter { message: error.to_string() });
    }
    if id.starts_with(ReleaseId::PREFIX) {
        let value = parse_release_id(id)?;
        let record = database
            .release(value)?
            .ok_or_else(|| StorageError::ReleaseNotFound { id: id.to_owned() })?;
        return renderer
            .render_shown("release", ids.short(id), &record.title, &record)
            .map_err(|error| CommandError::InvalidFilter { message: error.to_string() });
    }
    if id.starts_with(GateId::PREFIX) {
        let record = database.gate(parse_id::<GateId>(id)?)?;
        return renderer
            .render_shown_gate(&record, &ids)
            .map_err(|error| CommandError::InvalidFilter { message: error.to_string() });
    }
    Err(CommandError::InvalidFilter { message: format!("unknown record ID `{id}`") })
}

/// `arcl list`: the records `args` filters, with full IDs.
///
/// Filters are checked in a fixed order: the kind, then priorities, then
/// what the kind allows and the statuses, then the record IDs.
pub(super) fn list_connected(database: &Database, args: ListArgs) -> CResult<Vec<ConnectedSummary>> {
    RecordFilter { kind: args.kind.clone(), ..RecordFilter::default() }
        .validate()
        .map_err(filter_error)?;
    let mut filter = RecordFilter {
        priorities: args
            .priority
            .iter()
            .map(|value| TaskPriority::parse(value))
            .collect::<Result<Vec<_>, _>>()?,
        assignee: args.assignee.map(|name| name.trim().to_owned()),
        kind: args.kind,
        statuses: args.status,
        ..RecordFilter::default()
    };
    filter.validate().map_err(filter_error)?;
    filter.release_id = parse_optional_id::<ReleaseId>(args.release)?;
    filter.spec_id = parse_optional_id::<SpecId>(args.spec)?;
    filter.plan_id = parse_optional_id::<PlanId>(args.plan)?;
    filter.phase_id = parse_optional_id::<PhaseId>(args.phase)?;
    filter.parent_id = parse_optional_id::<TaskId>(args.parent)?;
    let records = database.list_records(&filter).map_err(filter_error)?;
    Ok(records
        .into_iter()
        .map(|record| ConnectedSummary::new(record.kind, record.id, record.title, &record.status))
        .collect())
}

/// `arcl tree`: the record tree from the record `root` names, a full ID, or
/// of every record.
pub(super) fn connected_tree(
    database: &Database, root: Option<&str>, options: TreeArgs,
) -> CResult<Vec<ConnectedTreeNode>> {
    let nodes = database.record_tree(root, options.all).map_err(filter_error)?;
    Ok(nodes.into_iter().map(tree_node).collect())
}

pub(super) fn resolve_ready_filter(args: ReadyArgs) -> CResult<PlanningReadyFilter> {
    Ok(PlanningReadyFilter {
        priorities: args
            .priority
            .iter()
            .map(|value| TaskPriority::parse(value))
            .collect::<Result<Vec<_>, _>>()?,
        spec_id: parse_optional_id::<SpecId>(args.spec)?,
        plan_id: parse_optional_id::<PlanId>(args.plan)?,
        phase_id: parse_optional_id::<PhaseId>(args.phase)?,
        parent_id: parse_optional_id::<TaskId>(args.parent)?,
    })
}

/// Report a refused filter as `arcl list` always has, and any other storage
/// failure as it is.
fn filter_error(error: StorageError) -> CommandError {
    match error {
        StorageError::InvalidFilter { message } => CommandError::InvalidFilter { message },
        error => CommandError::Storage(error),
    }
}

fn tree_node(node: RecordTreeNode) -> ConnectedTreeNode {
    ConnectedTreeNode {
        kind: node.kind,
        id: node.id,
        title: node.title,
        status: node.status,
        mark: StatusMark::of(node.tree_status),
        blocked_by: node.blocked_by,
        children: node.children.into_iter().map(tree_node).collect(),
    }
}
