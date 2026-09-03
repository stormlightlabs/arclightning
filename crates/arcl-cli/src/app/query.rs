use super::support::*;
use super::*;

struct ListFilter {
    kind: Option<String>,
    statuses: Vec<String>,
    priorities: Vec<TaskPriority>,
    release_id: Option<ReleaseId>,
    spec_id: Option<SpecId>,
    plan_id: Option<PlanId>,
    phase_id: Option<PhaseId>,
    parent_id: Option<TaskId>,
}

impl ListFilter {
    fn resolve_list_filter(args: ListArgs) -> CResult<Self> {
        let known = ["capture", "release", "spec", "plan", "phase", "task", "note"];
        if let Some(kind) = &args.kind
            && !known.contains(&kind.as_str())
        {
            return Err(CommandError::InvalidFilter {
                message: format!("unknown kind `{kind}`; use capture, release, spec, plan, phase, task, or note"),
            });
        }
        let priorities = args
            .priority
            .iter()
            .map(|value| TaskPriority::parse(value))
            .collect::<Result<Vec<_>, _>>()?;
        if !priorities.is_empty() && args.kind.as_deref().is_some_and(|kind| kind != "task") {
            return Err(CommandError::InvalidFilter { message: "priority filters apply only to tasks".to_owned() });
        }
        let known_statuses = [
            "captured",
            "promoted",
            "discarded",
            "open",
            "completed",
            "cancelled",
            "pending",
            "in_progress",
            "parked",
        ];
        if let Some(status) = args
            .status
            .iter()
            .find(|status| !known_statuses.contains(&status.as_str()))
        {
            return Err(CommandError::InvalidFilter { message: format!("unknown status `{status}`") });
        }
        if let Some(kind) = args.kind.as_deref() {
            let allowed = match kind {
                "capture" => &["captured", "promoted", "discarded"][..],
                "release" | "spec" | "plan" | "phase" => &["open", "completed", "cancelled"][..],
                "task" => &["pending", "in_progress", "parked", "completed", "cancelled"][..],
                "note" => &[][..],
                _ => &[][..],
            };
            if let Some(status) = args.status.iter().find(|status| !allowed.contains(&status.as_str())) {
                return Err(CommandError::InvalidFilter {
                    message: format!("status `{status}` does not apply to {kind} records"),
                });
            }
        }
        let release_id = parse_optional_id(args.release, ReleaseId::parse)?;
        let spec_id = parse_optional_id(args.spec, SpecId::parse)?;
        let plan_id = parse_optional_id(args.plan, PlanId::parse)?;
        let phase_id = parse_optional_id(args.phase, PhaseId::parse)?;
        let parent_id = parse_optional_id(args.parent, TaskId::parse)?;
        Ok(ListFilter {
            kind: args.kind,
            statuses: args.status,
            priorities,
            release_id,
            parent_id,
            spec_id,
            plan_id,
            phase_id,
        })
    }
}

pub(super) fn show_record(database: &Database, id: &str, renderer: &Renderer) -> CResult<Option<String>> {
    if id.starts_with(CaptureId::PREFIX) {
        let value = parse_capture_id(id)?;
        let record = database
            .capture(value)?
            .ok_or_else(|| StorageError::CaptureNotFound { id: id.to_owned() })?;
        return renderer
            .render_capture("shown", &record)
            .map_err(|error| CommandError::InvalidFilter { message: error.to_string() });
    }
    if id.starts_with(SpecId::PREFIX) {
        let value = parse_spec_id(id)?;
        let record = database
            .spec(value)?
            .ok_or_else(|| StorageError::SpecNotFound { id: id.to_owned() })?;
        return renderer
            .render_spec("shown", &record)
            .map_err(|error| CommandError::InvalidFilter { message: error.to_string() });
    }
    if id.starts_with(PlanId::PREFIX) {
        let value = parse_plan_id(id)?;
        let record = database
            .plan(value)?
            .ok_or_else(|| StorageError::PlanNotFound { id: id.to_owned() })?;
        return renderer
            .render_plan("shown", &record)
            .map_err(|error| CommandError::InvalidFilter { message: error.to_string() });
    }
    if id.starts_with(PhaseId::PREFIX) {
        let value = parse_phase_id(id)?;
        let record = database
            .phase(value)?
            .ok_or_else(|| StorageError::PhaseNotFound { id: id.to_owned() })?;
        return renderer
            .render_phase("shown", &record)
            .map_err(|error| CommandError::InvalidFilter { message: error.to_string() });
    }
    if id.starts_with(TaskId::PREFIX) {
        let value = parse_task_id(id)?;
        let record = database.planning_task_view(value)?;
        return renderer
            .render_planning_task_view(&record)
            .map_err(|error| CommandError::InvalidFilter { message: error.to_string() });
    }
    if id.starts_with(NoteId::PREFIX) {
        let value = parse_note_id(id)?;
        let record = database
            .note(value)?
            .ok_or_else(|| StorageError::NoteNotFound { id: id.to_owned() })?;
        return renderer
            .render_note("shown", &record)
            .map_err(|error| CommandError::InvalidFilter { message: error.to_string() });
    }
    if id.starts_with(ReleaseId::PREFIX) {
        let value = parse_release_id(id)?;
        let record = database
            .release(value)?
            .ok_or_else(|| StorageError::ReleaseNotFound { id: id.to_owned() })?;
        return renderer
            .render_connected_release("shown", &record)
            .map_err(|error| CommandError::InvalidFilter { message: error.to_string() });
    }
    Err(CommandError::InvalidFilter { message: format!("unknown record ID `{id}`") })
}

