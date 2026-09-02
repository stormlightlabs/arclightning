use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, params};
use serde::Serialize;

use super::*;
use arcl_core::domain::{
    DomainError, Phase, PhaseId, Plan, PlanId, PlanningTask, ProjectId, SpecId, TaskDependency, TaskId,
};
use arcl_core::plan::{PlanDocument, PlanEntries, PlanTaskEntry};

/// The change represented by one structured plan item.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanChange {
    Create,
    Update,
    Unchanged,
}

/// A phase or task change in a plan diff.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlanDiffItem {
    /// The phase key or fully qualified task key from the input.
    pub key: String,
    /// The existing record ID, when the input matched one.
    pub id: Option<String>,
    pub change: PlanChange,
}

/// One dependency change in a plan diff.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlanDependencyDiff {
    /// The task path from the structured input.
    pub task: String,
    /// The blocker reference from the structured input.
    pub blocker: String,
    pub change: PlanChange,
}

/// The non-destructive changes a structured plan would make.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlanDiff {
    pub plan_id: PlanId,
    /// Phase changes in document order.
    pub phases: Vec<PlanDiffItem>,
    /// Task changes in preorder.
    pub tasks: Vec<PlanDiffItem>,
    /// Dependency changes in task and input order.
    pub dependencies: Vec<PlanDependencyDiff>,
}

/// The persistent records returned after applying a structured plan.
#[derive(Clone, Debug, Serialize)]
pub struct PlanApplyResult {
    /// The persistent plan that owns the applied records.
    pub plan: Plan,
    /// All phases currently owned by the plan.
    pub phases: Vec<Phase>,
    /// All tasks currently owned by the plan.
    pub tasks: Vec<PlanningTask>,
    /// Dependencies whose blocked task belongs to this plan.
    pub dependencies: Vec<TaskDependency>,
    /// The changes made by this application.
    pub diff: PlanDiff,
}

struct PreparedPlan {
    project_id: ProjectId,
    plan: Plan,
    entries: PlanEntries,
    phases_by_key: HashMap<String, Phase>,
    tasks_by_path: HashMap<String, PlanningTask>,
    phase_ids: HashMap<String, PhaseId>,
    task_ids: HashMap<String, TaskId>,
    dependencies: Vec<(TaskId, TaskId, String, String)>,
    existing_dependencies: HashSet<(TaskId, TaskId)>,
    diff: PlanDiff,
}

pub fn check_plan(connection: &Connection, plan_id: PlanId, document: &PlanDocument) -> Result<PlanDiff> {
    diff_plan(connection, plan_id, document)
}

/// Return the phases, tasks, and dependencies a structured plan would add or update.
pub fn diff_plan(connection: &Connection, plan_id: PlanId, document: &PlanDocument) -> Result<PlanDiff> {
    Ok(prepare_plan(connection, plan_id, document)?.diff)
}

/// Apply structured plan input atomically and return its persistent records.
pub fn apply_plan(connection: &mut Connection, plan_id: PlanId, document: &PlanDocument) -> Result<PlanApplyResult> {
    let prepared = prepare_plan(connection, plan_id, document)?;
    let tx = connection.transaction()?;
    apply_prepared_plan(&tx, &prepared)?;
    tx.commit()?;
    finish_plan_apply(connection, prepared)
}

/// Create a persistent plan and apply its phases and tasks in one transaction.
pub fn create_and_apply_plan(
    connection: &mut Connection, project_id: ProjectId, spec_id: SpecId, title: String, body: String,
    document: &PlanDocument,
) -> Result<PlanApplyResult> {
    document.entries().map_err(StorageError::InvalidPlanInput)?;
    ensure_project(connection, project_id)?;
    let tx = connection.transaction()?;
    ensure_spec(&tx, project_id, spec_id)?;
    super::require_open_container(&tx, "specs", &spec_id.to_string(), "spec", StorageError::InvalidPlan)?;
    let plan = Plan::new(project_id, spec_id, title, body).map_err(StorageError::InvalidPlan)?;
    tx.execute(
        "INSERT INTO plans (id, project_id, spec_id, title, body, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            plan.id.to_string(),
            plan.project_id.to_string(),
            plan.spec_id.to_string(),
            plan.title,
            plan.body,
            plan.status.as_str()
        ],
    )?;
    let prepared = prepare_plan(&tx, plan.id, document)?;
    apply_prepared_plan(&tx, &prepared)?;
    tx.commit()?;
    finish_plan_apply(connection, prepared)
}

