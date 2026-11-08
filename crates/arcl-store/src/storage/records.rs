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
    /// Include archived tasks, which a list leaves out by default.
    pub archived: bool,
}

impl RecordFilter {
    /// Whether a filter that only tasks carry, a priority or an assignee, is set.
    fn tasks_only(&self) -> bool {
        !self.priorities.is_empty() || self.assignee.is_some()
    }

    /// Refuse an unknown kind or status, a status the kind does not have, and
    /// a task-only filter on another kind, with
    /// [`StorageError::InvalidFilter`]. [`list_records`] runs it first.
    pub fn validate(&self) -> Result<()> {
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

/// The filters of `arcl list` as a caller receives them, before any is
/// checked: a kind and status names, priority names, full or short record
/// IDs, an assignee, and whether to include archived tasks. The CLI and the
/// MCP `list` tool both build their [`RecordFilter`] with [`Self::resolve`].
#[derive(Clone, Debug, Default)]
pub struct RecordFilterArgs {
    /// One of [`RECORD_KINDS`].
    pub kind: Option<String>,
    /// Stored statuses, such as `pending`.
    pub statuses: Vec<String>,
    /// Task priority names, such as `high`.
    pub priorities: Vec<String>,
    /// A release ID, full or short.
    pub release: Option<String>,
    /// A spec ID, full or short.
    pub spec: Option<String>,
    /// A plan ID, full or short.
    pub plan: Option<String>,
    /// A phase ID, full or short.
    pub phase: Option<String>,
    /// A task ID, full or short, whose direct subtasks to list.
    pub parent: Option<String>,
    /// A claim name; surrounding whitespace is trimmed.
    pub assignee: Option<String>,
    /// Include archived tasks.
    pub all: bool,
}

impl RecordFilterArgs {
    /// Check the arguments and build the [`RecordFilter`] they name.
    ///
    /// The checks run in a fixed order: the kind, then the priorities, then
    /// the statuses and what the kind allows, then the release, spec, plan,
    /// phase and parent IDs. A refused kind, status or combination fails with
    /// [`StorageError::InvalidFilter`], and a priority or ID that does not
    /// parse with a [`DomainError`].
    ///
    /// `resolve` takes an ID argument and the full-ID prefix of the kind it
    /// must name, such as `arcl-s-`, and returns a full ID: a full ID as it
    /// is, a short ID resolved among records of that kind first. An ID of
    /// another kind then fails to parse. [`list_records`] refuses an ID that
    /// names no record.
    pub fn resolve<E>(
        self, mut resolve: impl FnMut(&str, &'static str) -> std::result::Result<String, E>,
    ) -> std::result::Result<RecordFilter, E>
    where
        E: From<DomainError> + From<StorageError>,
    {
        RecordFilter { kind: self.kind.clone(), ..RecordFilter::default() }.validate()?;
        let mut filter = RecordFilter {
            priorities: self
                .priorities
                .iter()
                .map(|value| TaskPriority::parse(value))
                .collect::<std::result::Result<Vec<_>, _>>()?,
            assignee: self.assignee.map(|name| name.trim().to_owned()),
            kind: self.kind,
            statuses: self.statuses,
            archived: self.all,
            ..RecordFilter::default()
        };
        filter.validate()?;
        filter.release_id = filter_id(self.release, &mut resolve)?;
        filter.spec_id = filter_id(self.spec, &mut resolve)?;
        filter.plan_id = filter_id(self.plan, &mut resolve)?;
        filter.phase_id = filter_id(self.phase, &mut resolve)?;
        filter.parent_id = filter_id(self.parent, &mut resolve)?;
        Ok(filter)
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
    /// How many archived tasks, counting their subtasks, the tree left out
    /// directly below this node. They count as done in progress counts.
    pub archived_tasks: usize,
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
            && (filter.archived || task.archived_at.is_none())
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
/// Archived tasks appear only with `all`, or as `root` with their subtree;
/// otherwise each node counts those left out below it in `archived_tasks`.
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
            archived_tasks: 0,
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
            archived_tasks: 0,
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
            archived_tasks: 0,
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
            archived_tasks: 0,
        });
    }
    nodes.sort_by(|left, right| (tree_rank(left.kind), &left.id).cmp(&(tree_rank(right.kind), &right.id)));
    let archived = graph
        .tasks
        .iter()
        .filter(|task| task.archived_at.is_some())
        .map(|task| task.id.to_string())
        .collect::<HashSet<_>>();
    let Some(root) = root else {
        if !all {
            hide_archived(&mut nodes, &archived);
        }
        return Ok(nodes);
    };
    let Some(node) = find_tree_node(&nodes, root) else {
        return Err(tree_root_error(root));
    };
    let mut node = node.clone();
    if !all && !archived.contains(&node.id) {
        hide_archived_below(&mut node, &archived);
    }
    Ok(vec![node])
}

/// Remove the nodes of `archived` tasks from `nodes` and every subtree, and
/// count each removed task and its subtasks on the node it sat below.
fn hide_archived(nodes: &mut Vec<RecordTreeNode>, archived: &HashSet<String>) {
    nodes.retain(|node| !is_archived_task(node, archived));
    for node in nodes {
        hide_archived_below(node, archived);
    }
}

/// Remove the nodes of `archived` tasks below `node`, at any depth, and count
/// each removed task and its subtasks on the node it sat below, `node`
/// included.
fn hide_archived_below(node: &mut RecordTreeNode, archived: &HashSet<String>) {
    let (hidden, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut node.children)
        .into_iter()
        .partition(|child| is_archived_task(child, archived));
    node.archived_tasks += hidden.iter().map(count_tasks).sum::<usize>();
    node.children = kept;
    for child in &mut node.children {
        hide_archived_below(child, archived);
    }
}

/// Whether `node` is the node of a task in `archived`.
fn is_archived_task(node: &RecordTreeNode, archived: &HashSet<String>) -> bool {
    node.kind == "task" && archived.contains(&node.id)
}

/// How many task nodes `node`'s subtree holds, counting `node`.
fn count_tasks(node: &RecordTreeNode) -> usize {
    usize::from(node.kind == "task") + node.archived_tasks + node.children.iter().map(count_tasks).sum::<usize>()
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
                archived_tasks: 0,
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
        archived_tasks: 0,
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
        archived_tasks: 0,
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

/// The ID of kind `T` that `value` names, through `resolve`, or none when
/// `value` is none.
fn filter_id<T, E>(
    value: Option<String>, resolve: &mut impl FnMut(&str, &'static str) -> std::result::Result<String, E>,
) -> std::result::Result<Option<T>, E>
where
    T: PrefixedId,
    E: From<DomainError>,
{
    value
        .map(|value| {
            let id = resolve(&value, T::PREFIX)?;
            T::parse(&id).map_err(|error| E::from(DomainError::InvalidId(error)))
        })
        .transpose()
}

fn status_matches(statuses: &[String], status: &str) -> bool {
    statuses.is_empty() || statuses.iter().any(|candidate| candidate == status)
}

#[cfg(test)]
mod tests {
    use super::{RecordFilterArgs, StorageError};
    use crate::Database;
    use arcl_core::domain::*;

    /// A refused [`RecordFilterArgs`], as a caller's error type wraps it.
    #[derive(Debug)]
    enum Refused {
        Domain(DomainError),
        Storage(StorageError),
    }

    impl From<DomainError> for Refused {
        fn from(error: DomainError) -> Self {
            Self::Domain(error)
        }
    }

    impl From<StorageError> for Refused {
        fn from(error: StorageError) -> Self {
            Self::Storage(error)
        }
    }

    fn full(value: &str, _prefix: &'static str) -> Result<String, Refused> {
        Ok(value.to_owned())
    }

    fn refused_filter(args: RecordFilterArgs) -> String {
        match args.resolve(full) {
            Err(Refused::Storage(StorageError::InvalidFilter { message })) => message,
            other => panic!("expected an invalid filter, got {other:?}"),
        }
    }

    #[test]
    fn the_kind_is_checked_before_priorities_and_statuses() {
        let message = refused_filter(RecordFilterArgs {
            kind: Some("epic".to_owned()),
            priorities: vec!["urgent".to_owned()],
            statuses: vec!["nope".to_owned()],
            ..RecordFilterArgs::default()
        });
        assert!(message.starts_with("unknown kind `epic`"), "{message}");
    }

    #[test]
    fn priorities_are_parsed_before_statuses_are_checked() {
        let error = RecordFilterArgs {
            kind: Some("spec".to_owned()),
            priorities: vec!["urgent".to_owned()],
            statuses: vec!["pending".to_owned()],
            ..RecordFilterArgs::default()
        }
        .resolve(full)
        .expect_err("an unknown priority refuses");
        assert!(matches!(error, Refused::Domain(_)), "{error:?}");
    }

    #[test]
    fn statuses_and_task_only_filters_must_fit_the_kind() {
        let status = refused_filter(RecordFilterArgs {
            kind: Some("capture".to_owned()),
            statuses: vec!["pending".to_owned()],
            ..RecordFilterArgs::default()
        });
        assert_eq!(status, "status `pending` does not apply to capture records");
        let priority = refused_filter(RecordFilterArgs {
            kind: Some("plan".to_owned()),
            priorities: vec!["high".to_owned()],
            ..RecordFilterArgs::default()
        });
        assert_eq!(priority, "priority filters apply only to tasks");
    }

    #[test]
    fn ids_resolve_in_order_with_the_prefix_of_their_kind() {
        let (release, spec, plan, phase, parent) = (
            ReleaseId::new(),
            SpecId::new(),
            PlanId::new(),
            PhaseId::new(),
            TaskId::new(),
        );
        let mut asked = Vec::new();
        let filter = RecordFilterArgs {
            kind: Some("task".to_owned()),
            statuses: vec!["pending".to_owned()],
            priorities: vec!["high".to_owned(), "low".to_owned()],
            release: Some(release.to_string()),
            spec: Some(spec.to_string()),
            plan: Some(plan.to_string()),
            phase: Some(phase.to_string()),
            parent: Some(parent.to_string()),
            assignee: Some("  storm ".to_owned()),
            all: true,
        }
        .resolve(|value, prefix| {
            asked.push(prefix);
            full(value, prefix)
        })
        .expect("filter resolves");
        assert_eq!(
            asked,
            [
                ReleaseId::PREFIX,
                SpecId::PREFIX,
                PlanId::PREFIX,
                PhaseId::PREFIX,
                TaskId::PREFIX
            ]
        );
        assert_eq!(filter.kind.as_deref(), Some("task"));
        assert_eq!(filter.statuses, ["pending"]);
        assert_eq!(filter.priorities, [TaskPriority::High, TaskPriority::Low]);
        assert_eq!(
            (
                filter.release_id,
                filter.spec_id,
                filter.plan_id,
                filter.phase_id,
                filter.parent_id
            ),
            (Some(release), Some(spec), Some(plan), Some(phase), Some(parent))
        );
        assert_eq!(filter.assignee.as_deref(), Some("storm"));
        assert!(filter.archived);
    }

    #[test]
    fn a_refused_name_resolves_no_id() {
        let error = RecordFilterArgs {
            kind: Some("note".to_owned()),
            statuses: vec!["open".to_owned()],
            spec: Some("anything".to_owned()),
            ..RecordFilterArgs::default()
        }
        .resolve(|_, _| -> Result<String, Refused> { panic!("no ID resolves after a refused status") })
        .expect_err("notes have no status");
        assert!(
            matches!(error, Refused::Storage(StorageError::InvalidFilter { .. })),
            "{error:?}"
        );
    }

    #[test]
    fn an_id_of_another_kind_fails_to_parse() {
        let error = RecordFilterArgs { spec: Some(TaskId::new().to_string()), ..RecordFilterArgs::default() }
            .resolve(full)
            .expect_err("a task ID is not a spec ID");
        assert!(matches!(error, Refused::Domain(DomainError::InvalidId(_))), "{error:?}");
    }

    #[test]
    fn short_ids_resolve_and_unknown_ids_fail_when_listed() {
        let mut database = Database::open_in_memory().expect("database opens");
        let spec = database
            .create_spec("Feature".to_owned(), String::new(), String::new())
            .expect("spec creates");
        let plan = database
            .create_plan(spec.id, "Roadmap".to_owned(), String::new())
            .expect("plan creates");
        let resolve =
            |value: &str, prefix: &'static str| database.resolve_record_id(value, Some(prefix)).map_err(Refused::from);
        let full_id = spec.id.to_string();
        let short = &full_id[full_id.len() - 8..];
        let filter = RecordFilterArgs { spec: Some(short.to_owned()), ..RecordFilterArgs::default() }
            .resolve(resolve)
            .expect("a short spec ID resolves");
        assert_eq!(filter.spec_id, Some(spec.id));
        let listed = database.list_records(&filter).expect("records list");
        let ids = listed.iter().map(|record| record.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids, [plan.id.to_string(), spec.id.to_string()]);

        let unknown = RecordFilterArgs { spec: Some(SpecId::new().to_string()), ..RecordFilterArgs::default() }
            .resolve(resolve)
            .expect("a full ID resolves as it is");
        let error = database.list_records(&unknown).expect_err("no such spec");
        assert!(matches!(error, StorageError::SpecNotFound { .. }), "{error:?}");
    }
}