pub(super) fn connected_tree(graph: &ConnectedGraph, root: Option<&str>) -> CResult<Vec<ConnectedTreeNode>> {
    let mut nodes = Vec::new();
    for release in &graph.releases {
        nodes.push(ConnectedTreeNode {
            kind: "release",
            id: release.id.to_string(),
            title: release.title.clone(),
            status: release.status.as_str().to_owned(),
            children: Vec::new(),
        });
    }
    for capture in &graph.captures {
        nodes.push(ConnectedTreeNode {
            kind: "capture",
            id: capture.id.to_string(),
            title: capture.title.clone(),
            status: capture.status.as_str().to_owned(),
            children: Vec::new(),
        });
    }
    for spec in &graph.specs {
        let mut children = graph
            .plans
            .iter()
            .filter(|plan| plan.spec_id == spec.id)
            .map(|plan| plan_tree_node(graph, plan))
            .collect::<CResult<Vec<_>>>()?;
        children.extend(
            graph
                .tasks
                .iter()
                .filter(|task| {
                    task.spec_id == Some(spec.id)
                        && task.plan_id.is_none()
                        && task.phase_id.is_none()
                        && task.parent_id.is_none()
                })
                .map(|task| task_tree_node(graph, task, &mut std::collections::HashSet::new()))
                .collect::<CResult<Vec<_>>>()?,
        );
        nodes.push(ConnectedTreeNode {
            kind: "spec",
            id: spec.id.to_string(),
            title: spec.title.clone(),
            status: spec.status.as_str().to_owned(),
            children,
        });
    }
    nodes.extend(
        graph
            .tasks
            .iter()
            .filter(|task| {
                task.spec_id.is_none() && task.plan_id.is_none() && task.phase_id.is_none() && task.parent_id.is_none()
            })
            .map(|task| task_tree_node(graph, task, &mut std::collections::HashSet::new()))
            .collect::<CResult<Vec<_>>>()?,
    );
    for note in &graph.notes {
        nodes.push(ConnectedTreeNode {
            kind: "note",
            id: note.id.to_string(),
            title: note.title.clone(),
            status: "available".to_owned(),
            children: Vec::new(),
        });
    }
    nodes.sort_by(|left, right| left.kind.cmp(right.kind).then_with(|| left.id.cmp(&right.id)));
    let Some(root) = root else { return Ok(nodes) };
    if let Some(node) = find_tree_node(&nodes, root) {
        return Ok(vec![node.clone()]);
    }
    Err(tree_root_error(root))
}

