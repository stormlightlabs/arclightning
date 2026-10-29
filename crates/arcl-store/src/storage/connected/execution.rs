use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, TransactionBehavior, params};
use serde::Serialize;

use super::{
    Result, StorageError, gates, gates_for_task, notes_for_task, phases, planning_dependencies, planning_task,
    planning_tasks, specs,
};
use arcl_core::domain::{
    ContainerStatus, DomainError, EdgeViolation, Gate, GateStatus, Phase, PhaseId, Plan, PlanId, PlanningTask,
    ProjectId, Spec, SpecId, TaskAction, TaskDependency, TaskId, TaskNote, TaskPriority, TaskStatus, validate_assignee,
    validate_blocker_edges,
};

/// The most task notes [`context`] includes, newest first. The packet counts
/// the rest in `omitted_notes`.
pub const CONTEXT_NOTE_LIMIT: usize = 10;

/// What completing or cancelling a task does with its open descendants.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OpenSubtasks {
    /// Refuse while any descendant is pending, in progress or parked.
    #[default]
    Refuse,
    /// Change only the task and leave its descendants as they are.
    Leave,
    /// Complete the task and its pending and in-progress descendants.
    Complete,
}

/// The fixed set of conditions that keep a connected-model task from being
/// ready. JSON writes each as a snake_case `kind`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ReadinessReasonKind {
    /// The task is not pending. Names the task.
    Status,
    /// The task has child tasks. Names the task.
    HasChildren,
    /// An ancestor task is parked, completed or cancelled. Names the ancestor.
    ClosedAncestor,
    /// The effective spec, plan or phase is not open. Names the container.
    ClosedContainer,
    /// One of the task's own blockers is unfinished. Names the blocker.
    DirectBlocker,
    /// A blocker held by an ancestor is unfinished. Names the ancestor and
    /// the blocker.
    InheritedBlocker,
    /// A direct or inherited blocker was cancelled and needs a decision.
    /// Names the blocker, and the ancestor when inherited.
    CancelledBlocker,
    /// A direct or inherited blocker task does not exist. Names the blocker
    /// ID, and the ancestor when inherited.
    MissingBlocker,
    /// One of the task's blocker edges breaks the blocking rules. Names the
    /// cycle path, or the edge's task and blocker with the parent chain.
    InvalidEdge,
    /// The task has a gate that `arcl gate check` has not found met. Names
    /// the gate, with its kind and target.
    OpenGate,
}

/// Optional filters for connected-model ready work.
#[derive(Clone, Debug, Default)]
pub struct PlanningReadyFilter {
    /// Restrict results to one or more priorities.
    pub priorities: Vec<TaskPriority>,
    /// Restrict results to the effective specification ancestry.
    pub spec_id: Option<SpecId>,
    /// Restrict results to the effective plan ancestry.
    pub plan_id: Option<PlanId>,
    /// Restrict results to the effective phase ancestry.
    pub phase_id: Option<PhaseId>,
    /// Restrict results to direct children of one task.
    pub parent_id: Option<TaskId>,
}

/// Every readiness condition that applies to a connected-model task.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PlanningReadiness {
    /// Whether the task is an actionable pending leaf with satisfied blockers.
    pub ready: bool,
    /// Whether at least one blocker of the task or of an ancestor task is
    /// unfinished or missing, or the task has an open gate.
    pub blocked: bool,
    /// Every condition that prevents readiness, in a stable order.
    pub reasons: Vec<ReadinessReason>,
}

impl PlanningReadiness {
    /// The human text of each reason, in order.
    pub fn messages(&self) -> Vec<String> {
        self.reasons.iter().map(|reason| reason.message.clone()).collect()
    }
}

/// One condition that keeps a task from being ready, with its kind, its human
/// text and the IDs of the records it names. JSON omits the ID fields a kind
/// does not use.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ReadinessReason {
    /// Which condition this is.
    pub kind: ReadinessReasonKind,
    /// The user-facing explanation that human and plain output print.
    pub message: String,
    /// The task the condition is on: the inspected task for `status` and
    /// `has_children`, or the task that holds the edge for an
    /// `invalid_edge`: the inspected task on a cycle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskId>,
    /// The closed ancestor, or the ancestor that holds an inherited blocker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ancestor: Option<TaskId>,
    /// The blocker task ID. For an `invalid_edge` cycle, the step after
    /// `task` on `path`, which is `task` itself on a self-loop.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocker: Option<TaskId>,
    /// The ID of the spec, plan or phase that is not open.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    /// For an `invalid_edge`, the cycle closed by repeating its first task,
    /// or the parent chain from the descendant up to the ancestor.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "schema", schemars(default))]
    pub path: Vec<TaskId>,
    /// For an `open_gate`, the gate: its ID, kind and target.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate: Option<Gate>,
}

impl ReadinessReason {
    fn new(kind: ReadinessReasonKind, message: String) -> Self {
        Self {
            kind,
            message,
            task: None,
            ancestor: None,
            blocker: None,
            container: None,
            path: Vec::new(),
            gate: None,
        }
    }
}

impl std::fmt::Display for ReadinessReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// A connected-model blocker of a task or of one of its ancestors, with its
/// completion evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PlanningBlockerView {
    /// The blocking task.
    pub task: PlanningTask,
    /// Whether the blocker has completed.
    pub completed: bool,
    /// Markdown evidence recorded on the blocker.
    pub evidence: String,
    /// The ancestor task that holds the dependency when the blocker is
    /// inherited, or `None` for a direct blocker. JSON omits it for direct
    /// blockers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ancestor: Option<TaskId>,
}

/// An enriched connected-model task view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PlanningTaskView {
    /// The task being inspected.
    pub task: PlanningTask,
    /// The computed readiness result.
    pub readiness: PlanningReadiness,
    /// Direct blockers of the task.
    pub blockers: Vec<PlanningBlockerView>,
    /// Blockers held by ancestor tasks, nearest ancestor first, each naming
    /// the ancestor that holds it. A blocker that is the task itself is left
    /// out.
    pub inherited_blockers: Vec<PlanningBlockerView>,
    /// Tasks that directly depend on this task.
    pub dependents: Vec<PlanningTask>,
    /// Direct child tasks.
    pub children: Vec<PlanningTask>,
    /// Parent tasks ordered from the oldest ancestor to the immediate parent.
    pub ancestors: Vec<PlanningTask>,
    /// The effective specification, if this task is under one.
    pub spec: Option<Spec>,
    /// The effective plan, if this task is under one.
    pub plan: Option<Plan>,
    /// The effective phase, if this task is under one.
    pub phase: Option<Phase>,
    /// Every note on the task, newest first.
    pub notes: Vec<TaskNote>,
    /// Every gate on the task, open or met, oldest first.
    pub gates: Vec<Gate>,
    /// The task the capture promoted to this task was filed from, if any.
    pub source_task_id: Option<TaskId>,
}

/// The focused context packet for a connected-model task.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PlanningContext {
    /// The task, including its Markdown body, handoff, and evidence.
    pub task: PlanningTask,
    /// Parent tasks ordered from the oldest ancestor to the immediate parent.
    pub ancestors: Vec<PlanningTask>,
    /// The effective specification, if this task is under one.
    pub spec: Option<Spec>,
    /// The effective plan, if this task is under one.
    pub plan: Option<Plan>,
    /// The effective phase, if this task is under one.
    pub phase: Option<Phase>,
    /// Direct blockers and their evidence.
    pub blockers: Vec<PlanningBlockerView>,
    /// Blockers held by ancestor tasks, nearest ancestor first, with their
    /// evidence and the ancestor that holds each.
    pub inherited_blockers: Vec<PlanningBlockerView>,
    /// Tasks that directly depend on this task.
    pub dependents: Vec<PlanningTask>,
    /// The computed readiness result.
    pub readiness: PlanningReadiness,
    /// Completed direct and inherited blockers that supplied evidence, one
    /// entry per blocker task. A direct entry wins over an inherited one.
    pub completion_evidence: Vec<PlanningBlockerView>,
    /// The task's [`CONTEXT_NOTE_LIMIT`] most recent notes, newest first.
    pub notes: Vec<TaskNote>,
    /// How many older notes the packet leaves out; `arcl show` lists them.
    pub omitted_notes: usize,
    /// Every gate on the task, open or met, oldest first.
    pub gates: Vec<Gate>,
}

/// One task in the work list: the task, its readiness and its effective
/// phase.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PlanningWorkItem {
    /// The task.
    pub task: PlanningTask,
    /// The computed readiness result, as [`planning_task_view`] reports it.
    pub readiness: PlanningReadiness,
    /// The task's effective phase: its own, or its nearest ancestor's.
    pub phase_id: Option<PhaseId>,
}

/// Every task with its readiness, and every phase, read in one pass.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PlanningWork {
    /// Every task, ordered by position, then ID.
    pub tasks: Vec<PlanningWorkItem>,
    /// Every phase, ordered by plan, then position, then ID.
    pub phases: Vec<Phase>,
}

/// The result of completing a connected-model task.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlanningCompletion {
    /// The completed task.
    pub task: PlanningTask,
    /// The descendants this completion completed, in tree order. Empty unless
    /// the completion used [`OpenSubtasks::Complete`].
    pub completed_subtasks: Vec<PlanningTask>,
    /// Tasks that were not ready before the completion and are ready after
    /// it, in ready-work order. Empty for any other lifecycle action.
    pub unblocked: Vec<PlanningTask>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Ancestry {
    spec_id: Option<SpecId>,
    plan_id: Option<PlanId>,
    phase_id: Option<PhaseId>,
}

struct PlanningData {
    tasks: HashMap<TaskId, PlanningTask>,
    /// For each task with child tasks, the IDs of those children, in no
    /// particular order. A task missing here is a leaf.
    children: HashMap<TaskId, Vec<TaskId>>,
    specs: HashMap<SpecId, Spec>,
    plans: HashMap<PlanId, Plan>,
    phases: HashMap<PhaseId, Phase>,
    dependencies: Vec<TaskDependency>,
    /// Every blocker rule violation already stored in the project.
    violations: Vec<EdgeViolation>,
    /// For each task, the indexes into `violations` of those that involve one
    /// of its own blocker edges, in violation order.
    violations_by_task: HashMap<TaskId, Vec<usize>>,
    /// For each task with an open gate, those gates, oldest first.
    open_gates: HashMap<TaskId, Vec<Gate>>,
}