fn apply_prepared_plan(connection: &Connection, prepared: &PreparedPlan) -> Result<()> {
    for phase_entry in &prepared.entries.phases {
        let phase_id = prepared.phase_ids[&phase_entry.key];
        if let Some(current) = prepared.phases_by_key.get(&phase_entry.key) {
            connection.execute(
                "UPDATE phases SET plan_key = ?1, title = ?2, position = ?3 WHERE id = ?4",
                params![
                    phase_entry.key,
                    phase_entry.title,
                    i64::from(phase_entry.position),
                    current.id.to_string()
                ],
            )?;
        } else {
            let mut phase = Phase::new(
                prepared.project_id,
                prepared.plan.id,
                phase_entry.title.clone(),
                String::new(),
                i64::from(phase_entry.position),
            )
            .map_err(StorageError::InvalidPhase)?;
            phase.id = phase_id;
            phase.plan_key = Some(phase_entry.key.clone());
            insert_phase(connection, &phase)?;
        }
    }

    let tasks = prepared
        .entries
        .tasks
        .iter()
        .map(|entry| prepared_task(prepared, entry))
        .collect::<Result<Vec<_>>>()?;
    super::validate_task_graph_candidates(connection, prepared.project_id, &tasks, true)?;

    for (task_entry, task) in prepared.entries.tasks.iter().zip(tasks) {
        if prepared.tasks_by_path.contains_key(&task_entry.path) {
            connection.execute(
                "UPDATE planning_tasks
                 SET spec_id = ?1, plan_id = ?2, phase_id = ?3, parent_id = ?4, plan_key = ?5,
                     title = ?6, priority = ?7, position = ?8
                 WHERE id = ?9",
                params![
                    task.spec_id.map(|value| value.to_string()),
                    task.plan_id.map(|value| value.to_string()),
                    task.phase_id.map(|value| value.to_string()),
                    task.parent_id.map(|value| value.to_string()),
                    task.plan_key,
                    task.title,
                    task.priority.as_str(),
                    task.position,
                    task.id.to_string()
                ],
            )?;
        } else {
            insert_task(connection, &task)?;
        }
    }

    let mut existing_dependencies = prepared.existing_dependencies.clone();
    for (task_id, blocker_id, _, _) in &prepared.dependencies {
        if existing_dependencies.contains(&(*task_id, *blocker_id)) {
            continue;
        }
        insert_plan_dependency(connection, prepared.project_id, *task_id, *blocker_id)?;
        existing_dependencies.insert((*task_id, *blocker_id));
    }
    Ok(())
}

fn prepared_task(prepared: &PreparedPlan, task_entry: &PlanTaskEntry) -> Result<PlanningTask> {
    let task_id = prepared.task_ids[&task_entry.path];
    let phase_id = prepared.phase_ids[&task_entry.phase_key];
    let parent_id = task_entry.parent_path.as_ref().map(|path| prepared.task_ids[path]);
    if let Some(current) = prepared.tasks_by_path.get(&task_entry.path) {
        Ok(PlanningTask {
            id: current.id,
            project_id: current.project_id,
            spec_id: Some(prepared.plan.spec_id),
            plan_id: Some(prepared.plan.id),
            phase_id: Some(phase_id),
            parent_id,
            plan_key: Some(task_entry.path.clone()),
            title: task_entry.title.clone(),
            body: current.body.clone(),
            status: current.status,
            priority: task_entry.priority,
            position: i64::from(task_entry.position),
            handoff: current.handoff.clone(),
            evidence: current.evidence.clone(),
        })
    } else {
        let mut task = PlanningTask::new(
            prepared.project_id,
            Some(prepared.plan.spec_id),
            Some(prepared.plan.id),
            Some(phase_id),
            parent_id,
            task_entry.title.clone(),
            String::new(),
            task_entry.priority,
            i64::from(task_entry.position),
        )
        .map_err(StorageError::InvalidPlanningTask)?;
        task.id = task_id;
        task.plan_key = Some(task_entry.path.clone());
        Ok(task)
    }
}

fn finish_plan_apply(connection: &Connection, prepared: PreparedPlan) -> Result<PlanApplyResult> {
    let plan = plan(connection, prepared.plan.id)?
        .ok_or_else(|| StorageError::PlanNotFound { id: prepared.plan.id.to_string() })?;
    let phases = phases(connection, prepared.project_id)?
        .into_iter()
        .filter(|phase| phase.plan_id == prepared.plan.id)
        .collect();
    let tasks = planning_tasks(connection, prepared.project_id)?
        .into_iter()
        .filter(|task| task.plan_id == Some(prepared.plan.id))
        .collect();
    let dependencies = planning_dependencies(connection, prepared.project_id)?
        .into_iter()
        .filter(|dependency| prepared.task_ids.values().any(|id| *id == dependency.task_id))
        .collect();
    Ok(PlanApplyResult { plan, phases, tasks, dependencies, diff: prepared.diff })
}