pub(super) fn list_connected(database: &Database, args: ListArgs) -> CResult<Vec<ConnectedSummary>> {
    let graph = database.connected_graph()?;
    let filter = ListFilter::resolve_list_filter(args)?;
    validate_list_targets(&graph, &filter)?;
    let mut records = Vec::new();
    for capture in &graph.captures {
        if filter.kind.as_deref().is_none_or(|kind| kind == "capture")
            && filter.release_id.is_none()
            && filter.spec_id.is_none()
            && filter.plan_id.is_none()
            && filter.phase_id.is_none()
            && filter.parent_id.is_none()
            && filter.priorities.is_empty()
            && status_matches(&filter.statuses, capture.status.as_str())
        {
            records.push(ConnectedSummary::new(
                "capture",
                capture.id.to_string(),
                capture.title.clone(),
                capture.status.as_str(),
            ));
        }
    }
    for release in &graph.releases {
        if filter.kind.as_deref().is_none_or(|kind| kind == "release")
            && filter.spec_id.is_none()
            && filter.plan_id.is_none()
            && filter.phase_id.is_none()
            && filter.parent_id.is_none()
            && filter.priorities.is_empty()
            && status_matches(&filter.statuses, release.status.as_str())
            && filter.release_id.is_none_or(|id| id == release.id)
        {
            records.push(ConnectedSummary::new(
                "release",
                release.id.to_string(),
                release.title.clone(),
                release.status.as_str(),
            ));
        }
    }
    for spec in &graph.specs {
        if filter.kind.as_deref().is_none_or(|kind| kind == "spec")
            && status_matches(&filter.statuses, spec.status.as_str())
            && filter.spec_id.is_none_or(|id| id == spec.id)
            && filter.plan_id.is_none()
            && filter.phase_id.is_none()
            && filter.parent_id.is_none()
            && filter.priorities.is_empty()
            && release_member_matches(&graph, "spec", &spec.id.to_string(), filter.release_id)
        {
            records.push(ConnectedSummary::new(
                "spec",
                spec.id.to_string(),
                spec.title.clone(),
                spec.status.as_str(),
            ));
        }
    }
    for plan in &graph.plans {
        if filter.kind.as_deref().is_none_or(|kind| kind == "plan")
            && status_matches(&filter.statuses, plan.status.as_str())
            && filter.plan_id.is_none_or(|id| id == plan.id)
            && filter.spec_id.is_none_or(|id| id == plan.spec_id)
            && filter.phase_id.is_none()
            && filter.parent_id.is_none()
            && filter.priorities.is_empty()
            && release_member_matches(&graph, "plan", &plan.id.to_string(), filter.release_id)
        {
            records.push(ConnectedSummary::new(
                "plan",
                plan.id.to_string(),
                plan.title.clone(),
                plan.status.as_str(),
            ));
        }
    }
    for phase in &graph.phases {
        if filter.kind.as_deref().is_none_or(|kind| kind == "phase")
            && status_matches(&filter.statuses, phase.status.as_str())
            && filter.phase_id.is_none_or(|id| id == phase.id)
            && filter.plan_id.is_none_or(|id| id == phase.plan_id)
            && filter.release_id.is_none()
            && filter.spec_id.is_none()
            && filter.parent_id.is_none()
            && filter.priorities.is_empty()
        {
            records.push(ConnectedSummary::new(
                "phase",
                phase.id.to_string(),
                phase.title.clone(),
                phase.status.as_str(),
            ));
        }
    }
    for task in &graph.tasks {
        let ancestry = task_ancestry(&graph, task.id)?;
        let task_matches = filter.kind.as_deref().is_none_or(|kind| kind == "task")
            && status_matches(&filter.statuses, task.status.as_str())
            && (filter.priorities.is_empty() || filter.priorities.contains(&task.priority))
            && filter.spec_id.is_none_or(|id| ancestry.0 == Some(id))
            && filter.plan_id.is_none_or(|id| ancestry.1 == Some(id))
            && filter.phase_id.is_none_or(|id| ancestry.2 == Some(id))
            && filter.parent_id.is_none_or(|id| task.parent_id == Some(id))
            && release_member_matches(&graph, "task", &task.id.to_string(), filter.release_id);
        if task_matches {
            records.push(ConnectedSummary::new(
                "task",
                task.id.to_string(),
                task.title.clone(),
                task.status.as_str(),
            ));
        }
    }
    for note in &graph.notes {
        if filter.kind.as_deref().is_none_or(|kind| kind == "note")
            && filter.spec_id.is_none()
            && filter.plan_id.is_none()
            && filter.phase_id.is_none()
            && filter.parent_id.is_none()
            && filter.priorities.is_empty()
            && filter
                .release_id
                .is_none_or(|id| release_member_matches(&graph, "note", &note.id.to_string(), Some(id)))
        {
            records.push(ConnectedSummary::new(
                "note",
                note.id.to_string(),
                note.title.clone(),
                "available",
            ));
        }
    }
    records.sort_by(|left, right| left.kind.cmp(right.kind).then_with(|| left.id.cmp(&right.id)));
    Ok(records)
}