/// Compute ready leaf tasks without requiring a plan, phase, or milestone.
pub fn planning_ready_tasks(
    connection: &Connection, project_id: ProjectId, filter: &PlanningReadyFilter,
) -> Result<Vec<PlanningTask>> {
    let data = load_data(connection, project_id)?;
    validate_filter(&data, filter)?;
    let mut ready = Vec::new();

    for task in data.tasks.values() {
        if filter
            .parent_id
            .is_some_and(|parent_id| task.parent_id != Some(parent_id))
        {
            continue;
        }
        if data.children.contains_key(&task.id) {
            continue;
        }
        let ancestry = effective_ancestry(&data, task.id, &mut HashSet::new())?;
        if !matches_ancestry(&ancestry, filter)
            || !filter.priorities.is_empty() && !filter.priorities.contains(&task.priority)
        {
            continue;
        }
        let readiness = readiness(&data, task, &ancestry)?;
        if readiness.ready {
            ready.push(task.clone());
        }
    }

    ready.sort_by(|left, right| {
        left.priority
            .cmp(&right.priority)
            .then_with(|| left.position.cmp(&right.position))
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(ready)
}

/// Build a task view using only the connected planning records relevant to it.
pub fn planning_task_view(connection: &Connection, id: TaskId) -> Result<PlanningTaskView> {
    let task =
        planning_task(connection, id)?.ok_or_else(|| StorageError::PlanningTaskNotFound { id: id.to_string() })?;
    let data = load_data(connection, task.project_id)?;
    let ancestry_ids = effective_ancestry(&data, id, &mut HashSet::new())?;
    let ancestors = parent_chain(&data, id)?;
    let blockers = blocker_views(&data, id, None)?;
    let mut inherited_blockers = Vec::new();
    for ancestor in ancestors.iter().rev() {
        inherited_blockers.extend(
            blocker_views(&data, ancestor.id, Some(ancestor.id))?
                .into_iter()
                .filter(|view| view.task.id != id),
        );
    }
    let dependents = dependent_tasks(&data, id);
    let children = child_tasks(&data, id);
    let readiness = readiness(&data, &task, &ancestry_ids)?;
    let mut notes = notes_for_task(connection, id)?;
    notes.reverse();
    let gates = gates_for_task(connection, id)?;
    let source_task_id = super::promoted_source_task(connection, &id.to_string())?;

    Ok(PlanningTaskView {
        task,
        readiness,
        blockers,
        inherited_blockers,
        dependents,
        children,
        ancestors,
        spec: ancestry_ids.spec_id.and_then(|value| data.specs.get(&value).cloned()),
        plan: ancestry_ids.plan_id.and_then(|value| data.plans.get(&value).cloned()),
        phase: ancestry_ids.phase_id.and_then(|value| data.phases.get(&value).cloned()),
        notes,
        gates,
        source_task_id,
    })
}

/// Read every task with its readiness and effective phase, and every phase,
/// loading the project's planning records once.
pub fn planning_work(connection: &Connection, project_id: ProjectId) -> Result<PlanningWork> {
    let data = load_data(connection, project_id)?;
    let mut tasks = Vec::with_capacity(data.tasks.len());
    for task in data.tasks.values() {
        let ancestry = effective_ancestry(&data, task.id, &mut HashSet::new())?;
        let readiness = readiness(&data, task, &ancestry)?;
        tasks.push(PlanningWorkItem { task: task.clone(), readiness, phase_id: ancestry.phase_id });
    }
    tasks.sort_by(|left, right| {
        left.task
            .position
            .cmp(&right.task.position)
            .then_with(|| left.task.id.cmp(&right.task.id))
    });
    Ok(PlanningWork { tasks, phases: phases(connection, project_id)? })
}

/// Build the bounded task context packet used by agents and execution adapters.
pub fn context(connection: &Connection, id: TaskId) -> Result<PlanningContext> {
    let view = planning_task_view(connection, id)?;
    let mut seen = HashSet::new();
    let completion_evidence = view
        .blockers
        .iter()
        .chain(&view.inherited_blockers)
        .filter(|blocker| blocker.completed && !blocker.evidence.is_empty())
        .filter(|blocker| seen.insert(blocker.task.id))
        .cloned()
        .collect();
    let mut notes = view.notes;
    let omitted_notes = notes.len().saturating_sub(CONTEXT_NOTE_LIMIT);
    notes.truncate(CONTEXT_NOTE_LIMIT);
    Ok(PlanningContext {
        task: view.task,
        ancestors: view.ancestors,
        spec: view.spec,
        plan: view.plan,
        phase: view.phase,
        blockers: view.blockers,
        inherited_blockers: view.inherited_blockers,
        dependents: view.dependents,
        readiness: view.readiness,
        completion_evidence,
        notes,
        omitted_notes,
        gates: view.gates,
    })
}

/// Apply one connected-model task lifecycle action atomically.
///
/// A start through this entry point is never forced; use
/// [`start_planning_task`] to choose.
pub fn transition_planning_task(
    connection: &mut Connection, project_id: ProjectId, id: TaskId, action: TaskAction, allow_open_children: bool,
) -> Result<PlanningTask> {
    let open_subtasks = if allow_open_children { OpenSubtasks::Leave } else { OpenSubtasks::Refuse };
    transition(connection, project_id, id, action, open_subtasks, false, None, None).map(|completion| completion.task)
}

/// Start a connected-model task atomically.
///
/// A pending task must be ready, apart from having child tasks, unless `force`
/// is set. Starting an in-progress task is a no-op either way, except that a
/// task an assignee holds is refused with [`StorageError::TaskClaimed`]; only
/// [`claim_planning_task`] with the holder's name starts it again.
pub fn start_planning_task(
    connection: &mut Connection, project_id: ProjectId, id: TaskId, force: bool,
) -> Result<PlanningTask> {
    transition(
        connection,
        project_id,
        id,
        TaskAction::Start,
        OpenSubtasks::Refuse,
        force,
        None,
        None,
    )
    .map(|completion| completion.task)
}

/// Start a connected-model task and set its assignee in one transaction.
///
/// Readiness and `force` work as in [`start_planning_task`]. A task another
/// assignee holds is refused with [`StorageError::TaskClaimed`]. A claim by
/// the current assignee changes nothing, and claiming an unassigned task that
/// is already in progress only sets the assignee. An invalid `assignee` is
/// refused with [`StorageError::InvalidClaimName`] before the task is read.
pub fn claim_planning_task(
    connection: &mut Connection, project_id: ProjectId, id: TaskId, force: bool, assignee: &str,
) -> Result<PlanningTask> {
    let assignee =
        validate_assignee(assignee).map_err(|_| StorageError::InvalidClaimName { value: assignee.to_owned() })?;
    transition(
        connection,
        project_id,
        id,
        TaskAction::Start,
        OpenSubtasks::Refuse,
        force,
        None,
        Some(assignee),
    )
    .map(|completion| completion.task)
}

/// Start the first task [`planning_ready_tasks`] returns for `filter`, in one
/// write transaction.
///
/// The transaction begins `IMMEDIATE`, so it holds SQLite's write lock from
/// selection to commit: a second process waits for it and then selects from
/// the committed state, so two calls never start the same task. With `claim`,
/// the task gets that assignee, as in [`claim_planning_task`]. Returns `None`,
/// writing nothing, when no task matches.
pub fn start_next_planning_task(
    connection: &mut Connection, project_id: ProjectId, filter: &PlanningReadyFilter, claim: Option<&str>,
) -> Result<Option<PlanningTask>> {
    let claim = claim
        .map(|name| validate_assignee(name).map_err(|_| StorageError::InvalidClaimName { value: name.to_owned() }))
        .transpose()?;
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let next = planning_ready_tasks(&tx, project_id, filter)?.into_iter().next();
    let Some(next) = next else {
        return Ok(None);
    };
    let completion = transition_in(
        &tx,
        project_id,
        next.id,
        TaskAction::Start,
        OpenSubtasks::Refuse,
        false,
        None,
        claim,
    )?;
    tx.commit()?;
    Ok(Some(completion.task))
}

/// Complete a connected-model task and optionally store Markdown evidence atomically.
///
/// `open_subtasks` chooses what happens to open descendants. With
/// [`OpenSubtasks::Complete`], the task and its pending and in-progress
/// descendants complete in one transaction, and the evidence goes on the task
/// only. That is refused, writing nothing, while a descendant is parked or the
/// task or an open descendant has an unfinished blocker from outside the
/// subtree, held directly or by an ancestor.
pub fn complete_planning_task(
    connection: &mut Connection, project_id: ProjectId, id: TaskId, open_subtasks: OpenSubtasks,
    evidence: Option<String>,
) -> Result<PlanningCompletion> {
    transition(
        connection,
        project_id,
        id,
        TaskAction::Complete,
        open_subtasks,
        false,
        evidence,
        None,
    )
}

/// Store a handoff note and park an in-progress connected-model task atomically.
pub fn handoff_planning_task(
    connection: &mut Connection, project_id: ProjectId, id: TaskId, note: String,
) -> Result<PlanningTask> {
    // Take the write lock before reading so a concurrent claim waits for
    // this release instead of failing on a lock upgrade.
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = planning_task(&tx, id)?.ok_or_else(|| StorageError::PlanningTaskNotFound { id: id.to_string() })?;
    ensure_project_task(&current, project_id)?;
    validate_task_graph(&tx, project_id, &current, false)?;
    if current.status != TaskStatus::InProgress {
        return Err(StorageError::InvalidPlanningTask(DomainError::InvalidTransition {
            entity: "task",
            action: "handoff",
            from: current.status.as_str().to_owned(),
        }));
    }
    tx.execute(
        "UPDATE planning_tasks SET handoff = ?1, status = 'parked', assignee = NULL WHERE id = ?2",
        params![note, id.to_string()],
    )?;
    tx.commit()?;
    Ok(PlanningTask { status: TaskStatus::Parked, handoff: note, assignee: None, ..current })
}

/// Apply `action` to task `id` in one transaction.
///
/// `claim` names the assignee a start sets. A start without the holder's
/// name is refused while the task has an assignee. Parking, completing and
/// cancelling clear the assignee, whoever runs them. The transaction is immediate, so a
/// concurrent writer waits up to the busy timeout rather than failing.
#[allow(clippy::too_many_arguments)]
fn transition(
    connection: &mut Connection, project_id: ProjectId, id: TaskId, action: TaskAction, open_subtasks: OpenSubtasks,
    force: bool, evidence: Option<String>, claim: Option<String>,
) -> Result<PlanningCompletion> {
    // Take the write lock before reading, so two claims on one task run one
    // after the other and the second sees the first's assignee. A deferred
    // transaction would fail the second with SQLITE_BUSY on the lock upgrade.
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let completion = transition_in(&tx, project_id, id, action, open_subtasks, force, evidence, claim)?;
    tx.commit()?;
    Ok(completion)
}

/// Apply `action` to task `id` inside the caller's transaction, which the
/// caller commits. See [`transition`].
#[allow(clippy::too_many_arguments)]
fn transition_in(
    tx: &Connection, project_id: ProjectId, id: TaskId, action: TaskAction, open_subtasks: OpenSubtasks, force: bool,
    evidence: Option<String>, claim: Option<String>,
) -> Result<PlanningCompletion> {
    let current = planning_task(tx, id)?.ok_or_else(|| StorageError::PlanningTaskNotFound { id: id.to_string() })?;
    ensure_project_task(&current, project_id)?;
    validate_task_graph(tx, project_id, &current, false)?;
    // A start by anyone but the holder is refused, with or without a claim
    // and whatever `force` says: `force` overrides readiness, not claims.
    if matches!(action, TaskAction::Start)
        && let Some(holder) = current
            .assignee
            .as_ref()
            .filter(|holder| claim.as_ref() != Some(*holder))
    {
        return Err(StorageError::TaskClaimed { id: id.to_string(), assignee: holder.clone() });
    }
    let next_status = current
        .status
        .apply(action)
        .map_err(StorageError::InvalidPlanningTask)?;
    let completed_subtasks = if open_subtasks == OpenSubtasks::Complete {
        let data = load_data(tx, project_id)?;
        open_subtasks_to_complete(&data, &current)?
    } else {
        Vec::new()
    };
    let assignee = match next_status {
        TaskStatus::Parked | TaskStatus::Completed | TaskStatus::Cancelled => None,
        TaskStatus::Pending | TaskStatus::InProgress => claim.or_else(|| current.assignee.clone()),
    };
    if next_status == current.status
        && evidence.is_none()
        && completed_subtasks.is_empty()
        && assignee == current.assignee
    {
        return Ok(PlanningCompletion { task: current, completed_subtasks, unblocked: Vec::new() });
    }
    if matches!(action, TaskAction::Start) && current.status == TaskStatus::Pending && !force {
        let data = load_data(tx, project_id)?;
        let ancestry = effective_ancestry(&data, id, &mut HashSet::new())?;
        let readiness = readiness_counting_children(&data, &current, &ancestry, false)?;
        if !readiness.ready {
            return Err(StorageError::InvalidPlanningTask(DomainError::NotReady {
                id: id.to_string(),
                reasons: readiness.messages(),
            }));
        }
    }
    if matches!(action, TaskAction::Complete | TaskAction::Cancel) && open_subtasks == OpenSubtasks::Refuse {
        let has_open_descendants: bool = tx.query_row(
            "WITH RECURSIVE descendants(id, status) AS (
                 SELECT id, status FROM planning_tasks WHERE parent_id = ?1
                 UNION ALL
                 SELECT task.id, task.status FROM planning_tasks task
                 JOIN descendants parent ON task.parent_id = parent.id
             )
             SELECT EXISTS (SELECT 1 FROM descendants WHERE status NOT IN ('completed', 'cancelled'))",
            params![id.to_string()],
            |row| row.get(0),
        )?;
        if has_open_descendants {
            return Err(StorageError::InvalidPlanningTask(DomainError::OpenDescendants {
                entity: "task",
                id: id.to_string(),
                action: action.as_str(),
            }));
        }
    }
    let completes = matches!(action, TaskAction::Complete);
    let ready_before: HashSet<TaskId> = if completes {
        planning_ready_tasks(tx, project_id, &PlanningReadyFilter::default())?
            .into_iter()
            .map(|task| task.id)
            .collect()
    } else {
        HashSet::new()
    };
    if next_status == current.status {
        tx.execute(
            "UPDATE planning_tasks SET evidence = ?1, assignee = ?2 WHERE id = ?3",
            params![
                evidence.as_deref().unwrap_or(&current.evidence),
                assignee,
                id.to_string()
            ],
        )?;
    } else {
        tx.execute(
            "UPDATE planning_tasks SET status = ?1, evidence = COALESCE(?2, evidence), assignee = ?3 WHERE id = ?4",
            params![next_status.as_str(), evidence, assignee, id.to_string()],
        )?;
    }
    for subtask in &completed_subtasks {
        tx.execute(
            "UPDATE planning_tasks SET status = ?1, assignee = NULL WHERE id = ?2",
            params![TaskStatus::Completed.as_str(), subtask.id.to_string()],
        )?;
    }
    let unblocked = if completes {
        planning_ready_tasks(tx, project_id, &PlanningReadyFilter::default())?
            .into_iter()
            .filter(|task| !ready_before.contains(&task.id))
            .collect()
    } else {
        Vec::new()
    };
    Ok(PlanningCompletion {
        task: PlanningTask { status: next_status, evidence: evidence.unwrap_or(current.evidence), assignee, ..current },
        completed_subtasks,
        unblocked,
    })
}

/// List the pending and in-progress descendants of `task` in tree order, each
/// already marked completed, or refuse when the subtree cannot complete as one
/// unit.
///
/// A parked descendant refuses, and so does an unfinished or missing blocker
/// outside the subtree held by the task, by an open descendant, or by an
/// ancestor of either. Blockers inside the subtree do not count, whatever
/// their status.
fn open_subtasks_to_complete(data: &PlanningData, task: &PlanningTask) -> Result<Vec<PlanningTask>> {
    let mut subtree = Vec::new();
    collect_subtree(data, task.id, &mut subtree);
    let inside: HashSet<TaskId> = std::iter::once(task.id)
        .chain(subtree.iter().map(|descendant| descendant.id))
        .collect();
    let is_open = |candidate: &PlanningTask| matches!(candidate.status, TaskStatus::Pending | TaskStatus::InProgress);

    let mut reasons = Vec::new();
    for descendant in subtree
        .iter()
        .filter(|descendant| descendant.status == TaskStatus::Parked)
    {
        reasons.push(format!("subtask `{}` is parked; unpark or cancel it", descendant.id));
    }

    // Each holder is the task, an open descendant, or an ancestor of one: the
    // task's ancestors from the root down, then the subtree in tree order.
    let mut holders = parent_chain(data, task.id)?;
    holders.push(task.clone());
    let mut held = HashSet::new();
    for descendant in subtree.iter().filter(|descendant| is_open(descendant)) {
        let mut current = descendant.parent_id;
        held.insert(descendant.id);
        while let Some(parent_id) = current.filter(|parent_id| *parent_id != task.id && held.insert(*parent_id)) {
            current = data.tasks.get(&parent_id).and_then(|parent| parent.parent_id);
        }
    }
    holders.extend(
        subtree
            .iter()
            .filter(|descendant| held.contains(&descendant.id))
            .cloned(),
    );
    for holder in &holders {
        for dependency in data
            .dependencies
            .iter()
            .filter(|dependency| dependency.task_id == holder.id && !inside.contains(&dependency.blocker_id))
        {
            if let Some(reason) = blocker_reason(data, dependency, None) {
                reasons.push(format!("`{}` is {reason}", holder.id));
            }
        }
    }
    if !reasons.is_empty() {
        return Err(StorageError::InvalidPlanningTask(DomainError::SubtasksNotCompletable {
            id: task.id.to_string(),
            reasons,
        }));
    }
    Ok(subtree
        .into_iter()
        .filter(|descendant| is_open(descendant))
        .map(|descendant| PlanningTask { status: TaskStatus::Completed, assignee: None, ..descendant })
        .collect())
}

/// Append the descendants of `id` to `subtree` in tree order: each task
/// before its children, and siblings by position.
fn collect_subtree(data: &PlanningData, id: TaskId, subtree: &mut Vec<PlanningTask>) {
    for child in child_tasks(data, id) {
        let child_id = child.id;
        subtree.push(child);
        collect_subtree(data, child_id, subtree);
    }
}

/// Validate a candidate task and the complete parent graph before a write.
pub(super) fn validate_task_graph(
    connection: &Connection, project_id: ProjectId, candidate: &PlanningTask, require_open_containers: bool,
) -> Result<()> {
    validate_task_graph_candidates(
        connection,
        project_id,
        std::slice::from_ref(candidate),
        require_open_containers,
    )
}

/// Validate several candidate tasks as one prospective graph. Plan application
/// uses this so moving a parent and its children does not fail on a transient
/// ancestry state between individual updates.
pub(super) fn validate_task_graph_candidates(
    connection: &Connection, project_id: ProjectId, candidates: &[PlanningTask], require_open_containers: bool,
) -> Result<()> {
    ensure_project(connection, project_id)?;
    let mut data = load_data(connection, project_id)?;
    for candidate in candidates {
        ensure_project_task(candidate, project_id)?;
        data.tasks.insert(candidate.id, candidate.clone());
    }
    data.children = index_children(&data.tasks);

    // Validate every candidate reference before evaluating effective ancestry. In
    // particular, this preserves a cross-project error instead of turning it into
    // a misleading missing-parent error. A parent in the candidate set has not
    // been written yet, so the prospective graph is authoritative for it.
    for task in data.tasks.values() {
        if let Some(spec_id) = task.spec_id {
            super::ensure_spec(connection, project_id, spec_id)?;
        }
        if let Some(plan_id) = task.plan_id {
            super::ensure_plan(connection, project_id, plan_id)?;
        }
        if let Some(phase_id) = task.phase_id {
            super::ensure_phase(connection, project_id, phase_id)?;
        }
        if let Some(parent_id) = task.parent_id {
            if parent_id == task.id {
                return Err(StorageError::InvalidPlanningTask(DomainError::SelfParent {
                    task: task.id.to_string(),
                }));
            }
            if !data.tasks.contains_key(&parent_id) {
                super::ensure_task(connection, project_id, parent_id)?;
            }
        }
    }

    for task in data.tasks.values() {
        effective_ancestry(&data, task.id, &mut HashSet::new())?;
    }
    if require_open_containers {
        for candidate in candidates {
            let ancestry = effective_ancestry(&data, candidate.id, &mut HashSet::new())?;
            ensure_open_containers(&data, ancestry)?;
        }
    }
    Ok(())
}

/// Map each parent to its child task IDs, in no particular order.
fn index_children(tasks: &HashMap<TaskId, PlanningTask>) -> HashMap<TaskId, Vec<TaskId>> {
    let mut children: HashMap<TaskId, Vec<TaskId>> = HashMap::new();
    for task in tasks.values() {
        if let Some(parent_id) = task.parent_id {
            children.entry(parent_id).or_default().push(task.id);
        }
    }
    children
}

fn load_data(connection: &Connection, project_id: ProjectId) -> Result<PlanningData> {
    let tasks: HashMap<TaskId, PlanningTask> = planning_tasks(connection, project_id)?
        .into_iter()
        .map(|task| (task.id, task))
        .collect();
    let dependencies = planning_dependencies(connection, project_id)?;
    let parents = tasks
        .values()
        .filter_map(|task| Some((task.id, task.parent_id?)))
        .collect();
    let violations = validate_blocker_edges(&parents, &dependencies);
    let children = index_children(&tasks);
    let mut violations_by_task: HashMap<TaskId, Vec<usize>> = HashMap::new();
    for (index, violation) in violations.iter().enumerate() {
        // A cycle visits each of its tasks once, and each step is an edge
        // the task holds, so every task on it but the closing repeat gets
        // the violation once.
        let holders = match violation {
            EdgeViolation::Cycle { path } => &path[..path.len().saturating_sub(1)],
            EdgeViolation::Ancestry { edge, .. } => std::slice::from_ref(&edge.task_id),
        };
        for holder in holders {
            violations_by_task.entry(*holder).or_default().push(index);
        }
    }
    let mut open_gates: HashMap<TaskId, Vec<Gate>> = HashMap::new();
    for gate in gates(connection, project_id)? {
        if gate.status == GateStatus::Open {
            open_gates.entry(gate.task_id).or_default().push(gate);
        }
    }
    Ok(PlanningData {
        tasks,
        children,
        open_gates,
        specs: specs(connection, project_id)?
            .into_iter()
            .map(|spec| (spec.id, spec))
            .collect(),
        plans: super::plans(connection, project_id)?
            .into_iter()
            .map(|plan| (plan.id, plan))
            .collect(),
        phases: phases(connection, project_id)?
            .into_iter()
            .map(|phase| (phase.id, phase))
            .collect(),
        dependencies,
        violations,
        violations_by_task,
    })
}

fn validate_filter(data: &PlanningData, filter: &PlanningReadyFilter) -> Result<()> {
    if let Some(id) = filter.spec_id
        && !data.specs.contains_key(&id)
    {
        return Err(StorageError::SpecNotFound { id: id.to_string() });
    }
    if let Some(id) = filter.plan_id
        && !data.plans.contains_key(&id)
    {
        return Err(StorageError::PlanNotFound { id: id.to_string() });
    }
    if let Some(id) = filter.phase_id
        && !data.phases.contains_key(&id)
    {
        return Err(StorageError::PhaseNotFound { id: id.to_string() });
    }
    if let Some(id) = filter.parent_id
        && !data.tasks.contains_key(&id)
    {
        return Err(StorageError::PlanningTaskNotFound { id: id.to_string() });
    }
    Ok(())
}

fn matches_ancestry(ancestry: &Ancestry, filter: &PlanningReadyFilter) -> bool {
    filter.spec_id.is_none_or(|id| ancestry.spec_id == Some(id))
        && filter.plan_id.is_none_or(|id| ancestry.plan_id == Some(id))
        && filter.phase_id.is_none_or(|id| ancestry.phase_id == Some(id))
}

fn effective_ancestry(data: &PlanningData, id: TaskId, visiting: &mut HashSet<TaskId>) -> Result<Ancestry> {
    if !visiting.insert(id) {
        return Err(StorageError::InvalidPlanningTask(DomainError::ParentCycle {
            task: id.to_string(),
            parent: id.to_string(),
        }));
    }
    let task = data
        .tasks
        .get(&id)
        .ok_or_else(|| StorageError::PlanningTaskNotFound { id: id.to_string() })?;
    let mut result = explicit_ancestry(data, task)?;
    if let Some(parent_id) = task.parent_id {
        let parent = effective_ancestry(data, parent_id, visiting)?;
        result = merge_ancestry(task.id, result, parent)?;
    }
    visiting.remove(&id);
    Ok(result)
}

fn explicit_ancestry(data: &PlanningData, task: &PlanningTask) -> Result<Ancestry> {
    let mut ancestry = Ancestry { spec_id: task.spec_id, plan_id: task.plan_id, phase_id: task.phase_id };
    if let Some(phase_id) = ancestry.phase_id {
        let phase = data
            .phases
            .get(&phase_id)
            .ok_or_else(|| StorageError::PhaseNotFound { id: phase_id.to_string() })?;
        if ancestry.plan_id.is_some_and(|plan_id| plan_id != phase.plan_id) {
            return Err(contradictory(task, "plan/phase"));
        }
        ancestry.plan_id = Some(phase.plan_id);
    }
    if let Some(plan_id) = ancestry.plan_id {
        let plan = data
            .plans
            .get(&plan_id)
            .ok_or_else(|| StorageError::PlanNotFound { id: plan_id.to_string() })?;
        if ancestry.spec_id.is_some_and(|spec_id| spec_id != plan.spec_id) {
            return Err(contradictory(task, "spec/plan"));
        }
        ancestry.spec_id = Some(plan.spec_id);
    }
    if let Some(spec_id) = ancestry.spec_id
        && !data.specs.contains_key(&spec_id)
    {
        return Err(StorageError::SpecNotFound { id: spec_id.to_string() });
    }
    Ok(ancestry)
}

fn merge_ancestry(task_id: TaskId, own: Ancestry, parent: Ancestry) -> Result<Ancestry> {
    if own
        .spec_id
        .zip(parent.spec_id)
        .is_some_and(|(left, right)| left != right)
    {
        return Err(contradictory_ids(task_id, "parent/spec"));
    }
    if own
        .plan_id
        .zip(parent.plan_id)
        .is_some_and(|(left, right)| left != right)
    {
        return Err(contradictory_ids(task_id, "parent/plan"));
    }
    if own
        .phase_id
        .zip(parent.phase_id)
        .is_some_and(|(left, right)| left != right)
    {
        return Err(contradictory_ids(task_id, "parent/phase"));
    }
    Ok(Ancestry {
        spec_id: own.spec_id.or(parent.spec_id),
        plan_id: own.plan_id.or(parent.plan_id),
        phase_id: own.phase_id.or(parent.phase_id),
    })
}

fn ensure_open_containers(data: &PlanningData, ancestry: Ancestry) -> Result<()> {
    for (entity, id, status) in [
        ancestry.spec_id.and_then(|id| {
            data.specs
                .get(&id)
                .map(|record| ("spec", id.to_string(), record.status))
        }),
        ancestry.plan_id.and_then(|id| {
            data.plans
                .get(&id)
                .map(|record| ("plan", id.to_string(), record.status))
        }),
        ancestry.phase_id.and_then(|id| {
            data.phases
                .get(&id)
                .map(|record| ("phase", id.to_string(), record.status))
        }),
    ]
    .into_iter()
    .flatten()
    {
        if status != ContainerStatus::Open {
            return Err(StorageError::InvalidPlanningTask(DomainError::InvalidContainerState {
                entity,
                id,
                status: status.as_str().to_owned(),
            }));
        }
    }
    Ok(())
}

fn readiness(data: &PlanningData, task: &PlanningTask, ancestry: &Ancestry) -> Result<PlanningReadiness> {
    readiness_counting_children(data, task, ancestry, true)
}

/// Compute readiness. `count_children` adds the "has child tasks" reason,
/// which start skips.
fn readiness_counting_children(
    data: &PlanningData, task: &PlanningTask, ancestry: &Ancestry, count_children: bool,
) -> Result<PlanningReadiness> {
    let mut reasons = Vec::new();
    if task.status != TaskStatus::Pending {
        reasons.push(ReadinessReason {
            task: Some(task.id),
            ..ReadinessReason::new(
                ReadinessReasonKind::Status,
                format!("task status is `{}`", task.status.as_str()),
            )
        });
    }
    let children = child_tasks(data, task.id);
    if count_children && !children.is_empty() {
        reasons.push(ReadinessReason {
            task: Some(task.id),
            ..ReadinessReason::new(
                ReadinessReasonKind::HasChildren,
                format!("task has {} child task(s)", children.len()),
            )
        });
    }
    let ancestors = parent_chain(data, task.id)?;
    for ancestor in &ancestors {
        if matches!(
            ancestor.status,
            TaskStatus::Parked | TaskStatus::Completed | TaskStatus::Cancelled
        ) {
            reasons.push(ReadinessReason {
                ancestor: Some(ancestor.id),
                ..ReadinessReason::new(
                    ReadinessReasonKind::ClosedAncestor,
                    format!("ancestor `{}` is `{}`", ancestor.id, ancestor.status.as_str()),
                )
            });
        }
    }
    for (entity, id, status) in [
        ancestry.spec_id.and_then(|id| {
            data.specs
                .get(&id)
                .map(|record| ("spec", id.to_string(), record.status))
        }),
        ancestry.plan_id.and_then(|id| {
            data.plans
                .get(&id)
                .map(|record| ("plan", id.to_string(), record.status))
        }),
        ancestry.phase_id.and_then(|id| {
            data.phases
                .get(&id)
                .map(|record| ("phase", id.to_string(), record.status))
        }),
    ]
    .into_iter()
    .flatten()
    {
        if status != ContainerStatus::Open {
            let message = format!("{entity} `{id}` is `{}`", status.as_str());
            reasons.push(ReadinessReason {
                container: Some(id),
                ..ReadinessReason::new(ReadinessReasonKind::ClosedContainer, message)
            });
        }
    }

    let mut blocked = false;
    // The task's own blockers come first, then each ancestor's, nearest
    // first. An ancestor blocked by this task cannot hold this task back.
    for holder in std::iter::once(task).chain(ancestors.iter().rev()) {
        let inherited = (holder.id != task.id).then_some(holder);
        for dependency in data
            .dependencies
            .iter()
            .filter(|dependency| dependency.task_id == holder.id)
            .filter(|dependency| inherited.is_none() || dependency.blocker_id != task.id)
        {
            if let Some(reason) = blocker_reason(data, dependency, inherited) {
                blocked = true;
                reasons.push(reason);
            }
        }
    }
    for gate in data.open_gates.get(&task.id).into_iter().flatten() {
        blocked = true;
        reasons.push(ReadinessReason {
            gate: Some(gate.clone()),
            ..ReadinessReason::new(
                ReadinessReasonKind::OpenGate,
                format!("gate `{}` waits {}", gate.id, gate.condition()),
            )
        });
    }
    reasons.extend(invalid_edge_reasons(data, task.id));
    Ok(PlanningReadiness { ready: reasons.is_empty(), blocked, reasons })
}

/// Describe each stored blocker rule violation that involves one of the
/// task's own blocker edges, once per violation, in the same words `check`
/// uses for it. `task` and `blocker` name the task's own edge in the
/// violation.
fn invalid_edge_reasons(data: &PlanningData, task_id: TaskId) -> Vec<ReadinessReason> {
    data.violations_by_task
        .get(&task_id)
        .into_iter()
        .flatten()
        .map(|&index| {
            let violation = &data.violations[index];
            let reason = ReadinessReason::new(ReadinessReasonKind::InvalidEdge, violation.to_string());
            match violation {
                // `task` holds the edge to the next step on the cycle, so
                // removing `task` -> `blocker` breaks it. A self-loop names
                // the task twice.
                EdgeViolation::Cycle { path } => ReadinessReason {
                    task: Some(task_id),
                    blocker: path
                        .iter()
                        .position(|&id| id == task_id)
                        .and_then(|index| path.get(index + 1).copied()),
                    path: path.clone(),
                    ..reason
                },
                EdgeViolation::Ancestry { edge, chain, .. } => ReadinessReason {
                    task: Some(edge.task_id),
                    blocker: Some(edge.blocker_id),
                    path: chain.clone(),
                    ..reason
                },
            }
        })
        .collect()
}

/// Explain why one blocker edge holds a task back, or return `None` once the
/// blocker has completed.
///
/// `ancestor` names the parent task that owns the edge when the blocker is
/// inherited. A cancelled blocker will never complete, so its reason asks for
/// the dependency to be removed or replaced instead of waited on.
fn blocker_reason(
    data: &PlanningData, dependency: &TaskDependency, ancestor: Option<&PlanningTask>,
) -> Option<ReadinessReason> {
    let subject = ancestor.map_or_else(
        || "blocked by".to_owned(),
        |ancestor| format!("ancestor `{}` is blocked by", ancestor.id),
    );
    let (kind, message) = match data.tasks.get(&dependency.blocker_id) {
        None => (
            ReadinessReasonKind::MissingBlocker,
            format!("{subject} missing task `{}`", dependency.blocker_id),
        ),
        Some(blocker) => match blocker.status {
            TaskStatus::Completed => return None,
            TaskStatus::Cancelled => (
                ReadinessReasonKind::CancelledBlocker,
                format!("{subject} cancelled `{}`; remove or replace the dependency", blocker.id),
            ),
            status if ancestor.is_some() => (
                ReadinessReasonKind::InheritedBlocker,
                format!("{subject} `{}` ({})", blocker.id, status.as_str()),
            ),
            status => (
                ReadinessReasonKind::DirectBlocker,
                format!("{subject} `{}` ({})", blocker.id, status.as_str()),
            ),
        },
    };
    Some(ReadinessReason {
        ancestor: ancestor.map(|ancestor| ancestor.id),
        blocker: Some(dependency.blocker_id),
        ..ReadinessReason::new(kind, message)
    })
}

fn parent_chain(data: &PlanningData, id: TaskId) -> Result<Vec<PlanningTask>> {
    let mut chain = Vec::new();
    let mut seen = HashSet::new();
    let mut current = data
        .tasks
        .get(&id)
        .ok_or_else(|| StorageError::PlanningTaskNotFound { id: id.to_string() })?
        .parent_id;
    while let Some(parent_id) = current {
        if !seen.insert(parent_id) || parent_id == id {
            return Err(StorageError::InvalidPlanningTask(DomainError::ParentCycle {
                task: id.to_string(),
                parent: parent_id.to_string(),
            }));
        }
        let parent = data
            .tasks
            .get(&parent_id)
            .ok_or_else(|| StorageError::PlanningTaskNotFound { id: parent_id.to_string() })?;
        chain.push(parent.clone());
        current = parent.parent_id;
    }
    chain.reverse();
    Ok(chain)
}

/// List the blockers that `holder_id` holds directly, each tagged with
/// `ancestor`: `None` for the inspected task's own blockers, or the holder's
/// id when it is an ancestor of the inspected task.
fn blocker_views(data: &PlanningData, holder_id: TaskId, ancestor: Option<TaskId>) -> Result<Vec<PlanningBlockerView>> {
    data.dependencies
        .iter()
        .filter(|dependency| dependency.task_id == holder_id)
        .map(|dependency| {
            let task = data
                .tasks
                .get(&dependency.blocker_id)
                .cloned()
                .ok_or_else(|| StorageError::PlanningTaskNotFound { id: dependency.blocker_id.to_string() })?;
            Ok(PlanningBlockerView {
                completed: task.status == TaskStatus::Completed,
                evidence: task.evidence.clone(),
                task,
                ancestor,
            })
        })
        .collect()
}

fn dependent_tasks(data: &PlanningData, id: TaskId) -> Vec<PlanningTask> {
    let mut result = data
        .dependencies
        .iter()
        .filter(|dependency| dependency.blocker_id == id)
        .filter_map(|dependency| data.tasks.get(&dependency.task_id).cloned())
        .collect::<Vec<_>>();
    result.sort_by_key(|task| task.id);
    result
}

fn child_tasks(data: &PlanningData, id: TaskId) -> Vec<PlanningTask> {
    let mut result = data
        .children
        .get(&id)
        .into_iter()
        .flatten()
        .filter_map(|child| data.tasks.get(child).cloned())
        .collect::<Vec<_>>();
    result.sort_by(|left, right| left.position.cmp(&right.position).then_with(|| left.id.cmp(&right.id)));
    result
}

fn ensure_project(connection: &Connection, project_id: ProjectId) -> Result<()> {
    super::ensure_project(connection, project_id)
}

fn ensure_project_task(task: &PlanningTask, project_id: ProjectId) -> Result<()> {
    if task.project_id == project_id {
        Ok(())
    } else {
        Err(StorageError::InvalidPlanningTask(DomainError::DifferentProject {
            entity: "task".to_owned(),
            id: task.id.to_string(),
            related: task.project_id.to_string(),
        }))
    }
}

fn contradictory(task: &PlanningTask, relationship: &str) -> StorageError {
    StorageError::InvalidPlanningTask(DomainError::ContradictoryAncestry {
        task: task.id.to_string(),
        relationship: relationship.to_owned(),
    })
}

fn contradictory_ids(task: TaskId, relationship: &str) -> StorageError {
    StorageError::InvalidPlanningTask(DomainError::ContradictoryAncestry {
        task: task.to_string(),
        relationship: relationship.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{Database, PlanningTaskCreate};
    use arcl_core::domain::ContainerAction;

    fn task(database: &mut Database, project_id: ProjectId, title: &str) -> PlanningTask {
        database
            .create_planning_task(PlanningTaskCreate {
                project_id,
                spec_id: None,
                plan_id: None,
                phase_id: None,
                parent_id: None,
                title: title.to_owned(),
                body: format!("{title} body"),
                priority: TaskPriority::Normal,
                position: 0,
            })
            .expect("task creates")
    }

    fn child(database: &mut Database, project_id: ProjectId, parent_id: TaskId, title: &str) -> PlanningTask {
        database
            .create_planning_task(PlanningTaskCreate {
                project_id,
                spec_id: None,
                plan_id: None,
                phase_id: None,
                parent_id: Some(parent_id),
                title: title.to_owned(),
                body: String::new(),
                priority: TaskPriority::Normal,
                position: 0,
            })
            .expect("child creates")
    }

    /// Store a blocker edge without the validator, as a database edited by
    /// hand can hold an edge that `add_planning_dependency` refuses.
    fn insert_raw_edge(database: &Database, project_id: ProjectId, task_id: TaskId, blocker_id: TaskId) {
        database
            .connection()
            .execute(
                "INSERT INTO planning_task_dependencies (project_id, task_id, blocker_id) VALUES (?1, ?2, ?3)",
                params![project_id.to_string(), task_id.to_string(), blocker_id.to_string()],
            )
            .expect("dependency inserts");
    }

    #[test]
    fn invalid_edge_cycle_is_reported_once_by_check_and_by_readiness() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let a = task(&mut database, project.id, "a");
        let b = task(&mut database, project.id, "b");
        let c = task(&mut database, project.id, "c");
        insert_raw_edge(&database, project.id, a.id, b.id);
        insert_raw_edge(&database, project.id, b.id, c.id);
        insert_raw_edge(&database, project.id, c.id, a.id);
        // With its blocker completed, `c` would be ready but for the cycle.
        database
            .connection()
            .execute(
                "UPDATE planning_tasks SET status = 'completed' WHERE id = ?1",
                [a.id.to_string()],
            )
            .expect("status updates");
        // The report starts at the smallest task ID, whatever the edge order.
        let order = [a.id, b.id, c.id];
        let start = (0..3).min_by_key(|&index| order[index]).expect("three tasks");
        let path: Vec<String> = (0..=3).map(|step| order[(start + step) % 3].to_string()).collect();
        let cycle = format!("blocker cycle: {}; each task is blocked by the next", path.join(" -> "));

        let report = database.check().expect("check runs");
        assert!(!report.valid);
        assert_eq!(report.errors, vec![cycle.clone()]);

        let view = database.planning_task_view(c.id).expect("view reads");
        assert!(!view.readiness.ready);
        assert_eq!(view.readiness.messages(), vec![cycle.clone()]);
        let reason = &view.readiness.reasons[0];
        assert_eq!(reason.kind, ReadinessReasonKind::InvalidEdge);
        let ids: Vec<String> = reason.path.iter().map(ToString::to_string).collect();
        assert_eq!(ids, path);
        // Each task on the cycle names its own edge to the next step.
        assert_eq!((reason.task, reason.blocker), (Some(c.id), Some(a.id)));
        let view = database.planning_task_view(b.id).expect("view reads");
        assert!(view.readiness.messages().contains(&cycle));
        let reason = view
            .readiness
            .reasons
            .iter()
            .find(|reason| reason.kind == ReadinessReasonKind::InvalidEdge)
            .expect("invalid edge reason");
        assert_eq!((reason.task, reason.blocker), (Some(b.id), Some(c.id)));
        let ready = database.ready_planning_tasks().expect("ready tasks query");
        assert!(!ready.iter().any(|value| value.id == c.id));
    }

    #[test]
    fn invalid_self_loop_is_a_one_task_cycle_that_can_be_removed() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let a = task(&mut database, project.id, "a");
        let other = task(&mut database, project.id, "other");
        insert_raw_edge(&database, project.id, a.id, a.id);
        let cycle = format!("blocker cycle: {0} -> {0}; each task is blocked by the next", a.id);

        let report = database.check().expect("check runs");
        assert!(!report.valid);
        assert_eq!(report.errors, vec![cycle.clone()]);
        let view = database.planning_task_view(a.id).expect("view reads");
        assert!(!view.readiness.ready);
        assert!(
            view.readiness.messages().contains(&cycle),
            "{:?}",
            view.readiness.messages()
        );
        let reason = view
            .readiness
            .reasons
            .iter()
            .find(|reason| reason.kind == ReadinessReasonKind::InvalidEdge)
            .expect("invalid edge reason");
        assert_eq!((reason.task, reason.blocker), (Some(a.id), Some(a.id)));
        assert_eq!(reason.path, vec![a.id, a.id]);
        let ready = database.ready_planning_tasks().expect("ready tasks query");
        assert_eq!(ready.iter().map(|value| value.id).collect::<Vec<_>>(), vec![other.id]);

        // Writes still refuse creating one, and removing it clears the report.
        assert!(database.add_planning_dependency(other.id, other.id).is_err());
        database
            .remove_planning_dependency(a.id, a.id)
            .expect("self-loop removes");
        assert!(database.check().expect("check runs").valid);
        assert!(database.planning_task_view(a.id).expect("view reads").readiness.ready);
    }

    #[test]
    fn invalid_edge_to_an_ancestor_is_reported_by_check_and_by_readiness() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let root = task(&mut database, project.id, "root");
        let middle = child(&mut database, project.id, root.id, "middle");
        let leaf = child(&mut database, project.id, middle.id, "leaf");
        insert_raw_edge(&database, project.id, leaf.id, root.id);
        let ancestry = format!(
            "task `{}` is blocked by `{}`, its ancestor; parent chain: {} -> {} -> {}",
            leaf.id, root.id, leaf.id, middle.id, root.id
        );

        let report = database.check().expect("check runs");
        assert!(!report.valid);
        assert_eq!(report.errors, vec![ancestry.clone()]);

        let view = database.planning_task_view(leaf.id).expect("view reads");
        assert!(!view.readiness.ready);
        assert!(
            view.readiness.messages().contains(&ancestry),
            "{:?}",
            view.readiness.messages()
        );
        let reason = view
            .readiness
            .reasons
            .iter()
            .find(|reason| reason.kind == ReadinessReasonKind::InvalidEdge)
            .expect("invalid edge reason");
        assert_eq!((reason.task, reason.blocker), (Some(leaf.id), Some(root.id)));
        assert_eq!(reason.path, vec![leaf.id, middle.id, root.id]);
    }

    #[test]
    fn invalid_edge_to_a_descendant_is_a_reason_on_the_parent() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let parent = task(&mut database, project.id, "parent");
        let leaf = child(&mut database, project.id, parent.id, "leaf");
        insert_raw_edge(&database, project.id, parent.id, leaf.id);
        let descendant = format!(
            "task `{}` is blocked by `{}`, its descendant; parent chain: {} -> {}",
            parent.id, leaf.id, leaf.id, parent.id
        );

        let report = database.check().expect("check runs");
        assert_eq!(report.errors, vec![descendant.clone()]);

        let view = database.planning_task_view(parent.id).expect("parent view reads");
        assert!(!view.readiness.ready);
        assert!(
            view.readiness.messages().contains(&descendant),
            "{:?}",
            view.readiness.messages()
        );
        let view = database.planning_task_view(leaf.id).expect("leaf view reads");
        assert!(!view.readiness.messages().contains(&descendant));
    }

    #[test]
    fn readiness_inherits_ancestor_blockers_until_they_complete() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let blocker = task(&mut database, project.id, "blocker");
        let grandparent = task(&mut database, project.id, "grandparent");
        let parent = child(&mut database, project.id, grandparent.id, "parent");
        let leaf = child(&mut database, project.id, parent.id, "leaf");
        database
            .add_planning_dependency(grandparent.id, blocker.id)
            .expect("dependency creates");

        let view = database.planning_task_view(leaf.id).expect("leaf view reads");
        assert!(!view.readiness.ready);
        assert!(view.readiness.blocked);
        assert_eq!(
            view.readiness.messages(),
            vec![format!(
                "ancestor `{}` is blocked by `{}` (pending)",
                grandparent.id, blocker.id
            )]
        );
        let ready = database.ready_planning_tasks().expect("ready tasks query");
        assert!(!ready.iter().any(|value| value.id == leaf.id));

        database
            .transition_planning_task(blocker.id, TaskAction::Complete, false)
            .expect("blocker completes");
        let view = database.planning_task_view(leaf.id).expect("leaf view reads");
        assert!(view.readiness.ready);
        assert!(!view.readiness.blocked);
        let ready = database.ready_planning_tasks().expect("ready tasks query");
        assert!(ready.iter().any(|value| value.id == leaf.id));
    }

    #[test]
    fn work_matches_each_task_view_and_lists_every_phase() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let spec = database
            .create_spec("Spec".to_owned(), "Spec body".to_owned(), "Acceptance".to_owned())
            .expect("spec creates");
        let plan = database
            .create_plan(spec.id, "Plan".to_owned(), "Plan body".to_owned())
            .expect("plan creates");
        let phase = database
            .create_phase(plan.id, "Phase".to_owned(), String::new(), 0)
            .expect("phase creates");
        let empty = database
            .create_phase(plan.id, "Empty".to_owned(), String::new(), 1)
            .expect("phase creates");
        let parent = database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: None,
                plan_id: None,
                phase_id: Some(phase.id),
                parent_id: None,
                title: "parent".to_owned(),
                body: String::new(),
                priority: TaskPriority::High,
                position: 1,
            })
            .expect("phase task creates");
        let leaf = child(&mut database, project.id, parent.id, "leaf");
        let blocker = task(&mut database, project.id, "blocker");
        database
            .add_planning_dependency(leaf.id, blocker.id)
            .expect("dependency creates");

        let work = database.planning_work().expect("work reads");
        let ids: Vec<TaskId> = work.tasks.iter().map(|item| item.task.id).collect();
        let mut expected = vec![leaf.id, blocker.id];
        expected.sort();
        expected.push(parent.id);
        assert_eq!(ids, expected, "ordered by position, then ID");
        for item in &work.tasks {
            let view = database.planning_task_view(item.task.id).expect("view reads");
            assert_eq!(item.task, view.task);
            assert_eq!(item.readiness, view.readiness);
            assert_eq!(item.phase_id, view.phase.map(|phase| phase.id));
        }
        let leaf_item = work.tasks.iter().find(|item| item.task.id == leaf.id).expect("leaf");
        assert_eq!(
            leaf_item.phase_id,
            Some(phase.id),
            "the leaf inherits its parent's phase"
        );
        assert!(leaf_item.readiness.blocked);
        let phases: Vec<PhaseId> = work.phases.iter().map(|phase| phase.id).collect();
        assert_eq!(phases, vec![phase.id, empty.id]);
    }

    #[test]
    fn readiness_reports_cancelled_blockers_as_decisions() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let cancelled = task(&mut database, project.id, "cancelled");
        let pending = task(&mut database, project.id, "pending");
        let parent = task(&mut database, project.id, "parent");
        let leaf = child(&mut database, project.id, parent.id, "leaf");
        database
            .add_planning_dependency(parent.id, cancelled.id)
            .expect("inherited dependency creates");
        database
            .add_planning_dependency(leaf.id, cancelled.id)
            .expect("direct cancelled dependency creates");
        database
            .add_planning_dependency(leaf.id, pending.id)
            .expect("direct pending dependency creates");
        database
            .transition_planning_task(cancelled.id, TaskAction::Cancel, false)
            .expect("blocker cancels");

        let view = database.planning_task_view(leaf.id).expect("leaf view reads");
        assert!(view.readiness.blocked);
        let reasons = view.readiness.messages();
        assert!(reasons.contains(&format!(
            "ancestor `{}` is blocked by cancelled `{}`; remove or replace the dependency",
            parent.id, cancelled.id
        )));
        assert!(reasons.contains(&format!(
            "blocked by cancelled `{}`; remove or replace the dependency",
            cancelled.id
        )));
        assert!(reasons.contains(&format!("blocked by `{}` (pending)", pending.id)));
    }

    /// The kind and the named record IDs of each reason, without the text.
    fn kinds_and_ids(readiness: &PlanningReadiness) -> Vec<(ReadinessReasonKind, Option<TaskId>, Option<TaskId>)> {
        readiness
            .reasons
            .iter()
            .map(|reason| (reason.kind, reason.ancestor, reason.blocker))
            .collect()
    }

    #[test]
    fn readiness_reasons_carry_kinds_and_record_ids() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let cancelled = task(&mut database, project.id, "cancelled");
        let pending = task(&mut database, project.id, "pending");
        let parent = task(&mut database, project.id, "parent");
        let leaf = child(&mut database, project.id, parent.id, "leaf");
        for (holder, blocker) in [
            (parent.id, cancelled.id),
            (parent.id, pending.id),
            (leaf.id, pending.id),
        ] {
            database
                .add_planning_dependency(holder, blocker)
                .expect("dependency creates");
        }
        database
            .transition_planning_task(cancelled.id, TaskAction::Cancel, false)
            .expect("blocker cancels");

        let view = database.planning_task_view(leaf.id).expect("leaf view reads");
        let reasons = kinds_and_ids(&view.readiness);
        let mut expected = vec![
            (ReadinessReasonKind::DirectBlocker, None, Some(pending.id)),
            (
                ReadinessReasonKind::CancelledBlocker,
                Some(parent.id),
                Some(cancelled.id),
            ),
            (ReadinessReasonKind::InheritedBlocker, Some(parent.id), Some(pending.id)),
        ];
        // The parent's two blockers come in ID order, and IDs made in the same
        // millisecond are not ordered by creation, so order the expected
        // reasons by blocker ID too.
        expected[1..].sort_by_key(|reason| reason.2);
        assert_eq!(reasons, expected);

        database
            .transition_planning_task(parent.id, TaskAction::Park, false)
            .expect("parent parks");
        let view = database.planning_task_view(parent.id).expect("parent view reads");
        let own: Vec<_> = view
            .readiness
            .reasons
            .iter()
            .take(2)
            .map(|reason| (reason.kind, reason.task))
            .collect();
        assert_eq!(
            own,
            vec![
                (ReadinessReasonKind::Status, Some(parent.id)),
                (ReadinessReasonKind::HasChildren, Some(parent.id)),
            ]
        );
        let view = database.planning_task_view(leaf.id).expect("leaf view reads");
        assert_eq!(
            kinds_and_ids(&view.readiness)[0],
            (ReadinessReasonKind::ClosedAncestor, Some(parent.id), None)
        );
    }

    #[test]
    fn readiness_names_a_missing_blocker_by_id() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let blocker = task(&mut database, project.id, "blocker");
        let leaf = task(&mut database, project.id, "leaf");
        database
            .add_planning_dependency(leaf.id, blocker.id)
            .expect("dependency creates");
        // Foreign keys keep a dangling edge out of the database, so drop the
        // blocker from the loaded records instead.
        let mut data = load_data(database.connection(), project.id).expect("data loads");
        data.tasks.remove(&blocker.id);
        let ancestry = effective_ancestry(&data, leaf.id, &mut HashSet::new()).expect("ancestry resolves");

        let readiness = readiness(&data, &leaf, &ancestry).expect("readiness computes");
        assert_eq!(
            kinds_and_ids(&readiness),
            vec![(ReadinessReasonKind::MissingBlocker, None, Some(blocker.id))]
        );
        assert_eq!(
            readiness.messages(),
            vec![format!("blocked by missing task `{}`", blocker.id)]
        );
    }

    #[test]
    fn readiness_names_the_ancestor_holding_a_missing_blocker() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let blocker = task(&mut database, project.id, "blocker");
        let parent = task(&mut database, project.id, "parent");
        let leaf = child(&mut database, project.id, parent.id, "leaf");
        database
            .add_planning_dependency(parent.id, blocker.id)
            .expect("dependency creates");
        let mut data = load_data(database.connection(), project.id).expect("data loads");
        data.tasks.remove(&blocker.id);
        let leaf = data.tasks[&leaf.id].clone();
        let ancestry = effective_ancestry(&data, leaf.id, &mut HashSet::new()).expect("ancestry resolves");

        let readiness = readiness(&data, &leaf, &ancestry).expect("readiness computes");
        assert_eq!(
            kinds_and_ids(&readiness),
            vec![(ReadinessReasonKind::MissingBlocker, Some(parent.id), Some(blocker.id))]
        );
        assert_eq!(
            readiness.messages(),
            vec![format!(
                "ancestor `{}` is blocked by missing task `{}`",
                parent.id, blocker.id
            )]
        );
    }

    #[test]
    fn readiness_ignores_an_ancestor_blocked_by_the_task_itself() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let parent = task(&mut database, project.id, "parent");
        let leaf = child(&mut database, project.id, parent.id, "leaf");
        // `add_planning_dependency` refuses this edge, so insert it directly.
        database
            .connection()
            .execute(
                "INSERT INTO planning_task_dependencies (project_id, task_id, blocker_id) VALUES (?1, ?2, ?3)",
                rusqlite::params![project.id.to_string(), parent.id.to_string(), leaf.id.to_string()],
            )
            .expect("dependency inserts");

        let view = database.planning_task_view(leaf.id).expect("leaf view reads");
        assert!(view.readiness.ready, "{:?}", view.readiness.messages());
        assert!(!view.readiness.blocked);
        assert!(view.inherited_blockers.is_empty());
        let ready = database.ready_planning_tasks().expect("ready tasks query");
        assert!(ready.iter().any(|value| value.id == leaf.id));
    }

    #[test]
    fn inherited_blockers_list_the_nearest_ancestor_first() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let far = task(&mut database, project.id, "far blocker");
        let near = task(&mut database, project.id, "near blocker");
        let own = task(&mut database, project.id, "own blocker");
        let grandparent = task(&mut database, project.id, "grandparent");
        let parent = child(&mut database, project.id, grandparent.id, "parent");
        let leaf = child(&mut database, project.id, parent.id, "leaf");
        database
            .add_planning_dependency(grandparent.id, far.id)
            .expect("grandparent dependency creates");
        database
            .add_planning_dependency(parent.id, near.id)
            .expect("parent dependency creates");
        database
            .add_planning_dependency(leaf.id, own.id)
            .expect("leaf dependency creates");

        let view = database.planning_task_view(leaf.id).expect("leaf view reads");
        assert_eq!(
            view.readiness.messages(),
            vec![
                format!("blocked by `{}` (pending)", own.id),
                format!("ancestor `{}` is blocked by `{}` (pending)", parent.id, near.id),
                format!("ancestor `{}` is blocked by `{}` (pending)", grandparent.id, far.id),
            ]
        );
        let direct: Vec<_> = view
            .blockers
            .iter()
            .map(|blocker| (blocker.task.id, blocker.ancestor))
            .collect();
        assert_eq!(direct, vec![(own.id, None)]);
        let inherited: Vec<_> = view
            .inherited_blockers
            .iter()
            .map(|blocker| (blocker.task.id, blocker.ancestor))
            .collect();
        assert_eq!(
            inherited,
            vec![(near.id, Some(parent.id)), (far.id, Some(grandparent.id))]
        );
    }

    #[test]
    fn completion_evidence_lists_each_blocker_once_preferring_the_direct_edge() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let blocker = task(&mut database, project.id, "blocker");
        let parent = task(&mut database, project.id, "parent");
        let leaf = child(&mut database, project.id, parent.id, "leaf");
        database
            .add_planning_dependency(parent.id, blocker.id)
            .expect("inherited dependency creates");
        database
            .add_planning_dependency(leaf.id, blocker.id)
            .expect("direct dependency creates");
        database
            .complete_planning_task(blocker.id, OpenSubtasks::Refuse, Some("shipped".to_owned()))
            .expect("blocker completes");

        let context = database.planning_context(leaf.id).expect("context reads");
        assert_eq!(context.blockers.len(), 1);
        assert_eq!(context.inherited_blockers.len(), 1);
        assert_eq!(context.completion_evidence.len(), 1);
        assert_eq!(context.completion_evidence[0].task.id, blocker.id);
        assert_eq!(context.completion_evidence[0].ancestor, None);
        assert_eq!(context.completion_evidence[0].evidence, "shipped");
    }

    #[test]
    fn ready_tasks_cover_project_spec_plan_phase_and_parent_placements() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let spec = database
            .create_spec("Spec".to_owned(), "Spec body".to_owned(), "Acceptance".to_owned())
            .expect("spec creates");
        let plan = database
            .create_plan(spec.id, "Plan".to_owned(), "Plan body".to_owned())
            .expect("plan creates");
        let phase = database
            .create_phase(plan.id, "Phase".to_owned(), "Phase body".to_owned(), 0)
            .expect("phase creates");
        let project_task = task(&mut database, project.id, "project");
        let spec_task = database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: Some(spec.id),
                plan_id: None,
                phase_id: None,
                parent_id: None,
                title: "spec".to_owned(),
                body: String::new(),
                priority: TaskPriority::High,
                position: 1,
            })
            .expect("spec task creates");
        let plan_task = database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: None,
                plan_id: Some(plan.id),
                phase_id: None,
                parent_id: None,
                title: "plan".to_owned(),
                body: String::new(),
                priority: TaskPriority::Normal,
                position: 2,
            })
            .expect("plan task creates");
        let phase_task = database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: None,
                plan_id: None,
                phase_id: Some(phase.id),
                parent_id: None,
                title: "phase".to_owned(),
                body: String::new(),
                priority: TaskPriority::Low,
                position: 3,
            })
            .expect("phase task creates");
        let parent = task(&mut database, project.id, "parent");
        let child = database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: None,
                plan_id: None,
                phase_id: None,
                parent_id: Some(parent.id),
                title: "child".to_owned(),
                body: String::new(),
                priority: TaskPriority::Critical,
                position: 0,
            })
            .expect("child creates");

        let ready = database.ready_planning_tasks().expect("ready tasks query");
        assert_eq!(ready.len(), 5);
        assert!(ready.iter().any(|value| value.id == project_task.id));
        assert!(ready.iter().any(|value| value.id == spec_task.id));
        assert!(ready.iter().any(|value| value.id == plan_task.id));
        assert!(ready.iter().any(|value| value.id == phase_task.id));
        assert!(ready.iter().any(|value| value.id == child.id));
        assert!(!ready.iter().any(|value| value.id == parent.id));
    }

    #[test]
    fn planning_context_explains_containers_ancestors_and_blockers() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let spec = database
            .create_spec("Spec".to_owned(), "spec markdown".to_owned(), "criteria".to_owned())
            .expect("spec creates");
        let plan = database
            .create_plan(spec.id, "Plan".to_owned(), "plan markdown".to_owned())
            .expect("plan creates");
        let phase = database
            .create_phase(plan.id, "Phase".to_owned(), "phase markdown".to_owned(), 0)
            .expect("phase creates");
        let blocker = database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: Some(spec.id),
                plan_id: Some(plan.id),
                phase_id: Some(phase.id),
                parent_id: None,
                title: "Blocker".to_owned(),
                body: String::new(),
                priority: TaskPriority::Normal,
                position: 0,
            })
            .expect("blocker creates");
        let task = database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: Some(spec.id),
                plan_id: Some(plan.id),
                phase_id: Some(phase.id),
                parent_id: None,
                title: "Task".to_owned(),
                body: "task markdown".to_owned(),
                priority: TaskPriority::Normal,
                position: 1,
            })
            .expect("task creates");
        database
            .add_planning_dependency(task.id, blocker.id)
            .expect("dependency creates");

        let view = database.planning_task_view(task.id).expect("task view reads");
        assert!(!view.readiness.ready);
        assert!(view.readiness.blocked);
        assert!(
            view.readiness
                .reasons
                .iter()
                .any(|reason| reason.message.contains("blocked by"))
        );
        assert_eq!(view.spec.as_ref().expect("spec context").body, "spec markdown");
        assert_eq!(view.plan.as_ref().expect("plan context").body, "plan markdown");
        assert_eq!(view.phase.as_ref().expect("phase context").body, "phase markdown");

        database
            .transition_planning_task(blocker.id, TaskAction::Complete, false)
            .expect("blocker completes");
        database
            .complete_planning_task(task.id, OpenSubtasks::Refuse, Some("verified".to_owned()))
            .expect("task completes");
        let context = database.planning_context(task.id).expect("context reads");
        assert_eq!(context.task.evidence, "verified");
        assert_eq!(context.blockers[0].task.id, blocker.id);
    }

    #[test]
    fn connected_execution_rejects_parent_cycles_before_writing() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let parent = task(&mut database, project.id, "parent");
        let child = database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: None,
                plan_id: None,
                phase_id: None,
                parent_id: Some(parent.id),
                title: "child".to_owned(),
                body: String::new(),
                priority: TaskPriority::Normal,
                position: 0,
            })
            .expect("child creates");
        let before = database.planning_tasks().expect("tasks snapshot");
        let error = database
            .update_planning_task(
                parent.id,
                crate::storage::PlanningTaskUpdate { parent_id: Some(Some(child.id)), ..Default::default() },
            )
            .expect_err("parent cycle rejects");
        assert!(matches!(
            error,
            StorageError::InvalidPlanningTask(DomainError::ParentCycle { .. })
        ));
        assert_eq!(database.planning_tasks().expect("tasks after failure"), before);
    }

    #[test]
    fn connected_execution_keeps_invalid_writes_atomic() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let spec = database
            .create_spec("Spec".to_owned(), String::new(), String::new())
            .expect("spec creates");
        let plan = database
            .create_plan(spec.id, "Plan".to_owned(), String::new())
            .expect("plan creates");
        let task = database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: Some(spec.id),
                plan_id: Some(plan.id),
                phase_id: None,
                parent_id: None,
                title: "Task".to_owned(),
                body: String::new(),
                priority: TaskPriority::Normal,
                position: 0,
            })
            .expect("task creates");
        database
            .transition_plan(plan.id, ContainerAction::Complete, true)
            .expect("plan force completes");
        let error = database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: None,
                plan_id: Some(plan.id),
                phase_id: None,
                parent_id: None,
                title: "Invalid".to_owned(),
                body: String::new(),
                priority: TaskPriority::Normal,
                position: 1,
            })
            .expect_err("closed plan rejects placement");
        assert!(matches!(
            error,
            StorageError::InvalidPlanningTask(DomainError::InvalidContainerState { .. })
        ));
        assert_eq!(database.planning_tasks().expect("tasks remain").len(), 1);

        // The closed plan makes the task not ready, so starting it takes force.
        database
            .start_planning_task(task.id, true)
            .expect("task can still be explicitly started after a forced container close");
        let handoff = database
            .handoff_planning_task(task.id, "resume".to_owned())
            .expect("handoff parks atomically");
        assert_eq!(handoff.status, TaskStatus::Parked);
        assert_eq!(handoff.handoff, "resume");
    }

    #[test]
    fn start_refuses_unready_task_unless_forced() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let blocker = task(&mut database, project.id, "Blocker");
        let blocked = task(&mut database, project.id, "Blocked");
        database
            .add_planning_dependency(blocked.id, blocker.id)
            .expect("dependency creates");

        let error = database
            .start_planning_task(blocked.id, false)
            .expect_err("blocked task is refused");
        match error {
            StorageError::InvalidPlanningTask(DomainError::NotReady { id, reasons }) => {
                assert_eq!(id, blocked.id.to_string());
                assert_eq!(reasons, vec![format!("blocked by `{}` (pending)", blocker.id)]);
            }
            other => panic!("unexpected error: {other:?}"),
        }
        let unchanged = database
            .planning_task(blocked.id)
            .expect("task reads")
            .expect("task exists");
        assert_eq!(unchanged.status, TaskStatus::Pending);

        let started = database
            .start_planning_task(blocked.id, true)
            .expect("forced start succeeds");
        assert_eq!(started.status, TaskStatus::InProgress);
    }

    #[test]
    fn start_repeats_on_in_progress_task_with_or_without_force() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let blocker = task(&mut database, project.id, "Blocker");
        let blocked = task(&mut database, project.id, "Blocked");
        database
            .add_planning_dependency(blocked.id, blocker.id)
            .expect("dependency creates");
        database
            .start_planning_task(blocked.id, true)
            .expect("forced start succeeds");

        for force in [false, true] {
            let repeated = database
                .start_planning_task(blocked.id, force)
                .expect("repeat start is a no-op");
            assert_eq!(repeated.status, TaskStatus::InProgress);
        }
    }

    #[test]
    fn start_skips_only_the_child_task_reason() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let parent = task(&mut database, project.id, "Parent");
        let child = database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: None,
                plan_id: None,
                phase_id: None,
                parent_id: Some(parent.id),
                title: "Child".to_owned(),
                body: String::new(),
                priority: TaskPriority::Normal,
                position: 0,
            })
            .expect("child creates");
        assert!(child.parent_id.is_some());
        let started = database
            .start_planning_task(parent.id, false)
            .expect("parent with children starts");
        assert_eq!(started.status, TaskStatus::InProgress);

        let blocker = task(&mut database, project.id, "Blocker");
        let blocked_parent = task(&mut database, project.id, "Blocked parent");
        database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: None,
                plan_id: None,
                phase_id: None,
                parent_id: Some(blocked_parent.id),
                title: "Blocked child".to_owned(),
                body: String::new(),
                priority: TaskPriority::Normal,
                position: 0,
            })
            .expect("child creates");
        database
            .add_planning_dependency(blocked_parent.id, blocker.id)
            .expect("dependency creates");
        let error = database
            .start_planning_task(blocked_parent.id, false)
            .expect_err("blocked parent is refused");
        match error {
            StorageError::InvalidPlanningTask(DomainError::NotReady { reasons, .. }) => {
                assert_eq!(reasons, vec![format!("blocked by `{}` (pending)", blocker.id)]);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    /// Assert that a start without force is refused with `reasons` and writes
    /// nothing, then that a forced start succeeds.
    fn assert_start_needs_force(database: &mut Database, id: TaskId, reasons: Vec<String>) {
        match database.start_planning_task(id, false) {
            Err(StorageError::InvalidPlanningTask(DomainError::NotReady { reasons: actual, .. })) => {
                assert_eq!(actual, reasons);
            }
            other => panic!("unexpected result: {other:?}"),
        }
        let unchanged = database.planning_task(id).expect("task reads").expect("task exists");
        assert_eq!(unchanged.status, TaskStatus::Pending);
        let started = database.start_planning_task(id, true).expect("forced start succeeds");
        assert_eq!(started.status, TaskStatus::InProgress);
    }

    #[test]
    fn start_refuses_a_task_under_a_parked_ancestor_unless_forced() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let parent = task(&mut database, project.id, "Parent");
        let child = database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: None,
                plan_id: None,
                phase_id: None,
                parent_id: Some(parent.id),
                title: "Child".to_owned(),
                body: String::new(),
                priority: TaskPriority::Normal,
                position: 0,
            })
            .expect("child creates");
        database
            .transition_planning_task(parent.id, TaskAction::Park, false)
            .expect("parent parks");

        assert_start_needs_force(
            &mut database,
            child.id,
            vec![format!("ancestor `{}` is `parked`", parent.id)],
        );
    }

    #[test]
    fn start_refuses_a_task_in_a_closed_plan_unless_forced() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let spec = database
            .create_spec("Spec".to_owned(), String::new(), String::new())
            .expect("spec creates");
        let plan = database
            .create_plan(spec.id, "Plan".to_owned(), String::new())
            .expect("plan creates");
        let task = database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: None,
                plan_id: Some(plan.id),
                phase_id: None,
                parent_id: None,
                title: "Task".to_owned(),
                body: String::new(),
                priority: TaskPriority::Normal,
                position: 0,
            })
            .expect("task creates");
        database
            .transition_plan(plan.id, ContainerAction::Cancel, true)
            .expect("plan force cancels");

        assert_start_needs_force(
            &mut database,
            task.id,
            vec![format!("plan `{}` is `cancelled`", plan.id)],
        );
    }

    fn status(database: &Database, id: TaskId) -> TaskStatus {
        database
            .planning_task(id)
            .expect("task reads")
            .expect("task exists")
            .status
    }

    /// Tasks created in the same millisecond can get IDs out of creation order,
    /// so tests compare equal-position siblings as sorted lists.
    fn sorted(mut ids: Vec<TaskId>) -> Vec<TaskId> {
        ids.sort();
        ids
    }

    fn sorted_ids(tasks: &[PlanningTask]) -> Vec<TaskId> {
        sorted(tasks.iter().map(|task| task.id).collect())
    }

    #[test]
    fn complete_with_subtasks_completes_open_descendants_in_tree_order() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let parent = task(&mut database, project.id, "Parent");
        let started = child(&mut database, project.id, parent.id, "Started");
        let grandchild = child(&mut database, project.id, started.id, "Grandchild");
        let done = child(&mut database, project.id, parent.id, "Done");
        let dropped = child(&mut database, project.id, parent.id, "Dropped");
        let pending = child(&mut database, project.id, parent.id, "Pending");
        database
            .start_planning_task(started.id, false)
            .expect("parent of a subtask starts");
        database
            .complete_planning_task(done.id, OpenSubtasks::Refuse, Some("done earlier".to_owned()))
            .expect("subtask completes");
        database
            .transition_planning_task(dropped.id, TaskAction::Cancel, false)
            .expect("subtask cancels");

        let completion = database
            .complete_planning_task(parent.id, OpenSubtasks::Complete, Some("all of it".to_owned()))
            .expect("task completes with its subtasks");

        assert_eq!(completion.task.status, TaskStatus::Completed);
        assert_eq!(completion.task.evidence, "all of it");
        assert_eq!(
            sorted_ids(&completion.completed_subtasks),
            sorted(vec![started.id, grandchild.id, pending.id])
        );
        for id in [parent.id, started.id, grandchild.id, done.id, pending.id] {
            assert_eq!(status(&database, id), TaskStatus::Completed);
        }
        assert_eq!(status(&database, dropped.id), TaskStatus::Cancelled);
        let started = database
            .planning_task(started.id)
            .expect("task reads")
            .expect("task exists");
        assert_eq!(started.evidence, "");
        let done = database
            .planning_task(done.id)
            .expect("task reads")
            .expect("task exists");
        assert_eq!(done.evidence, "done earlier");
    }

    #[test]
    fn complete_with_subtasks_refuses_parked_subtasks_and_outside_blockers() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let outside = task(&mut database, project.id, "Outside");
        let dropped = task(&mut database, project.id, "Dropped outside");
        let grandparent = task(&mut database, project.id, "Grandparent");
        let parent = child(&mut database, project.id, grandparent.id, "Parent");
        let parked = child(&mut database, project.id, parent.id, "Parked");
        let blocked = child(&mut database, project.id, parent.id, "Blocked");
        database
            .add_planning_dependency(grandparent.id, outside.id)
            .expect("dependency creates");
        database
            .add_planning_dependency(blocked.id, dropped.id)
            .expect("dependency creates");
        database
            .transition_planning_task(dropped.id, TaskAction::Cancel, false)
            .expect("outside blocker cancels");
        database.start_planning_task(parked.id, true).expect("subtask starts");
        database
            .transition_planning_task(parked.id, TaskAction::Park, false)
            .expect("subtask parks");

        match database.complete_planning_task(parent.id, OpenSubtasks::Complete, Some("evidence".to_owned())) {
            Err(StorageError::InvalidPlanningTask(DomainError::SubtasksNotCompletable { id, reasons })) => {
                assert_eq!(id, parent.id.to_string());
                assert_eq!(
                    reasons,
                    vec![
                        format!("subtask `{}` is parked; unpark or cancel it", parked.id),
                        format!("`{}` is blocked by `{}` (pending)", grandparent.id, outside.id),
                        format!(
                            "`{}` is blocked by cancelled `{}`; remove or replace the dependency",
                            blocked.id, dropped.id
                        ),
                    ]
                );
            }
            other => panic!("unexpected result: {other:?}"),
        }
        assert_eq!(status(&database, parent.id), TaskStatus::Pending);
        assert_eq!(status(&database, parked.id), TaskStatus::Parked);
        assert_eq!(status(&database, blocked.id), TaskStatus::Pending);
        let parent = database
            .planning_task(parent.id)
            .expect("task reads")
            .expect("task exists");
        assert_eq!(parent.evidence, "");
    }

    #[test]
    fn complete_with_subtasks_ignores_blockers_inside_the_subtree() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let parent = task(&mut database, project.id, "Parent");
        let dropped = child(&mut database, project.id, parent.id, "Dropped");
        let after_dropped = child(&mut database, project.id, parent.id, "After dropped");
        let first = child(&mut database, project.id, parent.id, "First");
        let second = child(&mut database, project.id, parent.id, "Second");
        database
            .add_planning_dependency(after_dropped.id, dropped.id)
            .expect("dependency creates");
        database
            .add_planning_dependency(second.id, first.id)
            .expect("dependency creates");
        database
            .transition_planning_task(dropped.id, TaskAction::Cancel, false)
            .expect("subtask cancels");

        let completion = database
            .complete_planning_task(parent.id, OpenSubtasks::Complete, None)
            .expect("inside blockers do not count");

        assert_eq!(
            sorted_ids(&completion.completed_subtasks),
            sorted(vec![after_dropped.id, first.id, second.id])
        );
        assert_eq!(status(&database, dropped.id), TaskStatus::Cancelled);
        assert_eq!(status(&database, after_dropped.id), TaskStatus::Completed);
        assert_eq!(status(&database, second.id), TaskStatus::Completed);
    }

    #[test]
    fn complete_with_subtasks_refuses_blockers_held_by_completed_intermediates() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let outside = task(&mut database, project.id, "Outside");
        let parent = task(&mut database, project.id, "Parent");
        let intermediate = child(&mut database, project.id, parent.id, "Intermediate");
        let grandchild = child(&mut database, project.id, intermediate.id, "Grandchild");
        database
            .add_planning_dependency(intermediate.id, outside.id)
            .expect("dependency creates");
        database
            .complete_planning_task(intermediate.id, OpenSubtasks::Leave, None)
            .expect("intermediate completes alone");

        match database.complete_planning_task(parent.id, OpenSubtasks::Complete, Some("evidence".to_owned())) {
            Err(StorageError::InvalidPlanningTask(DomainError::SubtasksNotCompletable { id, reasons })) => {
                assert_eq!(id, parent.id.to_string());
                assert_eq!(
                    reasons,
                    vec![format!(
                        "`{}` is blocked by `{}` (pending)",
                        intermediate.id, outside.id
                    )]
                );
            }
            other => panic!("unexpected result: {other:?}"),
        }
        assert_eq!(status(&database, parent.id), TaskStatus::Pending);
        assert_eq!(status(&database, grandchild.id), TaskStatus::Pending);
        let parent = database
            .planning_task(parent.id)
            .expect("task reads")
            .expect("task exists");
        assert_eq!(parent.evidence, "");
    }

    #[test]
    fn complete_with_subtasks_ignores_blockers_held_by_cancelled_leaves() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let outside = task(&mut database, project.id, "Outside");
        let parent = task(&mut database, project.id, "Parent");
        let dropped = child(&mut database, project.id, parent.id, "Dropped");
        let open = child(&mut database, project.id, parent.id, "Open");
        database
            .add_planning_dependency(dropped.id, outside.id)
            .expect("dependency creates");
        database
            .transition_planning_task(dropped.id, TaskAction::Cancel, false)
            .expect("subtask cancels");

        let completion = database
            .complete_planning_task(parent.id, OpenSubtasks::Complete, None)
            .expect("a cancelled leaf holds nothing");

        assert_eq!(
            completion
                .completed_subtasks
                .iter()
                .map(|subtask| subtask.id)
                .collect::<Vec<_>>(),
            vec![open.id]
        );
        assert_eq!(status(&database, parent.id), TaskStatus::Completed);
        assert_eq!(status(&database, dropped.id), TaskStatus::Cancelled);
    }

    #[test]
    fn complete_with_subtasks_finishes_an_already_completed_parent() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let parent = task(&mut database, project.id, "Parent");
        let open = child(&mut database, project.id, parent.id, "Open");
        database
            .complete_planning_task(parent.id, OpenSubtasks::Leave, Some("first pass".to_owned()))
            .expect("task completes alone");

        let completion = database
            .complete_planning_task(parent.id, OpenSubtasks::Complete, None)
            .expect("completed task completes its subtasks");

        assert_eq!(completion.task.status, TaskStatus::Completed);
        assert_eq!(completion.task.evidence, "first pass");
        assert_eq!(
            completion
                .completed_subtasks
                .iter()
                .map(|subtask| subtask.id)
                .collect::<Vec<_>>(),
            vec![open.id]
        );
        assert_eq!(status(&database, open.id), TaskStatus::Completed);
    }

    #[test]
    fn complete_leave_and_refuse_keep_their_behaviour() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let parent = task(&mut database, project.id, "Parent");
        let open = child(&mut database, project.id, parent.id, "Open");

        match database.complete_planning_task(parent.id, OpenSubtasks::Refuse, None) {
            Err(StorageError::InvalidPlanningTask(DomainError::OpenDescendants { .. })) => {}
            other => panic!("unexpected result: {other:?}"),
        }
        let completion = database
            .complete_planning_task(parent.id, OpenSubtasks::Leave, None)
            .expect("task completes alone");
        assert!(completion.completed_subtasks.is_empty());
        assert_eq!(status(&database, parent.id), TaskStatus::Completed);
        assert_eq!(status(&database, open.id), TaskStatus::Pending);
    }

    fn ids(tasks: &[PlanningTask]) -> Vec<TaskId> {
        tasks.iter().map(|task| task.id).collect()
    }

    #[test]
    fn completion_lists_dependents_and_their_children_it_made_ready() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let blocker = task(&mut database, project.id, "Blocker");
        let other = task(&mut database, project.id, "Other blocker");
        let dependent = task(&mut database, project.id, "Dependent");
        let parent = task(&mut database, project.id, "Blocked parent");
        let leaf = child(&mut database, project.id, parent.id, "Leaf");
        let waiting = task(&mut database, project.id, "Still waiting");
        let unrelated = task(&mut database, project.id, "Unrelated");
        for (holder, blocking) in [
            (dependent.id, blocker.id),
            (parent.id, blocker.id),
            (waiting.id, blocker.id),
            (waiting.id, other.id),
        ] {
            database
                .add_planning_dependency(holder, blocking)
                .expect("dependency creates");
        }

        let completion = database
            .complete_planning_task(blocker.id, OpenSubtasks::Refuse, None)
            .expect("blocker completes");

        let mut unblocked = ids(&completion.unblocked);
        unblocked.sort();
        let mut expected = vec![dependent.id, leaf.id];
        expected.sort();
        assert_eq!(unblocked, expected);
        assert!(!unblocked.contains(&waiting.id));
        assert!(!unblocked.contains(&unrelated.id));
    }

    #[test]
    fn completion_with_subtasks_lists_dependents_of_each_completed_subtask() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let parent = task(&mut database, project.id, "Parent");
        let subtask = child(&mut database, project.id, parent.id, "Subtask");
        let dependent = task(&mut database, project.id, "Dependent");
        database
            .add_planning_dependency(dependent.id, subtask.id)
            .expect("dependency creates");

        let completion = database
            .complete_planning_task(parent.id, OpenSubtasks::Complete, None)
            .expect("task completes with its subtasks");

        assert_eq!(ids(&completion.unblocked), vec![dependent.id]);
    }

    #[test]
    fn completion_that_unblocks_nothing_lists_nothing() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let lone = task(&mut database, project.id, "Lone");
        task(&mut database, project.id, "Other");

        let completion = database
            .complete_planning_task(lone.id, OpenSubtasks::Refuse, None)
            .expect("task completes");
        assert!(completion.unblocked.is_empty());

        let repeat = database
            .complete_planning_task(lone.id, OpenSubtasks::Refuse, None)
            .expect("repeat completion is a no-op");
        assert!(repeat.unblocked.is_empty());
    }

    #[test]
    fn claim_sets_the_assignee_and_refuses_another_holder() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let work = task(&mut database, project.id, "Work");

        let claimed = database
            .claim_planning_task(work.id, false, " codex ")
            .expect("claim starts the task");
        assert_eq!(claimed.status, TaskStatus::InProgress);
        assert_eq!(claimed.assignee.as_deref(), Some("codex"));
        assert_eq!(
            database.planning_task(work.id).expect("task reads"),
            Some(claimed.clone())
        );

        let again = database
            .claim_planning_task(work.id, false, "codex")
            .expect("the holder can claim again");
        assert_eq!(again, claimed);

        let error = database
            .claim_planning_task(work.id, false, "claude")
            .expect_err("another holder is refused");
        assert!(matches!(error, StorageError::TaskClaimed { ref assignee, .. } if assignee == "codex"));
        assert_eq!(error.exit_code(), 4);
        assert_eq!(
            database.planning_task(work.id).expect("task reads"),
            Some(claimed.clone())
        );

        for force in [false, true] {
            let error = database
                .start_planning_task(work.id, force)
                .expect_err("an unclaimed start of a held task is refused");
            assert!(
                matches!(error, StorageError::TaskClaimed { ref assignee, .. } if assignee == "codex"),
                "force {force}: {error:?}"
            );
        }
        assert_eq!(database.planning_task(work.id).expect("task reads"), Some(claimed));
    }

    #[test]
    fn claim_refuses_an_unready_task_without_writing() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let blocker = task(&mut database, project.id, "Blocker");
        let blocked = task(&mut database, project.id, "Blocked");
        database
            .add_planning_dependency(blocked.id, blocker.id)
            .expect("dependency creates");

        database
            .claim_planning_task(blocked.id, false, "codex")
            .expect_err("an unready task is refused");
        let stored = database
            .planning_task(blocked.id)
            .expect("task reads")
            .expect("task exists");
        assert_eq!((stored.status, stored.assignee), (TaskStatus::Pending, None));
    }

    #[test]
    fn park_handoff_complete_and_cancel_release_the_claim() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        for action in ["park", "handoff", "complete", "cancel"] {
            let work = task(&mut database, project.id, action);
            database
                .claim_planning_task(work.id, false, "codex")
                .expect("claim starts the task");
            let released = match action {
                "park" => database.transition_planning_task(work.id, TaskAction::Park, false),
                "handoff" => database.handoff_planning_task(work.id, "resume".to_owned()),
                "complete" => database
                    .complete_planning_task(work.id, OpenSubtasks::Refuse, None)
                    .map(|completion| completion.task),
                _ => database.transition_planning_task(work.id, TaskAction::Cancel, false),
            }
            .expect("action applies");
            assert_eq!(released.assignee, None, "{action}");
            let stored = database
                .planning_task(work.id)
                .expect("task reads")
                .expect("task exists");
            assert_eq!(stored.assignee, None, "{action}");
        }
    }

    #[test]
    fn completing_with_subtasks_releases_their_claims() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let parent = task(&mut database, project.id, "Parent");
        let subtask = child(&mut database, project.id, parent.id, "Subtask");
        database
            .claim_planning_task(subtask.id, false, "codex")
            .expect("claim starts the subtask");

        let completion = database
            .complete_planning_task(parent.id, OpenSubtasks::Complete, None)
            .expect("task completes with its subtasks");
        assert_eq!(completion.completed_subtasks[0].assignee, None);
        let stored = database
            .planning_task(subtask.id)
            .expect("task reads")
            .expect("task exists");
        assert_eq!(stored.assignee, None);
    }

    #[test]
    fn start_next_starts_ready_tasks_in_order_until_none_is_left() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        task(&mut database, project.id, "First");
        task(&mut database, project.id, "Second");
        let expected = database
            .ready_planning_tasks()
            .expect("ready tasks read")
            .into_iter()
            .map(|task| task.id)
            .collect::<Vec<_>>();
        assert_eq!(expected.len(), 2);

        let started = database
            .start_next_planning_task(&PlanningReadyFilter::default(), Some("codex"))
            .expect("start succeeds")
            .expect("a task starts");
        assert_eq!(started.id, expected[0]);
        assert_eq!(started.status, TaskStatus::InProgress);
        assert_eq!(started.assignee.as_deref(), Some("codex"));

        let unclaimed = database
            .start_next_planning_task(&PlanningReadyFilter::default(), None)
            .expect("start succeeds")
            .expect("a task starts");
        assert_eq!(unclaimed.id, expected[1]);
        assert_eq!(unclaimed.assignee, None);

        assert_eq!(
            database
                .start_next_planning_task(&PlanningReadyFilter::default(), None)
                .expect("start succeeds"),
            None
        );
    }

    #[test]
    fn racing_start_next_never_starts_one_task_twice() {
        let directory = tempfile::tempdir().expect("directory creates");
        let path = directory.path().join("arcl.db");
        let mut database = Database::open(&path).expect("database opens");
        let project = database.project().expect("project exists");
        for round in 0..20 {
            let ready = if round % 2 == 0 { 2 } else { 1 };
            let created = (0..ready)
                .map(|index| task(&mut database, project.id, &format!("Work {round}.{index}")).id)
                .collect::<Vec<_>>();
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
            let racers = ["codex", "claude"].map(|name| {
                let (path, barrier) = (path.clone(), barrier.clone());
                std::thread::spawn(move || {
                    let mut database = Database::open(&path).expect("database opens");
                    barrier.wait();
                    database.start_next_planning_task(&PlanningReadyFilter::default(), Some(name))
                })
            });
            let results = racers.map(|racer| racer.join().expect("racer finishes"));
            let started = results
                .iter()
                .flat_map(|result| result.as_ref().expect("start succeeds"))
                .map(|task| task.id)
                .collect::<Vec<_>>();
            if ready == 2 {
                assert_eq!(started.len(), 2, "round {round}: {results:?}");
                assert_ne!(started[0], started[1], "round {round}: {results:?}");
            } else {
                assert_eq!(started, created, "round {round}: {results:?}");
            }
            assert!(started.iter().all(|id| created.contains(id)), "round {round}");
        }
    }

    #[test]
    fn racing_claims_leave_one_holder_and_refuse_the_other() {
        let directory = tempfile::tempdir().expect("directory creates");
        let path = directory.path().join("arcl.db");
        let mut database = Database::open(&path).expect("database opens");
        let project = database.project().expect("project exists");
        for round in 0..20 {
            let work = task(&mut database, project.id, &format!("Work {round}"));
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
            let racers = ["codex", "claude"].map(|name| {
                let (path, barrier) = (path.clone(), barrier.clone());
                std::thread::spawn(move || {
                    let mut database = Database::open(&path).expect("database opens");
                    barrier.wait();
                    database.claim_planning_task(work.id, false, name)
                })
            });
            let results = racers.map(|racer| racer.join().expect("racer finishes"));
            let winners = results
                .iter()
                .filter_map(|result| result.as_ref().ok())
                .collect::<Vec<_>>();
            assert_eq!(winners.len(), 1, "round {round}: {results:?}");
            let loser = results
                .iter()
                .find_map(|result| result.as_ref().err())
                .expect("one claim is refused");
            assert!(
                matches!(loser, StorageError::TaskClaimed { assignee, .. } if Some(assignee) == winners[0].assignee.as_ref()),
                "round {round}: {loser:?}"
            );
        }
    }

    #[test]
    fn notes_append_in_order_and_read_newest_first() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let work = task(&mut database, project.id, "Work");

        let first = database
            .add_task_note(
                work.id,
                Some(arcl_core::domain::TaskNoteKind::Decision),
                "  Use SQLite  ",
            )
            .expect("note adds");
        assert_eq!(first.text, "Use SQLite");
        assert!(first.created_at.ends_with('Z'));
        database
            .add_task_note(work.id, None, "line one\nline two")
            .expect("note adds");
        database
            .complete_planning_task(work.id, OpenSubtasks::Refuse, None)
            .expect("task completes");
        database
            .add_task_note(work.id, Some(arcl_core::domain::TaskNoteKind::Result), "Shipped")
            .expect("a completed task takes notes");

        let stored = database.task_notes(work.id).expect("notes read");
        assert_eq!(
            stored.iter().map(|note| note.text.as_str()).collect::<Vec<_>>(),
            ["Use SQLite", "line one\nline two", "Shipped"]
        );
        let view = database.planning_task_view(work.id).expect("view reads");
        assert_eq!(
            view.notes.iter().map(|note| note.text.as_str()).collect::<Vec<_>>(),
            ["Shipped", "line one\nline two", "Use SQLite"]
        );

        for text in ["   ", "bell\u{7}"] {
            assert!(matches!(
                database.add_task_note(work.id, None, text),
                Err(StorageError::InvalidTaskNoteText(DomainError::InvalidTaskNote))
            ));
        }
        assert_eq!(database.task_notes(work.id).expect("notes read").len(), 3);
    }

    #[test]
    fn context_keeps_the_most_recent_notes_and_counts_the_rest() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let work = task(&mut database, project.id, "Work");
        for index in 0..CONTEXT_NOTE_LIMIT + 2 {
            database
                .add_task_note(work.id, None, &format!("note {index}"))
                .expect("note adds");
        }

        let context = database.planning_context(work.id).expect("context reads");
        assert_eq!(context.notes.len(), CONTEXT_NOTE_LIMIT);
        assert_eq!(context.omitted_notes, 2);
        assert_eq!(context.notes[0].text, format!("note {}", CONTEXT_NOTE_LIMIT + 1));
        assert_eq!(context.notes[CONTEXT_NOTE_LIMIT - 1].text, "note 2");
    }
}