fn prepare_plan(connection: &Connection, plan_id: PlanId, document: &PlanDocument) -> Result<PreparedPlan> {
    let entries = document.entries().map_err(StorageError::InvalidPlanInput)?;
    let plan = plan(connection, plan_id)?.ok_or_else(|| StorageError::PlanNotFound { id: plan_id.to_string() })?;
    let project_id = plan.project_id;
    let phases_by_key = phases(connection, project_id)?
        .into_iter()
        .filter(|phase| phase.plan_id == plan_id && phase.plan_key.is_some())
        .filter_map(|phase| phase.plan_key.clone().map(|key| (key, phase)))
        .collect::<HashMap<_, _>>();
    let tasks_by_path = planning_tasks(connection, project_id)?
        .into_iter()
        .filter(|task| task.plan_id == Some(plan_id) && task.plan_key.is_some())
        .filter_map(|task| task.plan_key.clone().map(|key| (key, task)))
        .collect::<HashMap<_, _>>();

    let phase_ids = entries
        .phases
        .iter()
        .map(|phase| {
            (
                phase.key.clone(),
                phases_by_key
                    .get(&phase.key)
                    .map_or_else(PhaseId::new, |value| value.id),
            )
        })
        .collect::<HashMap<_, _>>();
    let task_ids = entries
        .tasks
        .iter()
        .map(|task| {
            (
                task.path.clone(),
                tasks_by_path.get(&task.path).map_or_else(TaskId::new, |value| value.id),
            )
        })
        .collect::<HashMap<_, _>>();

    let mut dependencies = Vec::new();
    let mut seen_dependencies = HashSet::new();
    for task in &entries.tasks {
        let task_id = task_ids[&task.path];
        for reference in &task.blocked_by {
            let blocker_id = resolve_plan_task_reference(
                connection,
                project_id,
                &entries,
                &task_ids,
                &tasks_by_path,
                reference,
                &task.path,
            )?;
            if seen_dependencies.insert((task_id, blocker_id)) {
                dependencies.push((task_id, blocker_id, task.path.clone(), reference.clone()));
            }
        }
    }

    let existing_dependencies = planning_dependencies(connection, project_id)?
        .into_iter()
        .map(|dependency| (dependency.task_id, dependency.blocker_id))
        .collect::<HashSet<_>>();
    let mut dependency_graph = existing_dependencies.clone();
    for (task_id, blocker_id, _, _) in &dependencies {
        if task_id == blocker_id {
            return Err(StorageError::InvalidPlanningDependency(DomainError::SelfDependency {
                task: task_id.to_string(),
            }));
        }
        if !dependency_graph.contains(&(*task_id, *blocker_id)) {
            if dependency_reaches(&dependency_graph, *blocker_id, *task_id) {
                return Err(StorageError::InvalidPlanningDependency(DomainError::DependencyCycle {
                    task: task_id.to_string(),
                    blocker: blocker_id.to_string(),
                }));
            }
            dependency_graph.insert((*task_id, *blocker_id));
        }
    }
    let phases_diff = entries
        .phases
        .iter()
        .map(|entry| {
            let (id, change) = match phases_by_key.get(&entry.key) {
                None => (None, PlanChange::Create),
                Some(current) if current.title != entry.title || current.position != i64::from(entry.position) => {
                    (Some(current.id.to_string()), PlanChange::Update)
                }
                Some(current) => (Some(current.id.to_string()), PlanChange::Unchanged),
            };
            PlanDiffItem { key: entry.key.clone(), id, change }
        })
        .collect();
    let tasks_diff = entries
        .tasks
        .iter()
        .map(|entry| {
            let (id, change) = match tasks_by_path.get(&entry.path) {
                None => (None, PlanChange::Create),
                Some(current) if task_needs_update(current, entry, plan_id, phase_ids[&entry.phase_key], &task_ids) => {
                    (Some(current.id.to_string()), PlanChange::Update)
                }
                Some(current) => (Some(current.id.to_string()), PlanChange::Unchanged),
            };
            PlanDiffItem { key: entry.path.clone(), id, change }
        })
        .collect();
    let dependencies_diff = dependencies
        .iter()
        .map(|(task_id, blocker_id, task, blocker)| PlanDependencyDiff {
            task: task.clone(),
            blocker: blocker.clone(),
            change: if existing_dependencies.contains(&(*task_id, *blocker_id)) {
                PlanChange::Unchanged
            } else {
                PlanChange::Create
            },
        })
        .collect();
    let diff = PlanDiff { plan_id, phases: phases_diff, tasks: tasks_diff, dependencies: dependencies_diff };

    Ok(PreparedPlan {
        project_id,
        plan,
        entries,
        phases_by_key,
        tasks_by_path,
        phase_ids,
        task_ids,
        dependencies,
        existing_dependencies,
        diff,
    })
}

