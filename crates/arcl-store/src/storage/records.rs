//! The record list and tree read models that `arcl list`, `arcl tree` and
//! the GraphQL schema share.

use std::collections::HashSet;

use super::{ConnectedGraph, Result, StorageError};
use arcl_core::domain::*;

/// The record kinds human `arcl tree` output groups under a heading at the
/// top level, with each heading, in the order the sections print. They follow
/// the specs and the tasks outside any spec.
pub const TREE_SECTIONS: [(&str, &str); 3] = [("capture", "Inbox"), ("note", "Notes"), ("release", "Releases")];

/// Every record kind a list filter accepts.
pub const RECORD_KINDS: [&str; 7] = ["capture", "release", "spec", "plan", "phase", "task", "note"];

/// The status a tree node's mark is computed from: the record's stored
/// status, with readiness for tasks and started work for containers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TreeStatus {
    /// A task, and whether it is ready to start.
    Task { status: TaskStatus, ready: bool },
    /// A spec, plan or phase, and whether a task under it is in progress or
    /// completed. A release always carries `started: false`.
    Container { status: ContainerStatus, started: bool },
    /// A capture.
    Capture(CaptureStatus),
    /// A note, which has no stored status.
    Note,
}

/// The filters of `arcl list`. Every set filter must match.
#[derive(Clone, Debug, Default)]
pub struct RecordFilter {
    /// One of [`RECORD_KINDS`].
    pub kind: Option<String>,
    /// Stored statuses, such as `pending`; a note's status is `available`.
    pub statuses: Vec<String>,
    /// Task priorities. Only tasks carry one.
    pub priorities: Vec<TaskPriority>,
    /// Records that belong to this release.
    pub release_id: Option<ReleaseId>,
    /// Plans and tasks under this spec, or the spec itself.
    pub spec_id: Option<SpecId>,
    /// Phases and tasks under this plan, or the plan itself.
    pub plan_id: Option<PlanId>,
    /// Tasks under this phase, or the phase itself.
    pub phase_id: Option<PhaseId>,
    /// Direct subtasks of this task.
    pub parent_id: Option<TaskId>,
    /// Tasks claimed by this name.
    pub assignee: Option<String>,
}

impl RecordFilter {
    /// Whether a filter that only tasks carry, a priority or an assignee, is set.
    fn tasks_only(&self) -> bool {
        !self.priorities.is_empty() || self.assignee.is_some()
    }

    /// Refuse an unknown kind or status, a status the kind does not have, and
    /// a task-only filter on another kind, with
    /// [`StorageError::InvalidFilter`].
    fn validate(&self) -> Result<()> {
        let invalid = |message: String| Err(StorageError::InvalidFilter { message });
        if let Some(kind) = &self.kind
            && !RECORD_KINDS.contains(&kind.as_str())
        {
            return invalid(format!(
                "unknown kind `{kind}`; use capture, release, spec, plan, phase, task, or note"
            ));
        }
        if !self.priorities.is_empty() && self.kind.as_deref().is_some_and(|kind| kind != "task") {
            return invalid("priority filters apply only to tasks".to_owned());
        }
        if self.assignee.is_some() && self.kind.as_deref().is_some_and(|kind| kind != "task") {
            return invalid("assignee filters apply only to tasks".to_owned());
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
        if let Some(status) = self
            .statuses
            .iter()
            .find(|status| !known_statuses.contains(&status.as_str()))
        {
            return invalid(format!("unknown status `{status}`"));
        }
        if let Some(kind) = self.kind.as_deref() {
            let allowed = match kind {
                "capture" => &["captured", "promoted", "discarded"][..],
                "release" | "spec" | "plan" | "phase" => &["open", "completed", "cancelled"][..],
                "task" => &["pending", "in_progress", "parked", "completed", "cancelled"][..],
                _ => &[][..],
            };
            if let Some(status) = self.statuses.iter().find(|status| !allowed.contains(&status.as_str())) {
                return invalid(format!("status `{status}` does not apply to {kind} records"));
            }
        }
        Ok(())
    }
}

/// One record in a list: its kind, ID, title and status.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordSummary {
    /// The record kind, one of [`RECORD_KINDS`].
    pub kind: &'static str,
    /// The full record ID.
    pub id: String,
    /// The record title.
    pub title: String,
    /// The stored status; a note's is `available`.
    pub status: String,
}