pub(super) fn task_ancestry(
    graph: &ConnectedGraph, id: TaskId,
) -> CResult<(Option<SpecId>, Option<PlanId>, Option<PhaseId>)> {
    task_ancestry_with_seen(graph, id, &mut std::collections::HashSet::new())
}

pub(super) fn resolve_ready_filter(args: ReadyArgs) -> CResult<PlanningReadyFilter> {
    Ok(PlanningReadyFilter {
        priorities: args
            .priority
            .iter()
            .map(|value| TaskPriority::parse(value))
            .collect::<Result<Vec<_>, _>>()?,
        spec_id: parse_optional_id(args.spec, SpecId::parse)?,
        plan_id: parse_optional_id(args.plan, PlanId::parse)?,
        phase_id: parse_optional_id(args.phase, PhaseId::parse)?,
        parent_id: parse_optional_id(args.parent, TaskId::parse)?,
    })
}

fn plan_tree_node(graph: &ConnectedGraph, plan: &Plan) -> CResult<ConnectedTreeNode> {
    let mut children = graph
        .phases
        .iter()
        .filter(|phase| phase.plan_id == plan.id)
        .map(|phase| {
            let tasks = graph
                .tasks
                .iter()
                .filter(|task| task.phase_id == Some(phase.id) && task.parent_id.is_none())
                .map(|task| task_tree_node(graph, task, &mut std::collections::HashSet::new()))
                .collect::<CResult<Vec<_>>>()?;
            Ok(ConnectedTreeNode {
                kind: "phase",
                id: phase.id.to_string(),
                title: phase.title.clone(),
                status: phase.status.as_str().to_owned(),
                children: tasks,
            })
        })
        .collect::<CResult<Vec<_>>>()?;
    children.extend(
        graph
            .tasks
            .iter()
            .filter(|task| task.plan_id == Some(plan.id) && task.phase_id.is_none() && task.parent_id.is_none())
            .map(|task| task_tree_node(graph, task, &mut std::collections::HashSet::new()))
            .collect::<CResult<Vec<_>>>()?,
    );
    children.sort_by(|left, right| left.kind.cmp(right.kind).then_with(|| left.id.cmp(&right.id)));
    Ok(ConnectedTreeNode {
        kind: "plan",
        id: plan.id.to_string(),
        title: plan.title.clone(),
        status: plan.status.as_str().to_owned(),
        children,
    })
}

fn task_tree_node(
    graph: &ConnectedGraph, task: &PlanningTask, path: &mut std::collections::HashSet<TaskId>,
) -> CResult<ConnectedTreeNode> {
    if !path.insert(task.id) {
        return Err(CommandError::Storage(StorageError::InvalidPlanningTask(
            DomainError::ParentCycle { task: task.id.to_string(), parent: task.id.to_string() },
        )));
    }
    let mut children = graph
        .tasks
        .iter()
        .filter(|child| child.parent_id == Some(task.id))
        .map(|child| task_tree_node(graph, child, path))
        .collect::<CResult<Vec<_>>>()?;
    path.remove(&task.id);
    children.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(ConnectedTreeNode {
        kind: "task",
        id: task.id.to_string(),
        title: task.title.clone(),
        status: task.status.as_str().to_owned(),
        children,
    })
}

fn find_tree_node<'a>(nodes: &'a [ConnectedTreeNode], id: &str) -> Option<&'a ConnectedTreeNode> {
    nodes.iter().find_map(
        |node| {
            if node.id == id { Some(node) } else { find_tree_node(&node.children, id) }
        },
    )
}

