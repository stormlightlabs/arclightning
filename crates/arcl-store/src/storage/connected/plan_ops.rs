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
    let tx = connection.transaction()?;
    let prepared = prepare_plan(&tx, plan_id, document)?;
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
    validate_plan_edges(
        connection,
        project_id,
        &entries,
        &task_ids,
        &tasks_by_path,
        &dependencies,
        &existing_dependencies,
    )?;
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

/// Refuse the first blocker rule violation applying the plan would cause.
///
/// Builds the graph as it will stand after the plan's parent links and
/// dependencies are written, so `plan check` and `plan diff` refuse the same
/// edges as `plan apply`.
fn validate_plan_edges(
    connection: &Connection, project_id: ProjectId, entries: &PlanEntries, task_ids: &HashMap<String, TaskId>,
    tasks_by_path: &HashMap<String, PlanningTask>, dependencies: &[(TaskId, TaskId, String, String)],
    existing_dependencies: &HashSet<(TaskId, TaskId)>,
) -> Result<()> {
    let mut parents = task_parents(connection, project_id)?;
    let mut new_parents = Vec::new();
    for entry in &entries.tasks {
        let task_id = task_ids[&entry.path];
        let previous = tasks_by_path.get(&entry.path).and_then(|task| task.parent_id);
        match entry.parent_path.as_ref().map(|path| task_ids[path]) {
            Some(parent) => {
                parents.insert(task_id, parent);
                if previous != Some(parent) {
                    new_parents.push((task_id, parent));
                }
            }
            None => {
                parents.remove(&task_id);
            }
        }
    }
    let mut edges = planning_dependencies(connection, project_id)?;
    let mut new_edges = Vec::new();
    for (task_id, blocker_id, _, _) in dependencies {
        let dependency = TaskDependency::new(*task_id, *blocker_id).map_err(StorageError::InvalidPlanningDependency)?;
        if !existing_dependencies.contains(&(*task_id, *blocker_id)) {
            edges.push(dependency);
            new_edges.push(dependency);
        }
    }
    match caused_violation(&parents, &edges, &new_edges, &new_parents) {
        Some(violation) => Err(StorageError::InvalidPlanningDependency(violation.to_error())),
        None => Ok(()),
    }
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

fn insert_plan_dependency(
    connection: &Connection, project_id: ProjectId, task_id: TaskId, blocker_id: TaskId,
) -> Result<()> {
    connection.execute(
        "INSERT INTO planning_task_dependencies (project_id, task_id, blocker_id) VALUES (?1, ?2, ?3)",
        params![project_id.to_string(), task_id.to_string(), blocker_id.to_string()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::storage::{Database, PlanningTaskUpdate, StorageError};
    use arcl_core::domain::{DomainError, PlanId, TaskId};
    use arcl_core::plan::{PlanDocument, parse};

    /// A database with one open plan, and that plan's ID.
    fn database_with_plan() -> (Database, PlanId) {
        let mut database = Database::open_in_memory().expect("database opens");
        let spec = database
            .create_spec("Spec".to_owned(), String::new(), String::new())
            .expect("spec creates");
        let plan = database
            .create_plan(spec.id, "Plan".to_owned(), String::new())
            .expect("plan creates");
        (database, plan.id)
    }

    /// A plan with phase `work`, whose tasks follow in TOML.
    fn document(tasks: &str) -> PlanDocument {
        parse(&format!(
            "format-version = 1\n\n[[phases]]\nkey = \"work\"\ntitle = \"Work\"\nposition = 0\n{tasks}"
        ))
        .expect("plan parses")
    }

    /// The ID of the task at `path` among the plan's tasks.
    fn task_id(database: &Database, path: &str) -> TaskId {
        database
            .planning_tasks()
            .expect("tasks read")
            .into_iter()
            .find(|task| task.plan_key.as_deref() == Some(path))
            .expect("task exists")
            .id
    }

    /// P holds A, and A holds C.
    const NESTED: &str = "\n[[phases.tasks]]\nkey = \"p\"\ntitle = \"P\"\nposition = 0\n\n[[phases.tasks.subtasks]]\nkey = \"a\"\ntitle = \"A\"\nposition = 0\n\n[[phases.tasks.subtasks.subtasks]]\nkey = \"c\"\ntitle = \"C\"\nposition = 0\n";

    #[test]
    fn plan_edge_violations_are_refused_and_write_nothing() {
        let cycle = "\n[[phases.tasks]]\nkey = \"a\"\ntitle = \"A\"\nposition = 0\nblocked-by = [\"work/b\"]\n\n[[phases.tasks]]\nkey = \"b\"\ntitle = \"B\"\nposition = 1\nblocked-by = [\"work/a\"]\n";
        let ancestor = "\n[[phases.tasks]]\nkey = \"p\"\ntitle = \"P\"\nposition = 0\n\n[[phases.tasks.subtasks]]\nkey = \"a\"\ntitle = \"A\"\nposition = 0\nblocked-by = [\"work/p\"]\n";
        let descendant = "\n[[phases.tasks]]\nkey = \"p\"\ntitle = \"P\"\nposition = 0\nblocked-by = [\"work/p/a\"]\n\n[[phases.tasks.subtasks]]\nkey = \"a\"\ntitle = \"A\"\nposition = 0\n";
        for (tasks, expected) in [
            (cycle, "cycle: "),
            (ancestor, "its ancestor;"),
            (descendant, "its descendant;"),
        ] {
            let (mut database, plan_id) = database_with_plan();
            let document = document(tasks);
            for error in [
                database.check_plan(plan_id, &document).expect_err("check refuses"),
                database.apply_plan(plan_id, &document).expect_err("apply refuses"),
            ] {
                assert!(
                    matches!(
                        &error,
                        StorageError::InvalidPlanningDependency(
                            DomainError::BlockerCycle { .. } | DomainError::BlockerAncestry { .. }
                        )
                    ),
                    "{error:?}"
                );
                assert!(error.to_string().contains(expected), "{error}");
            }
            assert!(database.planning_tasks().expect("tasks read").is_empty());
            assert!(database.phases().expect("phases read").is_empty());
            assert!(database.planning_dependencies().expect("dependencies read").is_empty());
        }
    }

    #[test]
    fn reparent_refuses_a_chain_through_the_moved_task() {
        let (mut database, plan_id) = database_with_plan();
        let document = document(NESTED);
        database.apply_plan(plan_id, &document).expect("plan applies");
        let (p, a, c) = (
            task_id(&database, "work/p"),
            task_id(&database, "work/p/a"),
            task_id(&database, "work/p/a/c"),
        );
        let detach = PlanningTaskUpdate { parent_id: Some(None), ..PlanningTaskUpdate::default() };
        database.update_planning_task(a, detach).expect("A detaches");
        database.add_planning_dependency(c, p).expect("C is blocked by P");

        let expected = DomainError::BlockerAncestry {
            task: c.to_string(),
            blocker: p.to_string(),
            kinship: "ancestor",
            chain: [c, a, p].map(|id| id.to_string()).to_vec(),
        };
        let attach = PlanningTaskUpdate { parent_id: Some(Some(p)), ..PlanningTaskUpdate::default() };
        let error = database.update_planning_task(a, attach).expect_err("update refuses");
        assert!(
            matches!(&error, StorageError::InvalidPlanningDependency(found) if *found == expected),
            "{error:?}"
        );
        let error = database.apply_plan(plan_id, &document).expect_err("plan refuses");
        assert!(
            matches!(&error, StorageError::InvalidPlanningDependency(found) if *found == expected),
            "{error:?}"
        );
        let moved = database.planning_task(a).expect("task reads").expect("task exists");
        assert_eq!(moved.parent_id, None);
    }

    #[test]
    fn writes_ignore_violations_already_elsewhere() {
        let (mut database, plan_id) = database_with_plan();
        database.apply_plan(plan_id, &document(NESTED)).expect("plan applies");
        let (a, c) = (task_id(&database, "work/p/a"), task_id(&database, "work/p/a/c"));
        // C blocked by its parent A, inside the subtree the reparent below moves.
        database
            .connection
            .execute(
                "INSERT INTO planning_task_dependencies (project_id, task_id, blocker_id)
                 SELECT project_id, ?1, ?2 FROM planning_tasks WHERE id = ?1",
                [c.to_string(), a.to_string()],
            )
            .expect("bad edge inserts");

        let other = "\n[[phases.tasks]]\nkey = \"q\"\ntitle = \"Q\"\nposition = 1\nblocked-by = [\"work/p\"]\n";
        let mut tasks = NESTED.to_owned();
        tasks.push_str(other);
        database
            .apply_plan(plan_id, &document(&tasks))
            .expect("unrelated plan applies");
        let q = task_id(&database, "work/q");
        let attach = PlanningTaskUpdate { parent_id: Some(Some(q)), ..PlanningTaskUpdate::default() };
        database
            .update_planning_task(a, attach)
            .expect("unrelated reparent succeeds");
        assert_eq!(database.planning_dependencies().expect("dependencies read").len(), 2);
    }
}
