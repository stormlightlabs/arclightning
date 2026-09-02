use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, params};
use serde::Serialize;

use super::{Result, StorageError, phases, planning_dependencies, planning_task, planning_tasks, specs};
use arcl_core::domain::{
    ContainerStatus, DomainError, Phase, PhaseId, Plan, PlanId, PlanningTask, ProjectId, Spec, SpecId, TaskAction,
    TaskDependency, TaskId, TaskPriority, TaskStatus,
};

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
pub struct PlanningReadiness {
    /// Whether the task is an actionable pending leaf with satisfied blockers.
    pub ready: bool,
    /// Whether at least one direct blocker is unfinished or missing.
    pub blocked: bool,
    /// Stable, user-facing explanations for every condition that prevents readiness.
    pub reasons: Vec<String>,
}

/// A direct connected-model blocker and its completion evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlanningBlockerView {
    /// The blocking task.
    pub task: PlanningTask,
    /// Whether the blocker has completed.
    pub completed: bool,
    /// Markdown evidence recorded on the blocker.
    pub evidence: String,
}

/// An enriched connected-model task view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlanningTaskView {
    /// The task being inspected.
    pub task: PlanningTask,
    /// The computed readiness result.
    pub readiness: PlanningReadiness,
    /// Direct blockers of the task.
    pub blockers: Vec<PlanningBlockerView>,
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
}

/// The focused context packet for a connected-model task.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
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
    /// Tasks that directly depend on this task.
    pub dependents: Vec<PlanningTask>,
    /// The computed readiness result.
    pub readiness: PlanningReadiness,
    /// Completed direct blockers that supplied evidence.
    pub completion_evidence: Vec<PlanningBlockerView>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Ancestry {
    spec_id: Option<SpecId>,
    plan_id: Option<PlanId>,
    phase_id: Option<PhaseId>,
}

struct PlanningData {
    tasks: HashMap<TaskId, PlanningTask>,
    specs: HashMap<SpecId, Spec>,
    plans: HashMap<PlanId, Plan>,
    phases: HashMap<PhaseId, Phase>,
    dependencies: Vec<TaskDependency>,
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
        if data.tasks.values().any(|child| child.parent_id == Some(task.id)) {
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
    let blockers = blocker_views(&data, id)?;
    let dependents = dependent_tasks(&data, id);
    let children = child_tasks(&data, id);
    let readiness = readiness(&data, &task, &ancestry_ids)?;

    Ok(PlanningTaskView {
        task,
        readiness,
        blockers,
        dependents,
        children,
        ancestors,
        spec: ancestry_ids.spec_id.and_then(|value| data.specs.get(&value).cloned()),
        plan: ancestry_ids.plan_id.and_then(|value| data.plans.get(&value).cloned()),
        phase: ancestry_ids.phase_id.and_then(|value| data.phases.get(&value).cloned()),
    })
}

/// Build the bounded task context packet used by agents and execution adapters.
pub fn context(connection: &Connection, id: TaskId) -> Result<PlanningContext> {
    let view = planning_task_view(connection, id)?;
    let completion_evidence = view
        .blockers
        .iter()
        .filter(|blocker| blocker.completed && !blocker.evidence.is_empty())
        .cloned()
        .collect();
    Ok(PlanningContext {
        task: view.task,
        ancestors: view.ancestors,
        spec: view.spec,
        plan: view.plan,
        phase: view.phase,
        blockers: view.blockers,
        dependents: view.dependents,
        readiness: view.readiness,
        completion_evidence,
    })
}

/// Apply one connected-model task lifecycle action atomically.
pub fn transition_planning_task(
    connection: &mut Connection, project_id: ProjectId, id: TaskId, action: TaskAction, allow_open_children: bool,
) -> Result<PlanningTask> {
    transition(connection, project_id, id, action, allow_open_children, None)
}

/// Complete a connected-model task and optionally store Markdown evidence atomically.
pub fn complete_planning_task(
    connection: &mut Connection, project_id: ProjectId, id: TaskId, allow_open_children: bool, evidence: Option<String>,
) -> Result<PlanningTask> {
    transition(
        connection,
        project_id,
        id,
        TaskAction::Complete,
        allow_open_children,
        evidence,
    )
}

/// Store a handoff note and park an in-progress connected-model task atomically.
pub fn handoff_planning_task(
    connection: &mut Connection, project_id: ProjectId, id: TaskId, note: String,
) -> Result<PlanningTask> {
    let tx = connection.transaction()?;
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
        "UPDATE planning_tasks SET handoff = ?1, status = 'parked' WHERE id = ?2",
        params![note, id.to_string()],
    )?;
    tx.commit()?;
    Ok(PlanningTask { status: TaskStatus::Parked, handoff: note, ..current })
}