fn task_needs_update(
    current: &PlanningTask, entry: &PlanTaskEntry, plan_id: PlanId, phase_id: PhaseId,
    task_ids: &HashMap<String, TaskId>,
) -> bool {
    current.title != entry.title
        || current.priority != entry.priority
        || current.position != i64::from(entry.position)
        || current.spec_id.is_none()
        || current.plan_id != Some(plan_id)
        || current.phase_id != Some(phase_id)
        || current.parent_id != entry.parent_path.as_ref().map(|path| task_ids[path])
}

fn resolve_plan_task_reference(
    connection: &Connection, project_id: ProjectId, entries: &PlanEntries, task_ids: &HashMap<String, TaskId>,
    existing_tasks_by_path: &HashMap<String, PlanningTask>, reference: &str, task_path: &str,
) -> Result<TaskId> {
    if let Some(id) = task_ids.get(reference) {
        return Ok(*id);
    }
    if let Some(task) = existing_tasks_by_path.get(reference) {
        return Ok(task.id);
    }
    if let Ok(id) = TaskId::parse(reference) {
        let record = planning_task(connection, id)?;
        return match record {
            Some(record) if record.project_id == project_id => Ok(id),
            Some(record) => Err(StorageError::InvalidPlanningTask(DomainError::DifferentProject {
                entity: "task".to_owned(),
                id: id.to_string(),
                related: record.project_id.to_string(),
            })),
            None => Err(StorageError::InvalidPlanInput(
                arcl_core::plan::PlanError::UnknownDependency {
                    task: task_path.to_owned(),
                    dependency: reference.to_owned(),
                },
            )),
        };
    }
    let mut matches = entries
        .tasks
        .iter()
        .filter(|entry| entry.key == reference)
        .filter_map(|entry| task_ids.get(&entry.path).copied())
        .collect::<Vec<_>>();
    matches.extend(
        existing_tasks_by_path
            .values()
            .filter(|task| {
                task.plan_key
                    .as_deref()
                    .is_some_and(|key| key.rsplit('/').next() == Some(reference))
            })
            .map(|task| task.id),
    );
    matches.sort_unstable();
    matches.dedup();
    if matches.len() == 1 {
        return Ok(matches[0]);
    }
    if matches.len() > 1 {
        return Err(StorageError::InvalidPlanInput(
            arcl_core::plan::PlanError::AmbiguousDependency {
                task: task_path.to_owned(),
                dependency: reference.to_owned(),
            },
        ));
    }
    Err(StorageError::InvalidPlanInput(
        arcl_core::plan::PlanError::UnknownDependency { task: task_path.to_owned(), dependency: reference.to_owned() },
    ))
}

fn dependency_reaches(dependencies: &HashSet<(TaskId, TaskId)>, start: TaskId, target: TaskId) -> bool {
    let mut pending = vec![start];
    let mut seen = HashSet::new();
    while let Some(current) = pending.pop() {
        if current == target {
            return true;
        }
        if !seen.insert(current) {
            continue;
        }
        pending.extend(
            dependencies
                .iter()
                .filter(|(task, _)| *task == current)
                .map(|(_, blocker)| *blocker),
        );
    }
    false
}

fn insert_plan_dependency(
    connection: &Connection, project_id: ProjectId, task_id: TaskId, blocker_id: TaskId,
) -> Result<()> {
    let cycle: bool = connection.query_row(
        "WITH RECURSIVE reachable(id) AS (
             SELECT ?3
             UNION
             SELECT dependency.blocker_id
             FROM planning_task_dependencies dependency
             JOIN reachable ON dependency.task_id = reachable.id
             WHERE dependency.project_id = ?1
         )
         SELECT EXISTS (SELECT 1 FROM reachable WHERE id = ?2)",
        params![project_id.to_string(), task_id.to_string(), blocker_id.to_string()],
        |row| row.get(0),
    )?;
    if cycle {
        return Err(StorageError::InvalidPlanningDependency(DomainError::DependencyCycle {
            task: task_id.to_string(),
            blocker: blocker_id.to_string(),
        }));
    }
    connection.execute(
        "INSERT INTO planning_task_dependencies (project_id, task_id, blocker_id) VALUES (?1, ?2, ?3)",
        params![project_id.to_string(), task_id.to_string(), blocker_id.to_string()],
    )?;
    Ok(())
}