impl RecordSummary {
    fn new(kind: &'static str, id: String, title: String, status: &str) -> Self {
        Self { kind, id, title, status: status.to_owned() }
    }
}

/// A node of the record tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordTreeNode {
    /// The record kind.
    pub kind: &'static str,
    /// The full record ID.
    pub id: String,
    /// The record title.
    pub title: String,
    /// The stored status; a note's is `available`.
    pub status: String,
    /// The status with readiness or started work, for the node's mark.
    pub tree_status: TreeStatus,
    /// The unfinished tasks that block this task directly, in ID order. The
    /// root task of a rooted tree also lists those it inherits from its
    /// ancestors.
    pub blocked_by: Vec<String>,
    /// Child records in the planning hierarchy.
    pub children: Vec<RecordTreeNode>,
}

/// The records `filter` matches, sorted by kind, then ID. A filter that names
/// a missing record fails with its not-found error.
pub fn list_records(graph: &ConnectedGraph, filter: &RecordFilter) -> Result<Vec<RecordSummary>> {
    filter.validate()?;
    validate_list_targets(graph, filter)?;
    let mut records = Vec::new();
    for capture in &graph.captures {
        if filter.kind.as_deref().is_none_or(|kind| kind == "capture")
            && filter.release_id.is_none()
            && filter.spec_id.is_none()
            && filter.plan_id.is_none()
            && filter.phase_id.is_none()
            && filter.parent_id.is_none()
            && !filter.tasks_only()
            && status_matches(&filter.statuses, capture.status.as_str())
        {
            records.push(RecordSummary::new(
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
            && !filter.tasks_only()
            && status_matches(&filter.statuses, release.status.as_str())
            && filter.release_id.is_none_or(|id| id == release.id)
        {
            records.push(RecordSummary::new(
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
            && !filter.tasks_only()
            && release_member_matches(graph, "spec", &spec.id.to_string(), filter.release_id)
        {
            records.push(RecordSummary::new(
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
            && !filter.tasks_only()
            && release_member_matches(graph, "plan", &plan.id.to_string(), filter.release_id)
        {
            records.push(RecordSummary::new(
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
            && !filter.tasks_only()
        {
            records.push(RecordSummary::new(
                "phase",
                phase.id.to_string(),
                phase.title.clone(),
                phase.status.as_str(),
            ));
        }
    }
    for task in &graph.tasks {
        let ancestry = task_ancestry(graph, task.id)?;
        let task_matches = filter.kind.as_deref().is_none_or(|kind| kind == "task")
            && status_matches(&filter.statuses, task.status.as_str())
            && (filter.priorities.is_empty() || filter.priorities.contains(&task.priority))
            && filter.spec_id.is_none_or(|id| ancestry.0 == Some(id))
            && filter.plan_id.is_none_or(|id| ancestry.1 == Some(id))
            && filter.phase_id.is_none_or(|id| ancestry.2 == Some(id))
            && filter.parent_id.is_none_or(|id| task.parent_id == Some(id))
            && filter
                .assignee
                .as_ref()
                .is_none_or(|name| task.assignee.as_ref() == Some(name))
            && release_member_matches(graph, "task", &task.id.to_string(), filter.release_id);
        if task_matches {
            records.push(RecordSummary::new(
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
            && !filter.tasks_only()
            && filter
                .release_id
                .is_none_or(|id| release_member_matches(graph, "note", &note.id.to_string(), Some(id)))
        {
            records.push(RecordSummary::new(
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

/// Builds the record tree, from the record `root` names, a full ID, or of
/// every record. `ready` holds the ready tasks, for the pending tasks'
/// status. Specs come first, then tasks outside any spec, then captures,
/// notes and releases in [`TREE_SECTIONS`] order; each kind is in ID order.
/// Promoted and discarded captures appear only with `all` or as `root`.
pub fn record_tree(
    graph: &ConnectedGraph, ready: &HashSet<TaskId>, root: Option<&str>, all: bool,
) -> Result<Vec<RecordTreeNode>> {
    let mut nodes = Vec::new();
    for release in &graph.releases {
        nodes.push(RecordTreeNode {
            kind: "release",
            id: release.id.to_string(),
            title: release.title.clone(),
            status: release.status.as_str().to_owned(),
            tree_status: TreeStatus::Container { status: release.status, started: false },
            blocked_by: Vec::new(),
            children: Vec::new(),
        });
    }
    let all_captures = all || root.is_some();
    for capture in graph
        .captures
        .iter()
        .filter(|capture| all_captures || capture.status == CaptureStatus::Captured)
    {
        nodes.push(RecordTreeNode {
            kind: "capture",
            id: capture.id.to_string(),
            title: capture.title.clone(),
            status: capture.status.as_str().to_owned(),
            tree_status: TreeStatus::Capture(capture.status),
            blocked_by: Vec::new(),
            children: Vec::new(),
        });
    }
    for spec in &graph.specs {
        let mut children = graph
            .plans
            .iter()
            .filter(|plan| plan.spec_id == spec.id)
            .map(|plan| plan_tree_node(graph, ready, plan))
            .collect::<Result<Vec<_>>>()?;
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
                .map(|task| task_tree_node(graph, ready, task, &mut HashSet::new()))
                .collect::<Result<Vec<_>>>()?,
        );
        let (children, started) = unzip_started(children);
        nodes.push(RecordTreeNode {
            kind: "spec",
            id: spec.id.to_string(),
            title: spec.title.clone(),
            status: spec.status.as_str().to_owned(),
            tree_status: TreeStatus::Container { status: spec.status, started },
            blocked_by: Vec::new(),
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
            .map(|task| Ok(task_tree_node(graph, ready, task, &mut HashSet::new())?.0))
            .collect::<Result<Vec<_>>>()?,
    );
    for note in &graph.notes {
        nodes.push(RecordTreeNode {
            kind: "note",
            id: note.id.to_string(),
            title: note.title.clone(),
            status: "available".to_owned(),
            tree_status: TreeStatus::Note,
            blocked_by: Vec::new(),
            children: Vec::new(),
        });
    }
    nodes.sort_by(|left, right| (tree_rank(left.kind), &left.id).cmp(&(tree_rank(right.kind), &right.id)));
    let Some(root) = root else { return Ok(nodes) };
    if let Some(node) = find_tree_node(&nodes, root) {
        return Ok(vec![node.clone()]);
    }
    Err(tree_root_error(root))
}

/// The effective spec, plan and phase of task `id`: its own, or those its
/// phase, plan or nearest ancestor task sits under.
pub fn task_ancestry(graph: &ConnectedGraph, id: TaskId) -> Result<(Option<SpecId>, Option<PlanId>, Option<PhaseId>)> {
    task_ancestry_with_seen(graph, id, &mut HashSet::new())
}

/// The place of a top-level record `kind` in [`TREE_SECTIONS`], or `None`
/// for the specs and tasks that print before every section.
pub fn tree_section(kind: &str) -> Option<usize> {
    TREE_SECTIONS.iter().position(|(section, _)| *section == kind)
}

/// Builds a plan's subtree: phases in `position` order, then the plan's
/// phaseless tasks. Tasks under a phase or a parent sort by `position`, then ID.
///
/// Returns the node and whether a task in it is in progress or completed.
fn plan_tree_node(graph: &ConnectedGraph, ready: &HashSet<TaskId>, plan: &Plan) -> Result<(RecordTreeNode, bool)> {
    let mut phases = graph
        .phases
        .iter()
        .filter(|phase| phase.plan_id == plan.id)
        .collect::<Vec<_>>();
    phases.sort_by(|left, right| left.position.cmp(&right.position).then_with(|| left.id.cmp(&right.id)));
    let mut children = phases
        .into_iter()
        .map(|phase| {
            let tasks = positioned_tasks(graph, |task| {
                task.phase_id == Some(phase.id) && task.parent_id.is_none()
            })
            .into_iter()
            .map(|task| task_tree_node(graph, ready, task, &mut HashSet::new()))
            .collect::<Result<Vec<_>>>()?;
            let (tasks, started) = unzip_started(tasks);
            let node = RecordTreeNode {
                kind: "phase",
                id: phase.id.to_string(),
                title: phase.title.clone(),
                status: phase.status.as_str().to_owned(),
                tree_status: TreeStatus::Container { status: phase.status, started },
                blocked_by: Vec::new(),
                children: tasks,
            };
            Ok((node, started))
        })
        .collect::<Result<Vec<_>>>()?;
    children.extend(
        positioned_tasks(graph, |task| {
            task.plan_id == Some(plan.id) && task.phase_id.is_none() && task.parent_id.is_none()
        })
        .into_iter()
        .map(|task| task_tree_node(graph, ready, task, &mut HashSet::new()))
        .collect::<Result<Vec<_>>>()?,
    );
    // Stable: keeps the position order within each kind.
    children.sort_by(|left, right| left.0.kind.cmp(right.0.kind));
    let (children, started) = unzip_started(children);
    let node = RecordTreeNode {
        kind: "plan",
        id: plan.id.to_string(),
        title: plan.title.clone(),
        status: plan.status.as_str().to_owned(),
        tree_status: TreeStatus::Container { status: plan.status, started },
        blocked_by: Vec::new(),
        children,
    };
    Ok((node, started))
}

/// Builds a task's subtree. Returns the node and whether the task or a subtask
/// is in progress or completed, which marks the containers above it.
fn task_tree_node(
    graph: &ConnectedGraph, ready: &HashSet<TaskId>, task: &PlanningTask, path: &mut HashSet<TaskId>,
) -> Result<(RecordTreeNode, bool)> {
    if !path.insert(task.id) {
        return Err(StorageError::InvalidPlanningTask(DomainError::ParentCycle {
            task: task.id.to_string(),
            parent: task.id.to_string(),
        }));
    }
    let children = positioned_tasks(graph, |child| child.parent_id == Some(task.id))
        .into_iter()
        .map(|child| task_tree_node(graph, ready, child, path))
        .collect::<Result<Vec<_>>>()?;
    path.remove(&task.id);
    let (children, child_started) = unzip_started(children);
    let blocked_by = open_blockers(graph, task);
    let started = child_started || matches!(task.status, TaskStatus::InProgress | TaskStatus::Completed);
    let node = RecordTreeNode {
        kind: "task",
        id: task.id.to_string(),
        title: task.title.clone(),
        status: task.status.as_str().to_owned(),
        tree_status: TreeStatus::Task { status: task.status, ready: ready.contains(&task.id) },
        blocked_by,
        children,
    };
    Ok((node, started))
}

/// The IDs of the unfinished tasks that block `task` directly, in ID order,
/// or none when `task` is completed or cancelled.
fn open_blockers(graph: &ConnectedGraph, task: &PlanningTask) -> Vec<String> {
    if matches!(task.status, TaskStatus::Completed | TaskStatus::Cancelled) {
        return Vec::new();
    }
    let mut blockers = graph
        .dependencies
        .iter()
        .filter(|dependency| dependency.task_id == task.id)
        .filter(|dependency| {
            graph
                .tasks
                .iter()
                .find(|blocker| blocker.id == dependency.blocker_id)
                .is_none_or(|blocker| blocker.status != TaskStatus::Completed)
        })
        .map(|dependency| dependency.blocker_id)
        .collect::<Vec<_>>();
    blockers.sort();
    blockers.into_iter().map(|id| id.to_string()).collect()
}

/// Splits built subtrees into their nodes and whether any of them holds a
/// started task. An open container over a started task shows as in progress.
fn unzip_started(subtrees: Vec<(RecordTreeNode, bool)>) -> (Vec<RecordTreeNode>, bool) {
    let started = subtrees.iter().any(|(_, started)| *started);
    (subtrees.into_iter().map(|(node, _)| node).collect(), started)
}

/// Returns the tasks that match `keep`, sorted by `position`, then ID.
///
/// The store already loads tasks in that order; sorting here keeps the tree
/// correct for any graph built another way.
fn positioned_tasks(graph: &ConnectedGraph, keep: impl Fn(&PlanningTask) -> bool) -> Vec<&PlanningTask> {
    let mut tasks = graph.tasks.iter().filter(|task| keep(task)).collect::<Vec<_>>();
    tasks.sort_by(|left, right| left.position.cmp(&right.position).then_with(|| left.id.cmp(&right.id)));
    tasks
}

fn find_tree_node<'a>(nodes: &'a [RecordTreeNode], id: &str) -> Option<&'a RecordTreeNode> {
    nodes.iter().find_map(
        |node| {
            if node.id == id { Some(node) } else { find_tree_node(&node.children, id) }
        },
    )
}

/// The sort rank of a top-level tree node `kind`: specs, then tasks, then
/// the kinds in [`TREE_SECTIONS`] order.
fn tree_rank(kind: &str) -> usize {
    match kind {
        "spec" => 0,
        "task" => 1,
        _ => 2 + tree_section(kind).unwrap_or(TREE_SECTIONS.len()),
    }
}

fn tree_root_error(id: &str) -> StorageError {
    let id = id.to_owned();
    if id.starts_with(CaptureId::PREFIX) {
        return StorageError::CaptureNotFound { id };
    }
    if id.starts_with(SpecId::PREFIX) {
        return StorageError::SpecNotFound { id };
    }
    if id.starts_with(PlanId::PREFIX) {
        return StorageError::PlanNotFound { id };
    }
    if id.starts_with(PhaseId::PREFIX) {
        return StorageError::PhaseNotFound { id };
    }
    if id.starts_with(TaskId::PREFIX) {
        return StorageError::PlanningTaskNotFound { id };
    }
    if id.starts_with(NoteId::PREFIX) {
        return StorageError::NoteNotFound { id };
    }
    if id.starts_with(ReleaseId::PREFIX) {
        return StorageError::ReleaseNotFound { id };
    }
    StorageError::InvalidFilter { message: format!("unknown record ID `{id}`") }
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
    graph: &ConnectedGraph, id: TaskId, seen: &mut HashSet<TaskId>,
) -> Result<(Option<SpecId>, Option<PlanId>, Option<PhaseId>)> {
    if !seen.insert(id) {
        return Err(StorageError::InvalidPlanningTask(DomainError::ParentCycle {
            task: id.to_string(),
            parent: id.to_string(),
        }));
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

fn validate_list_targets(graph: &ConnectedGraph, filter: &RecordFilter) -> Result<()> {
    if let Some(id) = filter.release_id
        && !graph.releases.iter().any(|release| release.id == id)
    {
        return Err(StorageError::ReleaseNotFound { id: id.to_string() });
    }
    if let Some(id) = filter.spec_id
        && !graph.specs.iter().any(|spec| spec.id == id)
    {
        return Err(StorageError::SpecNotFound { id: id.to_string() });
    }
    if let Some(id) = filter.plan_id
        && !graph.plans.iter().any(|plan| plan.id == id)
    {
        return Err(StorageError::PlanNotFound { id: id.to_string() });
    }
    if let Some(id) = filter.phase_id
        && !graph.phases.iter().any(|phase| phase.id == id)
    {
        return Err(StorageError::PhaseNotFound { id: id.to_string() });
    }
    if let Some(id) = filter.parent_id
        && !graph.tasks.iter().any(|task| task.id == id)
    {
        return Err(StorageError::PlanningTaskNotFound { id: id.to_string() });
    }
    Ok(())
}

fn status_matches(statuses: &[String], status: &str) -> bool {
    statuses.is_empty() || statuses.iter().any(|candidate| candidate == status)
}