fn transition(
    connection: &mut Connection, project_id: ProjectId, id: TaskId, action: TaskAction, allow_open_children: bool,
    evidence: Option<String>,
) -> Result<PlanningTask> {
    let tx = connection.transaction()?;
    let current = planning_task(&tx, id)?.ok_or_else(|| StorageError::PlanningTaskNotFound { id: id.to_string() })?;
    ensure_project_task(&current, project_id)?;
    validate_task_graph(&tx, project_id, &current, false)?;
    let next_status = current
        .status
        .apply(action)
        .map_err(StorageError::InvalidPlanningTask)?;
    if next_status == current.status && evidence.is_none() {
        return Ok(current);
    }
    if matches!(action, TaskAction::Complete | TaskAction::Cancel) && !allow_open_children {
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
    if next_status == current.status {
        tx.execute(
            "UPDATE planning_tasks SET evidence = ?1 WHERE id = ?2",
            params![evidence.as_deref().unwrap_or(&current.evidence), id.to_string()],
        )?;
    } else {
        tx.execute(
            "UPDATE planning_tasks SET status = ?1, evidence = COALESCE(?2, evidence) WHERE id = ?3",
            params![next_status.as_str(), evidence, id.to_string()],
        )?;
    }
    tx.commit()?;
    Ok(PlanningTask { status: next_status, evidence: evidence.unwrap_or(current.evidence), ..current })
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

fn load_data(connection: &Connection, project_id: ProjectId) -> Result<PlanningData> {
    Ok(PlanningData {
        tasks: planning_tasks(connection, project_id)?
            .into_iter()
            .map(|task| (task.id, task))
            .collect(),
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
        dependencies: planning_dependencies(connection, project_id)?,
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
    let mut reasons = Vec::new();
    if task.status != TaskStatus::Pending {
        reasons.push(format!("task status is `{}`", task.status.as_str()));
    }
    let children = child_tasks(data, task.id);
    if !children.is_empty() {
        reasons.push(format!("task has {} child task(s)", children.len()));
    }
    for ancestor in parent_chain(data, task.id)? {
        if matches!(
            ancestor.status,
            TaskStatus::Parked | TaskStatus::Completed | TaskStatus::Cancelled
        ) {
            reasons.push(format!("ancestor `{}` is `{}`", ancestor.id, ancestor.status.as_str()));
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
            reasons.push(format!("{entity} `{id}` is `{}`", status.as_str()));
        }
    }

    let mut blocked = false;
    for dependency in data
        .dependencies
        .iter()
        .filter(|dependency| dependency.task_id == task.id)
    {
        match data.tasks.get(&dependency.blocker_id) {
            Some(blocker) if blocker.status == TaskStatus::Completed => {}
            Some(blocker) => {
                blocked = true;
                reasons.push(format!("blocked by `{}` ({})", blocker.id, blocker.status.as_str()));
            }
            None => {
                blocked = true;
                reasons.push(format!("blocked by missing task `{}`", dependency.blocker_id));
            }
        }
    }
    Ok(PlanningReadiness { ready: reasons.is_empty(), blocked, reasons })
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

fn blocker_views(data: &PlanningData, id: TaskId) -> Result<Vec<PlanningBlockerView>> {
    data.dependencies
        .iter()
        .filter(|dependency| dependency.task_id == id)
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
        .tasks
        .values()
        .filter(|task| task.parent_id == Some(id))
        .cloned()
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
                .any(|reason| reason.contains("blocked by"))
        );
        assert_eq!(view.spec.as_ref().expect("spec context").body, "spec markdown");
        assert_eq!(view.plan.as_ref().expect("plan context").body, "plan markdown");
        assert_eq!(view.phase.as_ref().expect("phase context").body, "phase markdown");

        database
            .transition_planning_task(blocker.id, TaskAction::Complete, false)
            .expect("blocker completes");
        database
            .complete_planning_task(task.id, false, Some("verified".to_owned()))
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

        database
            .transition_planning_task(task.id, TaskAction::Start, false)
            .expect("task can still be explicitly started after a forced container close");
        let handoff = database
            .handoff_planning_task(task.id, "resume".to_owned())
            .expect("handoff parks atomically");
        assert_eq!(handoff.status, TaskStatus::Parked);
        assert_eq!(handoff.handoff, "resume");
    }
}