fn tree_root_error(id: &str) -> CommandError {
    if id.starts_with(CaptureId::PREFIX) {
        return CommandError::Storage(StorageError::CaptureNotFound { id: id.to_owned() });
    }
    if id.starts_with(SpecId::PREFIX) {
        return CommandError::Storage(StorageError::SpecNotFound { id: id.to_owned() });
    }
    if id.starts_with(PlanId::PREFIX) {
        return CommandError::Storage(StorageError::PlanNotFound { id: id.to_owned() });
    }
    if id.starts_with(PhaseId::PREFIX) {
        return CommandError::Storage(StorageError::PhaseNotFound { id: id.to_owned() });
    }
    if id.starts_with(TaskId::PREFIX) {
        return CommandError::Storage(StorageError::PlanningTaskNotFound { id: id.to_owned() });
    }
    if id.starts_with(NoteId::PREFIX) {
        return CommandError::Storage(StorageError::NoteNotFound { id: id.to_owned() });
    }
    if id.starts_with(ReleaseId::PREFIX) {
        return CommandError::Storage(StorageError::ReleaseNotFound { id: id.to_owned() });
    }
    CommandError::InvalidFilter { message: format!("unknown record ID `{id}`") }
}

fn release_member_matches(graph: &ConnectedGraph, kind: &str, id: &str, release: Option<ReleaseId>) -> bool {
    release.is_none_or(|release| {
        graph
            .release_memberships
            .iter()
            .any(|member| member.release_id == release && member.record_kind.as_str() == kind && member.record_id == id)
    })
}

fn task_ancestry_with_seen(
    graph: &ConnectedGraph, id: TaskId, seen: &mut std::collections::HashSet<TaskId>,
) -> CResult<(Option<SpecId>, Option<PlanId>, Option<PhaseId>)> {
    if !seen.insert(id) {
        return Err(CommandError::Storage(StorageError::InvalidPlanningTask(
            DomainError::ParentCycle { task: id.to_string(), parent: id.to_string() },
        )));
    }
    let task = graph
        .tasks
        .iter()
        .find(|task| task.id == id)
        .ok_or_else(|| StorageError::PlanningTaskNotFound { id: id.to_string() })?;
    let mut ancestry = (task.spec_id, task.plan_id, task.phase_id);
    if let Some(phase_id) = ancestry.2 {
        let phase = graph
            .phases
            .iter()
            .find(|phase| phase.id == phase_id)
            .ok_or_else(|| StorageError::PhaseNotFound { id: phase_id.to_string() })?;
        ancestry.1.get_or_insert(phase.plan_id);
    }
    if let Some(plan_id) = ancestry.1 {
        let plan = graph
            .plans
            .iter()
            .find(|plan| plan.id == plan_id)
            .ok_or_else(|| StorageError::PlanNotFound { id: plan_id.to_string() })?;
        ancestry.0.get_or_insert(plan.spec_id);
    }
    if let Some(parent_id) = task.parent_id {
        let parent = task_ancestry_with_seen(graph, parent_id, seen)?;
        if ancestry.0.is_none() {
            ancestry.0 = parent.0;
        }
        if ancestry.1.is_none() {
            ancestry.1 = parent.1;
        }
        if ancestry.2.is_none() {
            ancestry.2 = parent.2;
        }
    }
    seen.remove(&id);
    Ok(ancestry)
}

fn validate_list_targets(graph: &ConnectedGraph, filter: &ListFilter) -> CResult<()> {
    if let Some(id) = filter.release_id
        && !graph.releases.iter().any(|release| release.id == id)
    {
        return Err(CommandError::Storage(StorageError::ReleaseNotFound {
            id: id.to_string(),
        }));
    }
    if let Some(id) = filter.spec_id
        && !graph.specs.iter().any(|spec| spec.id == id)
    {
        return Err(CommandError::Storage(StorageError::SpecNotFound { id: id.to_string() }));
    }
    if let Some(id) = filter.plan_id
        && !graph.plans.iter().any(|plan| plan.id == id)
    {
        return Err(CommandError::Storage(StorageError::PlanNotFound { id: id.to_string() }));
    }
    if let Some(id) = filter.phase_id
        && !graph.phases.iter().any(|phase| phase.id == id)
    {
        return Err(CommandError::Storage(StorageError::PhaseNotFound {
            id: id.to_string(),
        }));
    }
    if let Some(id) = filter.parent_id
        && !graph.tasks.iter().any(|task| task.id == id)
    {
        return Err(CommandError::Storage(StorageError::PlanningTaskNotFound {
            id: id.to_string(),
        }));
    }
    Ok(())
}

fn status_matches(statuses: &[String], status: &str) -> bool {
    statuses.is_empty() || statuses.iter().any(|candidate| candidate == status)
}
