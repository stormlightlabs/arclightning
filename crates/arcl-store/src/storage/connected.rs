use std::{collections::HashSet, str::FromStr};

use rusqlite::{Connection, OptionalExtension, params};

use super::{Result, StorageError, releases};
use arcl_core::domain::*;

mod execution;
mod plan_ops;
mod promotions;

pub use execution::*;
pub use plan_ops::*;
pub use promotions::{CapturePromotionInput, CapturePromotionRecord, CapturePromotionResult, CaptureTaskPromotion};

/// The stable project ID created for every operational database.
pub const DEFAULT_PROJECT_ID: &str = "arcl-pj-00000000000000000000000000";

/// All connected records and explicit edges belonging to one project.
#[derive(Clone, Debug)]
pub struct ConnectedGraph {
    /// The owning project.
    pub project: Project,
    /// Inbox captures in creation order.
    pub captures: Vec<Capture>,
    /// Capture provenance edges.
    pub capture_promotions: Vec<CapturePromotion>,
    /// Owned specifications.
    pub specs: Vec<Spec>,
    /// Persistent plans.
    pub plans: Vec<Plan>,
    /// Optional plan phases.
    pub phases: Vec<Phase>,
    /// Tasks at any supported planning level.
    pub tasks: Vec<PlanningTask>,
    /// Task blocking relationships.
    pub dependencies: Vec<arcl_core::domain::TaskDependency>,
    /// Markdown notes.
    pub notes: Vec<Note>,
    /// Named releases.
    pub releases: Vec<Release>,
    /// Explicit release membership edges.
    pub release_memberships: Vec<ReleaseMembership>,
    /// Explicit record links.
    pub links: Vec<RecordLink>,
}

/// Result of validating SQLite and connected-graph integrity.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize)]
pub struct CheckReport {
    /// Whether every integrity check passed.
    pub valid: bool,
    /// Human-readable validation failures.
    pub errors: Vec<String>,
}

/// Fields used to create a task at any supported planning level.
#[derive(Clone, Debug)]
pub struct PlanningTaskCreate {
    /// The owning project.
    pub project_id: ProjectId,
    /// Optional specification ancestry.
    pub spec_id: Option<SpecId>,
    /// Optional plan ancestry.
    pub plan_id: Option<PlanId>,
    /// Optional phase ancestry.
    pub phase_id: Option<PhaseId>,
    /// Optional parent-task ancestry.
    pub parent_id: Option<TaskId>,
    /// The task title.
    pub title: String,
    /// Markdown task body.
    pub body: String,
    /// Initial task priority.
    pub priority: TaskPriority,
    /// Stable sibling ordering position.
    pub position: i64,
}

/// Optional fields accepted by a connected task update.
#[derive(Clone, Debug, Default)]
pub struct PlanningTaskUpdate {
    /// Replacement title, when provided.
    pub title: Option<String>,
    /// Replacement Markdown body, when provided.
    pub body: Option<String>,
    /// Replacement priority, when provided.
    pub priority: Option<TaskPriority>,
    /// Replacement sibling position, when provided.
    pub position: Option<i64>,
    /// Optional replacement or clearing of specification ancestry.
    pub spec_id: Option<Option<SpecId>>,
    /// Optional replacement or clearing of plan ancestry.
    pub plan_id: Option<Option<PlanId>>,
    /// Optional replacement or clearing of phase ancestry.
    pub phase_id: Option<Option<PhaseId>>,
    /// Optional replacement or clearing of parent-task ancestry.
    pub parent_id: Option<Option<TaskId>>,
}

/// Optional fields accepted by a specification update.
#[derive(Clone, Debug, Default)]
pub struct SpecUpdate {
    /// Replacement title, when provided.
    pub title: Option<String>,
    /// Replacement Markdown body, when provided.
    pub body: Option<String>,
    /// Replacement acceptance criteria, when provided.
    pub acceptance_criteria: Option<String>,
}

/// Optional fields accepted by a plan update.
#[derive(Clone, Debug, Default)]
pub struct PlanUpdate {
    /// Replacement title, when provided.
    pub title: Option<String>,
    /// Replacement Markdown body, when provided.
    pub body: Option<String>,
}

/// Optional fields accepted by a phase update.
#[derive(Clone, Debug, Default)]
pub struct PhaseUpdate {
    /// Replacement title, when provided.
    pub title: Option<String>,
    /// Replacement Markdown body, when provided.
    pub body: Option<String>,
    /// Replacement sibling position, when provided.
    pub position: Option<i64>,
}

/// Optional fields accepted by a note update.
#[derive(Clone, Debug, Default)]
pub struct NoteUpdate {
    /// Replacement title, when provided.
    pub title: Option<String>,
    /// Replacement Markdown body, when provided.
    pub body: Option<String>,
}

/// Optional fields accepted by a capture update.
#[derive(Clone, Debug, Default)]
pub struct CaptureUpdate {
    /// Replacement title, when provided.
    pub title: Option<String>,
    /// Replacement Markdown body, when provided.
    pub body: Option<String>,
}

struct RawPhase {
    id: String,
    project: String,
    plan: String,
    plan_key: Option<String>,
    title: String,
    body: String,
    status: String,
    position: i64,
}

struct RawTask {
    id: String,
    project: String,
    spec: Option<String>,
    plan: Option<String>,
    phase: Option<String>,
    parent: Option<String>,
    plan_key: Option<String>,
    title: String,
    body: String,
    status: String,
    priority: String,
    position: i64,
    handoff: String,
    evidence: String,
}

pub fn project(connection: &Connection) -> Result<Project> {
    let (id, name) = connection
        .query_row("SELECT id, name FROM projects ORDER BY id LIMIT 1", [], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .optional()?
        .ok_or(StorageError::ProjectNotFound)?;
    let id = ProjectId::parse(&id)
        .map_err(DomainError::from)
        .map_err(StorageError::InvalidProject)?;
    Project::from_parts(id, name).map_err(StorageError::InvalidProject)
}

/// Validate SQLite constraints and the connected planning graph.
pub fn check(connection: &Connection) -> Result<CheckReport> {
    let mut errors = Vec::new();
    let integrity: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        errors.push(format!("SQLite integrity check: {integrity}"));
    }
    let mut statement = connection.prepare("PRAGMA foreign_key_check")?;
    let violations = statement.query_map([], |row| row.get::<_, String>(0))?;
    for table in violations {
        errors.push(format!("foreign-key violation in `{}`", table?));
    }

    let graph = graph(connection)?;
    for task in &graph.tasks {
        if has_parent_cycle(&graph.tasks, task.id) {
            errors.push(format!("parent cycle includes `{}`", task.id));
        }
        if has_dependency_cycle(&graph.dependencies, task.id) {
            errors.push(format!("dependency cycle includes `{}`", task.id));
        }
    }
    Ok(CheckReport { valid: errors.is_empty(), errors })
}

pub fn graph(connection: &Connection) -> Result<ConnectedGraph> {
    let project = project(connection)?;
    let project_id = project.id;
    Ok(ConnectedGraph {
        project,
        captures: captures(connection, project_id)?,
        capture_promotions: capture_promotions(connection, project_id)?,
        specs: specs(connection, project_id)?,
        plans: plans(connection, project_id)?,
        phases: phases(connection, project_id)?,
        tasks: planning_tasks(connection, project_id)?,
        dependencies: planning_dependencies(connection, project_id)?,
        notes: notes(connection, project_id)?,
        releases: releases::list(connection)?,
        release_memberships: release_memberships(connection, project_id)?,
        links: record_links(connection, project_id)?,
    })
}

pub fn captures(connection: &Connection, project_id: ProjectId) -> Result<Vec<Capture>> {
    let mut statement = connection.prepare(
        "SELECT id, project_id, title, body, status, created_at FROM captures
         WHERE project_id = ?1 ORDER BY created_at, id",
    )?;
    let rows = statement.query_map([project_id.to_string()], raw_capture)?;
    rows.map(|row| decode_capture(row?)).collect()
}

pub fn capture(connection: &Connection, id: CaptureId) -> Result<Option<Capture>> {
    let raw = connection
        .query_row(
            "SELECT id, project_id, title, body, status, created_at FROM captures WHERE id = ?1",
            [id.to_string()],
            raw_capture,
        )
        .optional()?;
    raw.map(decode_capture).transpose()
}

pub fn create_capture(
    connection: &mut Connection, project_id: ProjectId, title: String, body: String,
) -> Result<Capture> {
    ensure_project(connection, project_id)?;
    let tx = connection.transaction()?;
    let created_at: String = tx.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |row| row.get(0))?;
    let capture = Capture::new(project_id, title, body, created_at).map_err(StorageError::InvalidCapture)?;
    tx.execute(
        "INSERT INTO captures (id, project_id, title, body, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            capture.id.to_string(),
            capture.project_id.to_string(),
            capture.title,
            capture.body,
            capture.status.as_str(),
            capture.created_at
        ],
    )?;
    tx.commit()?;
    Ok(capture)
}

/// Read capture promotion provenance as explicit typed relationships.
pub fn capture_promotions(connection: &Connection, project_id: ProjectId) -> Result<Vec<CapturePromotion>> {
    promotions::capture_promotions(connection, project_id)
}

pub fn update_capture(connection: &mut Connection, id: CaptureId, update: CaptureUpdate) -> Result<Capture> {
    if update.title.is_none() && update.body.is_none() {
        return Err(StorageError::InvalidCapture(DomainError::NoFieldsToUpdate {
            entity: "capture",
        }));
    }
    if let Some(title) = &update.title {
        validate_title(title).map_err(StorageError::InvalidCapture)?;
    }
    let tx = connection.transaction()?;
    let current = capture(&tx, id)?.ok_or_else(|| StorageError::CaptureNotFound { id: id.to_string() })?;
    if current.status == CaptureStatus::Discarded {
        return Err(StorageError::InvalidCapture(DomainError::InvalidTransition {
            entity: "capture",
            action: "update",
            from: current.status.as_str().to_owned(),
        }));
    }
    let title = update.title.as_deref().unwrap_or(&current.title);
    let body = update.body.as_deref().unwrap_or(&current.body);
    tx.execute(
        "UPDATE captures SET title = ?1, body = ?2 WHERE id = ?3",
        params![title, body, id.to_string()],
    )?;
    tx.commit()?;
    Ok(Capture { title: title.to_owned(), body: body.to_owned(), ..current })
}

pub fn discard_capture(connection: &mut Connection, id: CaptureId) -> Result<Capture> {
    let tx = connection.transaction()?;
    let current = capture(&tx, id)?.ok_or_else(|| StorageError::CaptureNotFound { id: id.to_string() })?;
    let next_status = current
        .status
        .apply(CaptureAction::Discard)
        .map_err(StorageError::InvalidCapture)?;
    if next_status != current.status {
        tx.execute(
            "UPDATE captures SET status = ?1 WHERE id = ?2",
            params![next_status.as_str(), id.to_string()],
        )?;
    }
    tx.commit()?;
    Ok(Capture { status: next_status, ..current })
}

/// Promote a capture through the connected promotion workflow.
pub fn promote_capture(
    connection: &mut Connection, id: CaptureId, input: CapturePromotionInput,
) -> Result<CapturePromotionResult> {
    promotions::promote_capture(connection, id, input)
}

pub fn specs(connection: &Connection, project_id: ProjectId) -> Result<Vec<Spec>> {
    let mut statement = connection.prepare(
        "SELECT id, project_id, title, body, acceptance_criteria, status, source_capture_id FROM specs
         WHERE project_id = ?1 ORDER BY id",
    )?;
    let rows = statement.query_map([project_id.to_string()], raw_spec)?;
    rows.map(|row| decode_spec(row?)).collect()
}

pub fn spec(connection: &Connection, id: SpecId) -> Result<Option<Spec>> {
    let raw = connection
        .query_row(
            "SELECT id, project_id, title, body, acceptance_criteria, status, source_capture_id FROM specs WHERE id = ?1",
            [id.to_string()], raw_spec,
        )
        .optional()?;
    raw.map(decode_spec).transpose()
}

pub fn create_spec(
    connection: &mut Connection, project_id: ProjectId, title: String, body: String, acceptance_criteria: String,
) -> Result<Spec> {
    ensure_project(connection, project_id)?;
    let spec = Spec::new(project_id, title, body, acceptance_criteria).map_err(StorageError::InvalidSpec)?;
    let tx = connection.transaction()?;
    tx.execute(
        "INSERT INTO specs (id, project_id, title, body, acceptance_criteria, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            spec.id.to_string(),
            spec.project_id.to_string(),
            spec.title,
            spec.body,
            spec.acceptance_criteria,
            spec.status.as_str()
        ],
    )?;
    tx.commit()?;
    Ok(spec)
}

pub fn update_spec(connection: &mut Connection, id: SpecId, update: SpecUpdate) -> Result<Spec> {
    if update.title.is_none() && update.body.is_none() && update.acceptance_criteria.is_none() {
        return Err(StorageError::InvalidSpec(DomainError::NoFieldsToUpdate {
            entity: "spec",
        }));
    }
    if let Some(title) = &update.title {
        validate_title(title).map_err(StorageError::InvalidSpec)?;
    }
    let tx = connection.transaction()?;
    let current = spec(&tx, id)?.ok_or_else(|| StorageError::SpecNotFound { id: id.to_string() })?;
    let title = update.title.as_deref().unwrap_or(&current.title);
    let body = update.body.as_deref().unwrap_or(&current.body);
    let acceptance = update
        .acceptance_criteria
        .as_deref()
        .unwrap_or(&current.acceptance_criteria);
    tx.execute(
        "UPDATE specs SET title = ?1, body = ?2, acceptance_criteria = ?3 WHERE id = ?4",
        params![title, body, acceptance, id.to_string()],
    )?;
    tx.commit()?;
    Ok(Spec {
        title: title.to_owned(),
        body: body.to_owned(),
        acceptance_criteria: acceptance.to_owned(),
        ..current
    })
}

/// Complete or cancel a specification after checking its descendants.
pub fn transition_spec(
    connection: &mut Connection, id: SpecId, action: ContainerAction, allow_open_children: bool,
) -> Result<Spec> {
    let tx = connection.transaction()?;
    let current = spec(&tx, id)?.ok_or_else(|| StorageError::SpecNotFound { id: id.to_string() })?;
    let next_status = current
        .status
        .apply("spec", action)
        .map_err(StorageError::InvalidSpec)?;
    if next_status == current.status {
        return Ok(current);
    }
    if !allow_open_children && has_open_spec_descendants(&tx, id)? {
        return Err(StorageError::InvalidSpec(DomainError::OpenDescendants {
            entity: "spec",
            id: id.to_string(),
            action: action.as_str(),
        }));
    }
    tx.execute(
        "UPDATE specs SET status = ?1 WHERE id = ?2",
        params![next_status.as_str(), id.to_string()],
    )?;
    tx.commit()?;
    Ok(Spec { status: next_status, ..current })
}

fn has_open_spec_descendants(connection: &Connection, id: SpecId) -> Result<bool> {
    Ok(connection.query_row(
        "SELECT EXISTS (
             SELECT 1 FROM plans WHERE spec_id = ?1 AND status = 'open'
             UNION ALL
             SELECT 1 FROM planning_tasks WHERE spec_id = ?1 AND status NOT IN ('completed', 'cancelled')
             UNION ALL
             SELECT 1 FROM planning_tasks t JOIN plans p ON p.id = t.plan_id
             WHERE p.spec_id = ?1 AND t.status NOT IN ('completed', 'cancelled')
         )",
        [id.to_string()],
        |row| row.get(0),
    )?)
}

pub fn plans(connection: &Connection, project_id: ProjectId) -> Result<Vec<Plan>> {
    let mut statement = connection.prepare(
        "SELECT id, project_id, spec_id, title, body, status FROM plans
         WHERE project_id = ?1 ORDER BY spec_id, id",
    )?;
    let rows = statement.query_map([project_id.to_string()], raw_plan)?;
    rows.map(|row| decode_plan(row?)).collect()
}

pub fn plan(connection: &Connection, id: PlanId) -> Result<Option<Plan>> {
    let raw = connection
        .query_row(
            "SELECT id, project_id, spec_id, title, body, status FROM plans WHERE id = ?1",
            [id.to_string()],
            raw_plan,
        )
        .optional()?;
    raw.map(decode_plan).transpose()
}

pub fn create_plan(
    connection: &mut Connection, project_id: ProjectId, spec_id: SpecId, title: String, body: String,
) -> Result<Plan> {
    ensure_project(connection, project_id)?;
    let tx = connection.transaction()?;
    ensure_spec(&tx, project_id, spec_id)?;
    require_open_container(&tx, "specs", &spec_id.to_string(), "spec", StorageError::InvalidPlan)?;
    let plan = Plan::new(project_id, spec_id, title, body).map_err(StorageError::InvalidPlan)?;
    tx.execute(
        "INSERT INTO plans (id, project_id, spec_id, title, body, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            plan.id.to_string(),
            project_id.to_string(),
            spec_id.to_string(),
            plan.title,
            plan.body,
            plan.status.as_str()
        ],
    )?;
    tx.commit()?;
    Ok(plan)
}

pub fn update_plan(connection: &mut Connection, id: PlanId, update: PlanUpdate) -> Result<Plan> {
    if update.title.is_none() && update.body.is_none() {
        return Err(StorageError::InvalidPlan(DomainError::NoFieldsToUpdate {
            entity: "plan",
        }));
    }
    if let Some(title) = &update.title {
        validate_title(title).map_err(StorageError::InvalidPlan)?;
    }
    let tx = connection.transaction()?;
    let current = plan(&tx, id)?.ok_or_else(|| StorageError::PlanNotFound { id: id.to_string() })?;
    let title = update.title.as_deref().unwrap_or(&current.title);
    let body = update.body.as_deref().unwrap_or(&current.body);
    tx.execute(
        "UPDATE plans SET title = ?1, body = ?2 WHERE id = ?3",
        params![title, body, id.to_string()],
    )?;
    tx.commit()?;
    Ok(Plan { title: title.to_owned(), body: body.to_owned(), ..current })
}

/// Complete or cancel a persistent plan after checking phases and tasks.
pub fn transition_plan(
    connection: &mut Connection, id: PlanId, action: ContainerAction, allow_open_children: bool,
) -> Result<Plan> {
    let tx = connection.transaction()?;
    let current = plan(&tx, id)?.ok_or_else(|| StorageError::PlanNotFound { id: id.to_string() })?;
    let next_status = current
        .status
        .apply("plan", action)
        .map_err(StorageError::InvalidPlan)?;
    if next_status == current.status {
        return Ok(current);
    }
    if !allow_open_children && has_open_plan_descendants(&tx, id)? {
        return Err(StorageError::InvalidPlan(DomainError::OpenDescendants {
            entity: "plan",
            id: id.to_string(),
            action: action.as_str(),
        }));
    }
    tx.execute(
        "UPDATE plans SET status = ?1 WHERE id = ?2",
        params![next_status.as_str(), id.to_string()],
    )?;
    tx.commit()?;
    Ok(Plan { status: next_status, ..current })
}

fn has_open_plan_descendants(connection: &Connection, id: PlanId) -> Result<bool> {
    Ok(connection.query_row(
        "SELECT EXISTS (
             SELECT 1 FROM phases WHERE plan_id = ?1 AND status = 'open'
             UNION ALL
             SELECT 1 FROM planning_tasks WHERE plan_id = ?1 AND status NOT IN ('completed', 'cancelled')
         )",
        [id.to_string()],
        |row| row.get(0),
    )?)
}

pub fn phases(connection: &Connection, project_id: ProjectId) -> Result<Vec<Phase>> {
    let mut statement = connection.prepare("SELECT id, project_id, plan_id, plan_key, title, body, status, position FROM phases WHERE project_id = ?1 ORDER BY plan_id, position, id")?;
    let rows = statement.query_map([project_id.to_string()], raw_phase)?;
    rows.map(|row| decode_phase(row?)).collect()
}

pub fn phase(connection: &Connection, id: PhaseId) -> Result<Option<Phase>> {
    let raw = connection
        .query_row(
            "SELECT id, project_id, plan_id, plan_key, title, body, status, position FROM phases WHERE id = ?1",
            [id.to_string()],
            raw_phase,
        )
        .optional()?;
    raw.map(decode_phase).transpose()
}

pub fn create_phase(
    connection: &mut Connection, project_id: ProjectId, plan_id: PlanId, title: String, body: String, position: i64,
) -> Result<Phase> {
    ensure_project(connection, project_id)?;
    let phase = Phase::new(project_id, plan_id, title, body, position).map_err(StorageError::InvalidPhase)?;
    let tx = connection.transaction()?;
    ensure_plan(&tx, project_id, plan_id)?;
    require_open_container(&tx, "plans", &plan_id.to_string(), "plan", StorageError::InvalidPhase)?;
    let spec_id: String = tx.query_row(
        "SELECT spec_id FROM plans WHERE project_id = ?1 AND id = ?2",
        params![project_id.to_string(), plan_id.to_string()],
        |row| row.get(0),
    )?;
    require_open_container(&tx, "specs", &spec_id, "spec", StorageError::InvalidPhase)?;
    insert_phase(&tx, &phase)?;
    tx.commit()?;
    Ok(phase)
}

pub fn update_phase(connection: &mut Connection, id: PhaseId, update: PhaseUpdate) -> Result<Phase> {
    if update.title.is_none() && update.body.is_none() && update.position.is_none() {
        return Err(StorageError::InvalidPhase(DomainError::NoFieldsToUpdate {
            entity: "phase",
        }));
    }
    if let Some(title) = &update.title {
        validate_title(title).map_err(StorageError::InvalidPhase)?;
    }
    if let Some(position) = update.position {
        validate_position(position).map_err(StorageError::InvalidPhase)?;
    }
    let tx = connection.transaction()?;
    let current = phase(&tx, id)?.ok_or_else(|| StorageError::PhaseNotFound { id: id.to_string() })?;
    let title = update.title.as_deref().unwrap_or(&current.title);
    let body = update.body.as_deref().unwrap_or(&current.body);
    let position = update.position.unwrap_or(current.position);
    tx.execute(
        "UPDATE phases SET title = ?1, body = ?2, position = ?3 WHERE id = ?4",
        params![title, body, position, id.to_string()],
    )?;
    tx.commit()?;
    Ok(Phase { title: title.to_owned(), body: body.to_owned(), position, ..current })
}

/// Complete or cancel a phase after checking its tasks.
pub fn transition_phase(
    connection: &mut Connection, id: PhaseId, action: ContainerAction, allow_open_children: bool,
) -> Result<Phase> {
    let tx = connection.transaction()?;
    let current = phase(&tx, id)?.ok_or_else(|| StorageError::PhaseNotFound { id: id.to_string() })?;
    let next_status = current
        .status
        .apply("phase", action)
        .map_err(StorageError::InvalidPhase)?;
    if next_status == current.status {
        return Ok(current);
    }
    if !allow_open_children && has_open_phase_descendants(&tx, id)? {
        return Err(StorageError::InvalidPhase(DomainError::OpenDescendants {
            entity: "phase",
            id: id.to_string(),
            action: action.as_str(),
        }));
    }
    tx.execute(
        "UPDATE phases SET status = ?1 WHERE id = ?2",
        params![next_status.as_str(), id.to_string()],
    )?;
    tx.commit()?;
    Ok(Phase { status: next_status, ..current })
}

fn has_open_phase_descendants(connection: &Connection, id: PhaseId) -> Result<bool> {
    Ok(connection.query_row(
        "SELECT EXISTS (
             SELECT 1 FROM planning_tasks WHERE phase_id = ?1 AND status NOT IN ('completed', 'cancelled')
         )",
        [id.to_string()],
        |row| row.get(0),
    )?)
}

/// Check a structured plan against an existing persistent plan without writing.
pub fn planning_tasks(connection: &Connection, project_id: ProjectId) -> Result<Vec<PlanningTask>> {
    let mut statement = connection.prepare(
        "SELECT id, project_id, spec_id, plan_id, phase_id, parent_id, plan_key, title, body, status, priority, position, handoff, evidence
         FROM planning_tasks WHERE project_id = ?1
         ORDER BY COALESCE(phase_id, ''), COALESCE(plan_id, ''), COALESCE(spec_id, ''), COALESCE(parent_id, ''), position, id",
    )?;
    let rows = statement.query_map([project_id.to_string()], raw_task)?;
    rows.map(|row| decode_task(row?)).collect()
}

pub fn planning_task(connection: &Connection, id: TaskId) -> Result<Option<PlanningTask>> {
    let raw = connection.query_row(
        "SELECT id, project_id, spec_id, plan_id, phase_id, parent_id, plan_key, title, body, status, priority, position, handoff, evidence FROM planning_tasks WHERE id = ?1",
        [id.to_string()], raw_task,
    ).optional()?;
    raw.map(decode_task).transpose()
}

pub fn create_planning_task(connection: &mut Connection, create: PlanningTaskCreate) -> Result<PlanningTask> {
    let task = PlanningTask::new(
        create.project_id,
        create.spec_id,
        create.plan_id,
        create.phase_id,
        create.parent_id,
        create.title,
        create.body,
        create.priority,
        create.position,
    )
    .map_err(StorageError::InvalidPlanningTask)?;
    let tx = connection.transaction()?;
    validate_task_ancestry(&tx, &task)?;
    insert_task(&tx, &task)?;
    tx.commit()?;
    Ok(task)
}

/// Create a task and all requested blocking relationships in one transaction.
pub fn create_planning_task_with_dependencies(
    connection: &mut Connection, create: PlanningTaskCreate, blockers: &[TaskId],
) -> Result<PlanningTask> {
    let task = PlanningTask::new(
        create.project_id,
        create.spec_id,
        create.plan_id,
        create.phase_id,
        create.parent_id,
        create.title,
        create.body,
        create.priority,
        create.position,
    )
    .map_err(StorageError::InvalidPlanningTask)?;
    let tx = connection.transaction()?;
    validate_task_ancestry(&tx, &task)?;
    insert_task(&tx, &task)?;

    for blocker_id in blockers {
        let dependency = TaskDependency::new(task.id, *blocker_id).map_err(StorageError::InvalidPlanningDependency)?;
        ensure_task(&tx, create.project_id, *blocker_id)?;
        let cycle: bool = tx.query_row(
            "WITH RECURSIVE reachable(id) AS (
                 SELECT ?3
                 UNION
                 SELECT dependency.blocker_id
                 FROM planning_task_dependencies dependency
                 JOIN reachable ON dependency.task_id = reachable.id
                 WHERE dependency.project_id = ?1
             )
             SELECT EXISTS (SELECT 1 FROM reachable WHERE id = ?2)",
            params![
                create.project_id.to_string(),
                task.id.to_string(),
                blocker_id.to_string()
            ],
            |row| row.get(0),
        )?;
        if cycle {
            return Err(StorageError::InvalidPlanningDependency(DomainError::DependencyCycle {
                task: task.id.to_string(),
                blocker: blocker_id.to_string(),
            }));
        }
        tx.execute(
            "INSERT INTO planning_task_dependencies (project_id, task_id, blocker_id) VALUES (?1, ?2, ?3)",
            params![
                create.project_id.to_string(),
                dependency.task_id.to_string(),
                dependency.blocker_id.to_string()
            ],
        )?;
    }
    tx.commit()?;
    Ok(task)
}

pub fn update_planning_task(
    connection: &mut Connection, id: TaskId, update: PlanningTaskUpdate,
) -> Result<PlanningTask> {
    if update.title.is_none()
        && update.body.is_none()
        && update.priority.is_none()
        && update.position.is_none()
        && update.spec_id.is_none()
        && update.plan_id.is_none()
        && update.phase_id.is_none()
        && update.parent_id.is_none()
    {
        return Err(StorageError::InvalidPlanningTask(DomainError::NoFieldsToUpdate {
            entity: "task",
        }));
    }
    if let Some(title) = &update.title {
        validate_title(title).map_err(StorageError::InvalidPlanningTask)?;
    }
    if let Some(position) = update.position {
        validate_position(position).map_err(StorageError::InvalidPlanningTask)?;
    }
    let tx = connection.transaction()?;
    let current = planning_task(&tx, id)?.ok_or_else(|| StorageError::PlanningTaskNotFound { id: id.to_string() })?;
    let next = PlanningTask {
        id,
        project_id: current.project_id,
        spec_id: update.spec_id.unwrap_or(current.spec_id),
        plan_id: update.plan_id.unwrap_or(current.plan_id),
        phase_id: update.phase_id.unwrap_or(current.phase_id),
        parent_id: update.parent_id.unwrap_or(current.parent_id),
        plan_key: current.plan_key,
        title: update.title.unwrap_or(current.title),
        body: update.body.unwrap_or(current.body),
        status: current.status,
        priority: update.priority.unwrap_or(current.priority),
        position: update.position.unwrap_or(current.position),
        handoff: current.handoff,
        evidence: current.evidence,
    };
    validate_task_ancestry(&tx, &next)?;
    tx.execute(
        "UPDATE planning_tasks SET spec_id = ?1, plan_id = ?2, phase_id = ?3, parent_id = ?4, title = ?5, body = ?6, priority = ?7, position = ?8 WHERE id = ?9",
        params![next.spec_id.map(|x| x.to_string()), next.plan_id.map(|x| x.to_string()), next.phase_id.map(|x| x.to_string()), next.parent_id.map(|x| x.to_string()), next.title, next.body, next.priority.as_str(), next.position, next.id.to_string()],
    )?;
    tx.commit()?;
    Ok(next)
}

/// Read all connected-model dependency edges.
pub fn planning_dependencies(connection: &Connection, project_id: ProjectId) -> Result<Vec<TaskDependency>> {
    let mut statement = connection.prepare(
        "SELECT task_id, blocker_id FROM planning_task_dependencies
         WHERE project_id = ?1 ORDER BY task_id, blocker_id",
    )?;
    let rows = statement.query_map([project_id.to_string()], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    rows.map(|row| {
        let (task, blocker) = row?;
        let task_id = TaskId::parse(&task)
            .map_err(DomainError::from)
            .map_err(StorageError::InvalidPlanningDependency)?;
        let blocker_id = TaskId::parse(&blocker)
            .map_err(DomainError::from)
            .map_err(StorageError::InvalidPlanningDependency)?;
        TaskDependency::new(task_id, blocker_id).map_err(StorageError::InvalidPlanningDependency)
    })
    .collect()
}

/// Add a dependency between connected-model tasks.
pub fn add_planning_dependency(
    connection: &mut Connection, project_id: ProjectId, task_id: TaskId, blocker_id: TaskId,
) -> Result<TaskDependency> {
    let dependency = TaskDependency::new(task_id, blocker_id).map_err(StorageError::InvalidPlanningDependency)?;
    ensure_project(connection, project_id)?;
    let tx = connection.transaction()?;
    ensure_task(&tx, project_id, task_id)?;
    ensure_task(&tx, project_id, blocker_id)?;
    let duplicate: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM planning_task_dependencies WHERE project_id = ?1 AND task_id = ?2 AND blocker_id = ?3)",
        params![project_id.to_string(), task_id.to_string(), blocker_id.to_string()],
        |row| row.get(0),
    )?;
    if duplicate {
        return Err(StorageError::InvalidPlanningDependency(
            DomainError::DuplicateDependency { task: task_id.to_string(), blocker: blocker_id.to_string() },
        ));
    }
    let cycle: bool = tx.query_row(
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
    tx.execute(
        "INSERT INTO planning_task_dependencies (project_id, task_id, blocker_id) VALUES (?1, ?2, ?3)",
        params![project_id.to_string(), task_id.to_string(), blocker_id.to_string()],
    )?;
    tx.commit()?;
    Ok(dependency)
}

/// Remove a connected-model dependency.
pub fn remove_planning_dependency(
    connection: &mut Connection, project_id: ProjectId, task_id: TaskId, blocker_id: TaskId,
) -> Result<TaskDependency> {
    let dependency = TaskDependency::new(task_id, blocker_id).map_err(StorageError::InvalidPlanningDependency)?;
    let tx = connection.transaction()?;
    ensure_task(&tx, project_id, task_id)?;
    ensure_task(&tx, project_id, blocker_id)?;
    let count = tx.execute(
        "DELETE FROM planning_task_dependencies WHERE project_id = ?1 AND task_id = ?2 AND blocker_id = ?3",
        params![project_id.to_string(), task_id.to_string(), blocker_id.to_string()],
    )?;
    if count == 0 {
        return Err(StorageError::PlanningDependencyNotFound {
            task: task_id.to_string(),
            blocker: blocker_id.to_string(),
        });
    }
    tx.commit()?;
    Ok(dependency)
}

pub fn notes(connection: &Connection, project_id: ProjectId) -> Result<Vec<Note>> {
    let mut statement =
        connection.prepare("SELECT id, project_id, title, body FROM notes WHERE project_id = ?1 ORDER BY id")?;
    let rows = statement.query_map([project_id.to_string()], raw_note)?;
    rows.map(|row| decode_note(row?)).collect()
}

pub fn note(connection: &Connection, id: NoteId) -> Result<Option<Note>> {
    let raw = connection
        .query_row(
            "SELECT id, project_id, title, body FROM notes WHERE id = ?1",
            [id.to_string()],
            raw_note,
        )
        .optional()?;
    raw.map(decode_note).transpose()
}

pub fn create_note(connection: &mut Connection, project_id: ProjectId, title: String, body: String) -> Result<Note> {
    ensure_project(connection, project_id)?;
    let note = Note::new(project_id, title, body).map_err(StorageError::InvalidNote)?;
    let tx = connection.transaction()?;
    tx.execute(
        "INSERT INTO notes (id, project_id, title, body) VALUES (?1, ?2, ?3, ?4)",
        params![note.id.to_string(), project_id.to_string(), note.title, note.body],
    )?;
    tx.commit()?;
    Ok(note)
}

pub fn update_note(connection: &mut Connection, id: NoteId, update: NoteUpdate) -> Result<Note> {
    if update.title.is_none() && update.body.is_none() {
        return Err(StorageError::InvalidNote(DomainError::NoFieldsToUpdate {
            entity: "note",
        }));
    }
    if let Some(title) = &update.title {
        validate_title(title).map_err(StorageError::InvalidNote)?;
    }
    let tx = connection.transaction()?;
    let current = note(&tx, id)?.ok_or_else(|| StorageError::NoteNotFound { id: id.to_string() })?;
    let title = update.title.as_deref().unwrap_or(&current.title);
    let body = update.body.as_deref().unwrap_or(&current.body);
    tx.execute(
        "UPDATE notes SET title = ?1, body = ?2 WHERE id = ?3",
        params![title, body, id.to_string()],
    )?;
    tx.commit()?;
    Ok(Note { title: title.to_owned(), body: body.to_owned(), ..current })
}

pub fn release_memberships(connection: &Connection, project_id: ProjectId) -> Result<Vec<ReleaseMembership>> {
    let mut statement = connection.prepare("SELECT project_id, release_id, record_kind, record_id FROM release_memberships WHERE project_id = ?1 ORDER BY release_id, record_kind, record_id")?;
    let rows = statement.query_map([project_id.to_string()], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    rows.map(|row| {
        let (project, release, kind, id) = row?;
        decode_membership(&project, &release, &kind, &id)
    })
    .collect()
}

/// Add a single release edge; descendants are not included.
pub fn add_release_membership(
    connection: &mut Connection, project_id: ProjectId, release_id: ReleaseId, kind: ReleaseMemberKind,
    record_id: String,
) -> Result<ReleaseMembership> {
    ensure_project(connection, project_id)?;
    let tx = connection.transaction()?;
    ensure_release(&tx, project_id, release_id)?;
    ensure_member(&tx, project_id, kind, &record_id)?;
    let membership = ReleaseMembership { project_id, release_id, record_kind: kind, record_id };
    tx.execute(
        "INSERT INTO release_memberships (project_id, release_id, record_kind, record_id) VALUES (?1, ?2, ?3, ?4)",
        params![
            project_id.to_string(),
            release_id.to_string(),
            kind.as_str(),
            membership.record_id
        ],
    )?;
    tx.commit()?;
    Ok(membership)
}

/// Remove a single release edge.
pub fn remove_release_membership(
    connection: &mut Connection, project_id: ProjectId, release_id: ReleaseId, kind: ReleaseMemberKind, record_id: &str,
) -> Result<ReleaseMembership> {
    let tx = connection.transaction()?;
    ensure_release(&tx, project_id, release_id)?;
    ensure_member(&tx, project_id, kind, record_id)?;
    let count = tx.execute("DELETE FROM release_memberships WHERE project_id = ?1 AND release_id = ?2 AND record_kind = ?3 AND record_id = ?4", params![project_id.to_string(), release_id.to_string(), kind.as_str(), record_id])?;
    if count == 0 {
        return Err(StorageError::ReleaseMembershipNotFound {
            release_id: release_id.to_string(),
            record_id: record_id.to_owned(),
        });
    }
    tx.commit()?;
    Ok(ReleaseMembership { project_id, release_id, record_kind: kind, record_id: record_id.to_owned() })
}

/// Add a note link to a record in the same project.
pub fn add_note_link(
    connection: &mut Connection, project_id: ProjectId, note_id: NoteId, kind: LinkedRecordKind, record_id: String,
) -> Result<NoteLink> {
    ensure_project(connection, project_id)?;
    let tx = connection.transaction()?;
    ensure_note(&tx, project_id, note_id)?;
    ensure_link_target(&tx, project_id, kind, &record_id)?;
    let link = NoteLink { project_id, note_id, record_kind: kind, record_id };
    tx.execute("INSERT INTO record_links (project_id, source_kind, source_id, target_kind, target_id) VALUES (?1, 'note', ?2, ?3, ?4)", params![project_id.to_string(), note_id.to_string(), kind.as_str(), link.record_id])?;
    tx.commit()?;
    Ok(link)
}

/// Remove a note link.
pub fn remove_note_link(
    connection: &mut Connection, project_id: ProjectId, note_id: NoteId, kind: LinkedRecordKind, record_id: &str,
) -> Result<NoteLink> {
    let tx = connection.transaction()?;
    ensure_note(&tx, project_id, note_id)?;
    ensure_link_target(&tx, project_id, kind, record_id)?;
    let count = tx.execute("DELETE FROM record_links WHERE project_id = ?1 AND source_kind = 'note' AND source_id = ?2 AND target_kind = ?3 AND target_id = ?4", params![project_id.to_string(), note_id.to_string(), kind.as_str(), record_id])?;
    if count == 0 {
        return Err(StorageError::NoteLinkNotFound { note: note_id.to_string(), record: record_id.to_owned() });
    }
    tx.commit()?;
    Ok(NoteLink { project_id, note_id, record_kind: kind, record_id: record_id.to_owned() })
}

/// Add a relationship between any two records in the same project.
pub fn add_record_link(
    connection: &mut Connection, project_id: ProjectId, source_kind: LinkedRecordKind, source_id: String,
    target_kind: LinkedRecordKind, target_id: String,
) -> Result<RecordLink> {
    ensure_project(connection, project_id)?;
    let tx = connection.transaction()?;
    ensure_link_target(&tx, project_id, source_kind, &source_id)?;
    ensure_link_target(&tx, project_id, target_kind, &target_id)?;
    tx.execute(
        "INSERT INTO record_links (project_id, source_kind, source_id, target_kind, target_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            project_id.to_string(),
            source_kind.as_str(),
            source_id,
            target_kind.as_str(),
            target_id
        ],
    )?;
    tx.commit()?;
    Ok(RecordLink { project_id, source_kind, source_id, target_kind, target_id })
}

/// Read all generic record relationships in deterministic order.
pub fn record_links(connection: &Connection, project_id: ProjectId) -> Result<Vec<RecordLink>> {
    let mut statement = connection.prepare(
        "SELECT project_id, source_kind, source_id, target_kind, target_id
         FROM record_links WHERE project_id = ?1
         ORDER BY source_kind, source_id, target_kind, target_id",
    )?;
    let rows = statement.query_map([project_id.to_string()], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    rows.map(|row| {
        let (project, source_kind, source_id, target_kind, target_id) = row?;
        Ok(RecordLink {
            project_id: ProjectId::parse(&project)
                .map_err(DomainError::from)
                .map_err(StorageError::InvalidLink)?,
            source_kind: linked_kind(&source_kind)?,
            source_id,
            target_kind: linked_kind(&target_kind)?,
            target_id,
        })
    })
    .collect()
}

/// Remove a relationship between two records.
pub fn remove_record_link(
    connection: &mut Connection, project_id: ProjectId, source_kind: LinkedRecordKind, source_id: &str,
    target_kind: LinkedRecordKind, target_id: &str,
) -> Result<RecordLink> {
    let tx = connection.transaction()?;
    ensure_link_target(&tx, project_id, source_kind, source_id)?;
    ensure_link_target(&tx, project_id, target_kind, target_id)?;
    let count = tx.execute(
        "DELETE FROM record_links
         WHERE project_id = ?1 AND source_kind = ?2 AND source_id = ?3
           AND target_kind = ?4 AND target_id = ?5",
        params![
            project_id.to_string(),
            source_kind.as_str(),
            source_id,
            target_kind.as_str(),
            target_id
        ],
    )?;
    if count == 0 {
        return Err(StorageError::RecordLinkNotFound {
            source_id: source_id.to_owned(),
            target_id: target_id.to_owned(),
        });
    }
    tx.commit()?;
    Ok(RecordLink {
        project_id,
        source_kind,
        source_id: source_id.to_owned(),
        target_kind,
        target_id: target_id.to_owned(),
    })
}

pub fn note_links(connection: &Connection, project_id: ProjectId) -> Result<Vec<NoteLink>> {
    let mut statement = connection.prepare("SELECT project_id, source_id, target_kind, target_id FROM record_links WHERE project_id = ?1 AND source_kind = 'note' ORDER BY source_id, target_kind, target_id")?;
    let rows = statement.query_map([project_id.to_string()], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    rows.map(|row| {
        let (project, note, kind, record) = row?;
        let project_id = ProjectId::parse(&project)
            .map_err(DomainError::from)
            .map_err(StorageError::InvalidLink)?;
        let note_id = NoteId::parse(&note)
            .map_err(DomainError::from)
            .map_err(StorageError::InvalidLink)?;
        Ok(NoteLink { project_id, note_id, record_kind: linked_kind(&kind)?, record_id: record })
    })
    .collect()
}

fn raw_capture(row: &rusqlite::Row<'_>) -> rusqlite::Result<(String, String, String, String, String, String)> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}
fn decode_capture(raw: (String, String, String, String, String, String)) -> Result<Capture> {
    let (id, project, title, body, status, created_at) = raw;
    let id = CaptureId::parse(&id)
        .map_err(DomainError::from)
        .map_err(StorageError::InvalidCapture)?;
    let project_id = ProjectId::parse(&project)
        .map_err(DomainError::from)
        .map_err(StorageError::InvalidCapture)?;
    let status = CaptureStatus::parse(&status).map_err(StorageError::InvalidCapture)?;
    let mut value = Capture::new(project_id, title, body, created_at).map_err(StorageError::InvalidCapture)?;
    value.id = id;
    value.status = status;
    Ok(value)
}
type RawSpec = (String, String, String, String, String, String, Option<String>);

fn raw_spec(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawSpec> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
    ))
}
fn decode_spec(raw: (String, String, String, String, String, String, Option<String>)) -> Result<Spec> {
    let (id, project, title, body, acceptance, status, source) = raw;
    let id = SpecId::parse(&id)
        .map_err(DomainError::from)
        .map_err(StorageError::InvalidSpec)?;
    let project_id = ProjectId::parse(&project)
        .map_err(DomainError::from)
        .map_err(StorageError::InvalidSpec)?;
    let status = ContainerStatus::parse("spec", &status).map_err(StorageError::InvalidSpec)?;
    let source_capture_id = source
        .map(|x| {
            CaptureId::parse(&x)
                .map_err(DomainError::from)
                .map_err(StorageError::InvalidSpec)
        })
        .transpose()?;
    let mut value = Spec::new(project_id, title, body, acceptance).map_err(StorageError::InvalidSpec)?;
    value.id = id;
    value.status = status;
    value.source_capture_id = source_capture_id;
    Ok(value)
}
fn raw_plan(row: &rusqlite::Row<'_>) -> rusqlite::Result<(String, String, String, String, String, String)> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}
fn decode_plan(raw: (String, String, String, String, String, String)) -> Result<Plan> {
    let (id, project, spec, title, body, status) = raw;
    let id = PlanId::parse(&id)
        .map_err(DomainError::from)
        .map_err(StorageError::InvalidPlan)?;
    let project_id = ProjectId::parse(&project)
        .map_err(DomainError::from)
        .map_err(StorageError::InvalidPlan)?;
    let spec_id = SpecId::parse(&spec)
        .map_err(DomainError::from)
        .map_err(StorageError::InvalidPlan)?;
    let status = ContainerStatus::parse("plan", &status).map_err(StorageError::InvalidPlan)?;
    let mut value = Plan::new(project_id, spec_id, title, body).map_err(StorageError::InvalidPlan)?;
    value.id = id;
    value.status = status;
    Ok(value)
}
fn raw_phase(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawPhase> {
    Ok(RawPhase {
        id: row.get(0)?,
        project: row.get(1)?,
        plan: row.get(2)?,
        plan_key: row.get(3)?,
        title: row.get(4)?,
        body: row.get(5)?,
        status: row.get(6)?,
        position: row.get(7)?,
    })
}
fn decode_phase(raw: RawPhase) -> Result<Phase> {
    let RawPhase { id, project, plan, plan_key, title, body, status, position } = raw;
    let id = PhaseId::parse(&id)
        .map_err(DomainError::from)
        .map_err(StorageError::InvalidPhase)?;
    let project_id = ProjectId::parse(&project)
        .map_err(DomainError::from)
        .map_err(StorageError::InvalidPhase)?;
    let plan_id = PlanId::parse(&plan)
        .map_err(DomainError::from)
        .map_err(StorageError::InvalidPhase)?;
    let status = ContainerStatus::parse("phase", &status).map_err(StorageError::InvalidPhase)?;
    let mut value = Phase::new(project_id, plan_id, title, body, position).map_err(StorageError::InvalidPhase)?;
    value.id = id;
    value.plan_key = plan_key;
    value.status = status;
    Ok(value)
}

fn raw_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawTask> {
    Ok(RawTask {
        id: row.get(0)?,
        project: row.get(1)?,
        spec: row.get(2)?,
        plan: row.get(3)?,
        phase: row.get(4)?,
        parent: row.get(5)?,
        plan_key: row.get(6)?,
        title: row.get(7)?,
        body: row.get(8)?,
        status: row.get(9)?,
        priority: row.get(10)?,
        position: row.get(11)?,
        handoff: row.get(12)?,
        evidence: row.get(13)?,
    })
}
fn decode_task(raw: RawTask) -> Result<PlanningTask> {
    let id = TaskId::parse(&raw.id)
        .map_err(DomainError::from)
        .map_err(StorageError::InvalidPlanningTask)?;
    let project_id = ProjectId::parse(&raw.project)
        .map_err(DomainError::from)
        .map_err(StorageError::InvalidPlanningTask)?;
    let spec_id = parse_id(raw.spec, SpecId::parse).map_err(StorageError::InvalidPlanningTask)?;
    let plan_id = parse_id(raw.plan, PlanId::parse).map_err(StorageError::InvalidPlanningTask)?;
    let phase_id = parse_id(raw.phase, PhaseId::parse).map_err(StorageError::InvalidPlanningTask)?;
    let parent_id = parse_id(raw.parent, TaskId::parse).map_err(StorageError::InvalidPlanningTask)?;
    let status = TaskStatus::from_str(&raw.status).map_err(StorageError::InvalidPlanningTask)?;
    let priority = TaskPriority::parse(&raw.priority).map_err(StorageError::InvalidPlanningTask)?;
    Ok(PlanningTask {
        id,
        project_id,
        spec_id,
        plan_id,
        phase_id,
        parent_id,
        plan_key: raw.plan_key,
        title: raw.title,
        body: raw.body,
        status,
        priority,
        position: raw.position,
        handoff: raw.handoff,
        evidence: raw.evidence,
    })
}
fn parse_id<T>(
    value: Option<String>, parse: impl Fn(&str) -> std::result::Result<T, arcl_core::domain::IdError>,
) -> std::result::Result<Option<T>, DomainError> {
    value.map(|x| parse(&x).map_err(DomainError::from)).transpose()
}
fn raw_note(row: &rusqlite::Row<'_>) -> rusqlite::Result<(String, String, String, String)> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
}
fn decode_note(raw: (String, String, String, String)) -> Result<Note> {
    let (id, project, title, body) = raw;
    let id = NoteId::parse(&id)
        .map_err(DomainError::from)
        .map_err(StorageError::InvalidNote)?;
    let project_id = ProjectId::parse(&project)
        .map_err(DomainError::from)
        .map_err(StorageError::InvalidNote)?;
    let mut value = Note::new(project_id, title, body).map_err(StorageError::InvalidNote)?;
    value.id = id;
    Ok(value)
}

fn require_open_container(
    connection: &Connection, table: &str, id: &str, entity: &'static str, error: fn(DomainError) -> StorageError,
) -> Result<()> {
    let status: String = connection.query_row(&format!("SELECT status FROM {table} WHERE id = ?1"), [id], |row| {
        row.get(0)
    })?;
    let status = ContainerStatus::parse(entity, &status).map_err(error)?;
    if status != ContainerStatus::Open {
        return Err(error(DomainError::InvalidContainerState {
            entity,
            id: id.to_owned(),
            status: status.as_str().to_owned(),
        }));
    }
    Ok(())
}

fn ensure_project(connection: &Connection, id: ProjectId) -> Result<()> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS (SELECT 1 FROM projects WHERE id = ?1)",
        [id.to_string()],
        |row| row.get(0),
    )?;
    if exists { Ok(()) } else { Err(StorageError::ProjectNotFound) }
}
fn ensure_record(
    connection: &Connection, table: &str, project: ProjectId, id: &str, missing: StorageError,
) -> Result<()> {
    let sql = format!("SELECT project_id FROM {table} WHERE id = ?1");
    let value: Option<String> = connection.query_row(&sql, [id], |row| row.get(0)).optional()?;
    match value {
        Some(found) if found == project.to_string() => Ok(()),
        Some(found) => Err(StorageError::InvalidPlanningTask(DomainError::DifferentProject {
            entity: table.to_owned(),
            id: id.to_owned(),
            related: found,
        })),
        None => Err(missing),
    }
}
fn ensure_capture(c: &Connection, p: ProjectId, id: CaptureId) -> Result<()> {
    ensure_record(
        c,
        "captures",
        p,
        &id.to_string(),
        StorageError::CaptureNotFound { id: id.to_string() },
    )
}
fn ensure_spec(c: &Connection, p: ProjectId, id: SpecId) -> Result<()> {
    ensure_record(
        c,
        "specs",
        p,
        &id.to_string(),
        StorageError::SpecNotFound { id: id.to_string() },
    )
}
fn ensure_plan(c: &Connection, p: ProjectId, id: PlanId) -> Result<()> {
    ensure_record(
        c,
        "plans",
        p,
        &id.to_string(),
        StorageError::PlanNotFound { id: id.to_string() },
    )
}
fn ensure_phase(c: &Connection, p: ProjectId, id: PhaseId) -> Result<()> {
    ensure_record(
        c,
        "phases",
        p,
        &id.to_string(),
        StorageError::PhaseNotFound { id: id.to_string() },
    )
}
fn ensure_task(c: &Connection, p: ProjectId, id: TaskId) -> Result<()> {
    ensure_record(
        c,
        "planning_tasks",
        p,
        &id.to_string(),
        StorageError::PlanningTaskNotFound { id: id.to_string() },
    )
}
fn ensure_note(c: &Connection, p: ProjectId, id: NoteId) -> Result<()> {
    ensure_record(
        c,
        "notes",
        p,
        &id.to_string(),
        StorageError::NoteNotFound { id: id.to_string() },
    )
}
fn ensure_release(c: &Connection, p: ProjectId, id: ReleaseId) -> Result<()> {
    ensure_record(
        c,
        "releases",
        p,
        &id.to_string(),
        StorageError::ReleaseNotFound { id: id.to_string() },
    )
}

fn validate_task_ancestry(c: &Connection, task: &PlanningTask) -> Result<()> {
    execution::validate_task_graph(c, task.project_id, task, true)
}

pub(super) fn validate_task_graph_candidates(
    c: &Connection, project_id: ProjectId, candidates: &[PlanningTask], require_open_containers: bool,
) -> Result<()> {
    execution::validate_task_graph_candidates(c, project_id, candidates, require_open_containers)
}

fn insert_task(c: &Connection, task: &PlanningTask) -> Result<()> {
    c.execute("INSERT INTO planning_tasks (id, project_id, spec_id, plan_id, phase_id, parent_id, plan_key, title, body, status, priority, position, handoff, evidence) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)", params![task.id.to_string(), task.project_id.to_string(), task.spec_id.map(|x|x.to_string()), task.plan_id.map(|x|x.to_string()), task.phase_id.map(|x|x.to_string()), task.parent_id.map(|x|x.to_string()), task.plan_key, task.title, task.body, task.status.as_str(), task.priority.as_str(), task.position, task.handoff, task.evidence])?;
    Ok(())
}
fn insert_phase(c: &Connection, phase: &Phase) -> Result<()> {
    c.execute("INSERT INTO phases (id, project_id, plan_id, plan_key, title, body, status, position) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)", params![phase.id.to_string(), phase.project_id.to_string(), phase.plan_id.to_string(), phase.plan_key, phase.title, phase.body, phase.status.as_str(), phase.position])?;
    Ok(())
}

fn member_kind(value: &str) -> Result<ReleaseMemberKind> {
    match value {
        "spec" => Ok(ReleaseMemberKind::Spec),
        "plan" => Ok(ReleaseMemberKind::Plan),
        "task" => Ok(ReleaseMemberKind::Task),
        "note" => Ok(ReleaseMemberKind::Note),
        _ => Err(StorageError::InvalidMembership(DomainError::InvalidStatus {
            entity: "release member",
            value: value.to_owned(),
        })),
    }
}
fn linked_kind(value: &str) -> Result<LinkedRecordKind> {
    match value {
        "capture" => Ok(LinkedRecordKind::Capture),
        "spec" => Ok(LinkedRecordKind::Spec),
        "plan" => Ok(LinkedRecordKind::Plan),
        "phase" => Ok(LinkedRecordKind::Phase),
        "task" => Ok(LinkedRecordKind::Task),
        "note" => Ok(LinkedRecordKind::Note),
        "release" => Ok(LinkedRecordKind::Release),
        _ => Err(StorageError::InvalidLink(DomainError::InvalidStatus {
            entity: "linked record",
            value: value.to_owned(),
        })),
    }
}
fn decode_membership(project: &str, release: &str, kind: &str, record: &str) -> Result<ReleaseMembership> {
    Ok(ReleaseMembership {
        project_id: ProjectId::parse(project)
            .map_err(DomainError::from)
            .map_err(StorageError::InvalidMembership)?,
        release_id: ReleaseId::parse(release)
            .map_err(DomainError::from)
            .map_err(StorageError::InvalidMembership)?,
        record_kind: member_kind(kind)?,
        record_id: record.to_owned(),
    })
}
fn ensure_member(c: &Connection, p: ProjectId, kind: ReleaseMemberKind, id: &str) -> Result<()> {
    match kind {
        ReleaseMemberKind::Spec => ensure_spec(
            c,
            p,
            SpecId::parse(id)
                .map_err(DomainError::from)
                .map_err(StorageError::InvalidMembership)?,
        ),
        ReleaseMemberKind::Plan => ensure_plan(
            c,
            p,
            PlanId::parse(id)
                .map_err(DomainError::from)
                .map_err(StorageError::InvalidMembership)?,
        ),
        ReleaseMemberKind::Task => ensure_task(
            c,
            p,
            TaskId::parse(id)
                .map_err(DomainError::from)
                .map_err(StorageError::InvalidMembership)?,
        ),
        ReleaseMemberKind::Note => ensure_note(
            c,
            p,
            NoteId::parse(id)
                .map_err(DomainError::from)
                .map_err(StorageError::InvalidMembership)?,
        ),
    }
}
fn ensure_link_target(c: &Connection, p: ProjectId, kind: LinkedRecordKind, id: &str) -> Result<()> {
    match kind {
        LinkedRecordKind::Capture => ensure_capture(
            c,
            p,
            CaptureId::parse(id)
                .map_err(DomainError::from)
                .map_err(StorageError::InvalidLink)?,
        ),
        LinkedRecordKind::Spec => ensure_spec(
            c,
            p,
            SpecId::parse(id)
                .map_err(DomainError::from)
                .map_err(StorageError::InvalidLink)?,
        ),
        LinkedRecordKind::Plan => ensure_plan(
            c,
            p,
            PlanId::parse(id)
                .map_err(DomainError::from)
                .map_err(StorageError::InvalidLink)?,
        ),
        LinkedRecordKind::Phase => ensure_phase(
            c,
            p,
            PhaseId::parse(id)
                .map_err(DomainError::from)
                .map_err(StorageError::InvalidLink)?,
        ),
        LinkedRecordKind::Task => ensure_task(
            c,
            p,
            TaskId::parse(id)
                .map_err(DomainError::from)
                .map_err(StorageError::InvalidLink)?,
        ),
        LinkedRecordKind::Note => ensure_note(
            c,
            p,
            NoteId::parse(id)
                .map_err(DomainError::from)
                .map_err(StorageError::InvalidLink)?,
        ),
        LinkedRecordKind::Release => ensure_release(
            c,
            p,
            ReleaseId::parse(id)
                .map_err(DomainError::from)
                .map_err(StorageError::InvalidLink)?,
        ),
    }
}

fn has_parent_cycle(tasks: &[PlanningTask], start: TaskId) -> bool {
    let mut seen = HashSet::new();
    let mut current = Some(start);
    while let Some(id) = current {
        if !seen.insert(id) {
            return true;
        }
        current = tasks.iter().find(|task| task.id == id).and_then(|task| task.parent_id);
    }
    false
}

fn has_dependency_cycle(dependencies: &[TaskDependency], start: TaskId) -> bool {
    fn visit(
        dependencies: &[TaskDependency], current: TaskId, visited: &mut HashSet<TaskId>, active: &mut HashSet<TaskId>,
    ) -> bool {
        if active.contains(&current) {
            return true;
        }
        if !visited.insert(current) {
            return false;
        }
        active.insert(current);
        let cycle = dependencies
            .iter()
            .filter(|edge| edge.task_id == current)
            .any(|edge| visit(dependencies, edge.blocker_id, visited, active));
        active.remove(&current);
        cycle
    }

    visit(dependencies, start, &mut HashSet::new(), &mut HashSet::new())
}

#[cfg(test)]
mod tests {

    use crate::storage::{
        CapturePromotionInput, CapturePromotionRecord, CaptureTaskPromotion, Database, PlanningTaskCreate, StorageError,
    };
    use arcl_core::domain::{
        ContainerAction, DomainError, LinkedRecordKind, ProjectId, ReleaseMemberKind, TaskPriority,
    };
    use arcl_core::plan::{PlanDocument, PlanPhase, PlanTask};

    #[test]
    fn connected_records_keep_markdown_and_allow_each_task_placement() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let capture = database
            .create_capture("Thought".to_owned(), "Capture **body**".to_owned())
            .expect("capture creates");
        assert_eq!(capture.body, "Capture **body**");
        let spec = database
            .create_spec("Spec".to_owned(), "# Spec".to_owned(), "- [ ] criterion".to_owned())
            .expect("spec creates");
        let plan = database
            .create_plan(spec.id, "Plan".to_owned(), "Plan body".to_owned())
            .expect("plan creates");
        let phase = database
            .create_phase(plan.id, "Phase".to_owned(), "Phase body".to_owned(), 0)
            .expect("phase creates");
        let project_task = database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: None,
                plan_id: None,
                phase_id: None,
                parent_id: None,
                title: "Project task".to_owned(),
                body: "Task body".to_owned(),
                priority: TaskPriority::Normal,
                position: 0,
            })
            .expect("project task creates");
        let spec_task = database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: Some(spec.id),
                plan_id: None,
                phase_id: None,
                parent_id: None,
                title: "Spec task".to_owned(),
                body: String::new(),
                priority: TaskPriority::Normal,
                position: 0,
            })
            .expect("spec task creates");
        let plan_task = database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: Some(spec.id),
                plan_id: Some(plan.id),
                phase_id: None,
                parent_id: None,
                title: "Plan task".to_owned(),
                body: String::new(),
                priority: TaskPriority::Normal,
                position: 0,
            })
            .expect("plan task creates");
        let phase_task = database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: Some(spec.id),
                plan_id: Some(plan.id),
                phase_id: Some(phase.id),
                parent_id: None,
                title: "Phase task".to_owned(),
                body: String::new(),
                priority: TaskPriority::Normal,
                position: 0,
            })
            .expect("phase task creates");
        let child = database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: Some(spec.id),
                plan_id: Some(plan.id),
                phase_id: Some(phase.id),
                parent_id: Some(phase_task.id),
                title: "Child".to_owned(),
                body: String::new(),
                priority: TaskPriority::Low,
                position: 0,
            })
            .expect("child creates");
        assert_eq!(child.parent_id, Some(phase_task.id));
        assert_eq!(database.planning_tasks().expect("tasks list").len(), 5);
        assert_eq!(
            database
                .planning_task(project_task.id)
                .expect("task reads")
                .expect("task exists")
                .body,
            "Task body"
        );
        assert_eq!(
            database
                .planning_task(spec_task.id)
                .expect("task reads")
                .expect("task exists")
                .spec_id,
            Some(spec.id)
        );
        assert_eq!(
            database
                .planning_task(plan_task.id)
                .expect("task reads")
                .expect("task exists")
                .plan_id,
            Some(plan.id)
        );

        let second_spec = database
            .create_spec("Other".to_owned(), String::new(), String::new())
            .expect("second spec creates");
        let second_plan = database
            .create_plan(second_spec.id, "Other plan".to_owned(), String::new())
            .expect("second plan creates");
        let error = database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: Some(spec.id),
                plan_id: Some(second_plan.id),
                phase_id: None,
                parent_id: None,
                title: "Invalid".to_owned(),
                body: String::new(),
                priority: TaskPriority::Normal,
                position: 0,
            })
            .expect_err("contradictory ancestry rejects");
        assert!(matches!(
            error,
            StorageError::InvalidPlanningTask(DomainError::ContradictoryAncestry { .. })
        ));

        let other_project = ProjectId::new();
        database
            .connection()
            .execute(
                "INSERT INTO projects (id, name) VALUES (?1, 'Other')",
                [other_project.to_string()],
            )
            .expect("second project inserts");
        let error = database
            .create_planning_task(PlanningTaskCreate {
                project_id: other_project,
                spec_id: None,
                plan_id: None,
                phase_id: None,
                parent_id: Some(project_task.id),
                title: "Cross-project child".to_owned(),
                body: String::new(),
                priority: TaskPriority::Normal,
                position: 0,
            })
            .expect_err("cross-project ancestry rejects");
        assert!(matches!(
            error,
            StorageError::InvalidPlanningTask(DomainError::DifferentProject { .. })
        ));
    }

    #[test]
    fn capture_promotion_preserves_provenance_and_rejects_a_second_destination() {
        let mut database = Database::open_in_memory().expect("database opens");
        let capture = database
            .create_capture("Captured idea".to_owned(), "Original **Markdown**".to_owned())
            .expect("capture creates");
        let first = database
            .promote_capture(
                capture.id,
                CapturePromotionInput::Spec {
                    title: "Owned spec".to_owned(),
                    body: "Spec body".to_owned(),
                    acceptance_criteria: "- [ ] works".to_owned(),
                },
            )
            .expect("capture promotes");
        let CapturePromotionRecord::Spec(spec) = &first.record else {
            panic!("promotion creates a spec");
        };
        assert_eq!(first.capture.status.as_str(), "promoted");
        assert_eq!(spec.source_capture_id, Some(capture.id));
        assert_eq!(database.capture_promotions().expect("provenance reads").len(), 1);

        let repeated = database
            .promote_capture(
                capture.id,
                CapturePromotionInput::Spec {
                    title: "Ignored title".to_owned(),
                    body: "Ignored body".to_owned(),
                    acceptance_criteria: String::new(),
                },
            )
            .expect("same destination is idempotent");
        let CapturePromotionRecord::Spec(repeated_spec) = repeated.record else {
            panic!("repeated promotion returns a spec");
        };
        assert_eq!(repeated_spec.id, spec.id);
        assert_eq!(database.specs().expect("specs read").len(), 1);

        let error = database
            .promote_capture(
                capture.id,
                CapturePromotionInput::Note { title: "Other destination".to_owned(), body: String::new() },
            )
            .expect_err("a second destination is ambiguous");
        assert!(matches!(error, StorageError::AmbiguousCapturePromotion { .. }));
        assert!(database.notes().expect("notes read").is_empty());
    }

    #[test]
    fn capture_to_task_and_note_are_atomic_and_keep_the_source_content() {
        let mut database = Database::open_in_memory().expect("database opens");
        let spec = database
            .create_spec("Spec".to_owned(), String::new(), String::new())
            .expect("spec creates");
        let capture = database
            .create_capture("Task thought".to_owned(), "Task details".to_owned())
            .expect("capture creates");
        let promoted = database
            .promote_capture_to_task(
                capture.id,
                CaptureTaskPromotion {
                    spec_id: Some(spec.id),
                    plan_id: None,
                    phase_id: None,
                    parent_id: None,
                    title: capture.title.clone(),
                    body: capture.body.clone(),
                    priority: TaskPriority::High,
                    position: 0,
                },
            )
            .expect("capture promotes to task");
        assert!(matches!(promoted.record, CapturePromotionRecord::Task(_)));
        assert_eq!(database.planning_tasks().expect("tasks read").len(), 1);
        assert_eq!(
            database
                .capture(capture.id)
                .expect("capture reads")
                .expect("capture exists")
                .body,
            capture.body
        );

        let note_capture = database
            .create_capture("Note thought".to_owned(), "Note details".to_owned())
            .expect("capture creates");
        let note = database
            .promote_capture_to_note(note_capture.id, note_capture.title.clone(), note_capture.body.clone())
            .expect("capture promotes to note");
        assert!(matches!(note.record, CapturePromotionRecord::Note(_)));
        assert_eq!(database.notes().expect("notes read").len(), 1);

        let invalid = database
            .create_capture("Invalid".to_owned(), String::new())
            .expect("capture creates");
        let error = database
            .promote_capture(
                invalid.id,
                CapturePromotionInput::Task(CaptureTaskPromotion {
                    spec_id: Some(spec.id),
                    plan_id: None,
                    phase_id: Some(arcl_core::domain::PhaseId::new()),
                    parent_id: None,
                    title: "Invalid".to_owned(),
                    body: String::new(),
                    priority: TaskPriority::Normal,
                    position: 0,
                }),
            )
            .expect_err("invalid ancestry rejects");
        assert!(matches!(error, StorageError::PhaseNotFound { .. }));
        assert_eq!(
            database
                .capture(invalid.id)
                .expect("capture reads")
                .expect("capture exists")
                .status
                .as_str(),
            "captured"
        );
        assert_eq!(database.planning_tasks().expect("tasks read").len(), 1);
        assert_eq!(spec.project_id, database.project().expect("project remains").id);
    }

    #[test]
    fn structured_plan_diff_and_apply_are_idempotent() {
        let mut database = Database::open_in_memory().expect("database opens");
        let spec = database
            .create_spec("Spec".to_owned(), String::new(), String::new())
            .expect("spec creates");
        let plan = database
            .create_plan(spec.id, "Plan".to_owned(), "Plan body".to_owned())
            .expect("plan creates");
        let document = PlanDocument {
            format_version: 1,
            phases: vec![PlanPhase {
                key: "storage".to_owned(),
                title: "Storage".to_owned(),
                position: 0,
                tasks: vec![
                    PlanTask {
                        key: "schema".to_owned(),
                        title: "Update schema".to_owned(),
                        priority: Some(TaskPriority::High),
                        position: 0,
                        blocked_by: Vec::new(),
                        subtasks: Vec::new(),
                    },
                    PlanTask {
                        key: "verify".to_owned(),
                        title: "Verify changes".to_owned(),
                        priority: None,
                        position: 1,
                        blocked_by: vec!["storage/schema".to_owned()],
                        subtasks: Vec::new(),
                    },
                ],
            }],
        };
        let first_diff = database.diff_plan(plan.id, &document).expect("diff succeeds");
        assert!(
            first_diff
                .phases
                .iter()
                .all(|item| item.change == super::PlanChange::Create)
        );
        assert_eq!(first_diff.tasks.len(), 2);
        let first = database.apply_plan(plan.id, &document).expect("plan applies");
        assert_eq!(first.phases.len(), 1);
        assert_eq!(first.tasks.len(), 2);
        assert_eq!(first.dependencies.len(), 1);

        let second_diff = database.check_plan(plan.id, &document).expect("check succeeds");
        assert!(
            second_diff
                .phases
                .iter()
                .all(|item| item.change == super::PlanChange::Unchanged)
        );
        assert!(
            second_diff
                .tasks
                .iter()
                .all(|item| item.change == super::PlanChange::Unchanged)
        );
        assert!(
            second_diff
                .dependencies
                .iter()
                .all(|item| item.change == super::PlanChange::Unchanged)
        );
        database.apply_plan(plan.id, &document).expect("repeated plan applies");
        assert_eq!(database.phases().expect("phases read").len(), 1);
        assert_eq!(database.planning_tasks().expect("tasks read").len(), 2);
        assert_eq!(database.planning_dependencies().expect("dependencies read").len(), 1);

        let mut invalid = document.clone();
        invalid.phases[0].tasks[0].blocked_by = vec!["does-not-exist".to_owned()];
        let error = database
            .apply_plan(plan.id, &invalid)
            .expect_err("unknown dependency rejects");
        assert!(matches!(error, StorageError::InvalidPlanInput(_)));
        assert_eq!(database.planning_tasks().expect("tasks remain").len(), 2);
    }

    #[test]
    fn creating_and_applying_a_plan_is_atomic() {
        let mut database = Database::open_in_memory().expect("database opens");
        let spec = database
            .create_spec("Spec".to_owned(), String::new(), String::new())
            .expect("spec creates");
        let document = PlanDocument {
            format_version: 1,
            phases: vec![PlanPhase {
                key: "delivery".to_owned(),
                title: "Delivery".to_owned(),
                position: 0,
                tasks: vec![PlanTask {
                    key: "ship".to_owned(),
                    title: "Ship it".to_owned(),
                    priority: None,
                    position: 0,
                    blocked_by: Vec::new(),
                    subtasks: Vec::new(),
                }],
            }],
        };
        let applied = database
            .create_and_apply_plan(spec.id, "Plan".to_owned(), "Plan body".to_owned(), &document)
            .expect("plan creates and applies");
        assert_eq!(applied.plan.spec_id, spec.id);
        assert_eq!(database.plans().expect("plans read").len(), 1);
        assert_eq!(database.planning_tasks().expect("tasks read").len(), 1);

        let invalid = PlanDocument { format_version: 9, phases: Vec::new() };
        let error = database
            .create_and_apply_plan(spec.id, "Invalid".to_owned(), String::new(), &invalid)
            .expect_err("invalid plan does not write");
        assert!(matches!(error, StorageError::InvalidPlanInput(_)));
        assert_eq!(database.plans().expect("plans remain").len(), 1);
    }

    #[test]
    fn connected_containers_transition_without_partial_writes() {
        let mut database = Database::open_in_memory().expect("database opens");
        let spec = database
            .create_spec("Spec".to_owned(), String::new(), String::new())
            .expect("spec creates");
        let plan = database
            .create_plan(spec.id, "Plan".to_owned(), String::new())
            .expect("plan creates");
        let phase = database
            .create_phase(plan.id, "Phase".to_owned(), String::new(), 0)
            .expect("phase creates");
        let task = database
            .create_planning_task(PlanningTaskCreate {
                project_id: spec.project_id,
                spec_id: Some(spec.id),
                plan_id: Some(plan.id),
                phase_id: Some(phase.id),
                parent_id: None,
                title: "Task".to_owned(),
                body: String::new(),
                priority: TaskPriority::Normal,
                position: 0,
            })
            .expect("task creates");
        assert!(matches!(
            database.transition_phase(phase.id, ContainerAction::Complete, false),
            Err(StorageError::InvalidPhase(DomainError::OpenDescendants { .. }))
        ));
        database
            .connection()
            .execute(
                "UPDATE planning_tasks SET status = 'completed' WHERE id = ?1",
                [task.id.to_string()],
            )
            .expect("task completes");
        database
            .transition_phase(phase.id, ContainerAction::Complete, false)
            .expect("phase completes");
        database
            .transition_plan(plan.id, ContainerAction::Complete, false)
            .expect("plan completes");
        database
            .transition_spec(spec.id, ContainerAction::Complete, false)
            .expect("spec completes");
    }

    #[test]
    fn release_membership_and_note_links_do_not_include_descendants() {
        let mut database = Database::open_in_memory().expect("database opens");
        let release = database
            .create_release("Release".to_owned(), String::new())
            .expect("release creates");
        let spec = database
            .create_spec("Spec".to_owned(), String::new(), String::new())
            .expect("spec creates");
        let plan = database
            .create_plan(spec.id, "Plan".to_owned(), String::new())
            .expect("plan creates");
        let note = database
            .create_note("Note".to_owned(), "Research".to_owned())
            .expect("note creates");
        let project = database.project().expect("project exists");
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
            .add_release_membership(release.id, ReleaseMemberKind::Spec, spec.id.to_string())
            .expect("spec membership adds");
        database
            .add_release_membership(release.id, ReleaseMemberKind::Plan, plan.id.to_string())
            .expect("plan membership adds");
        database
            .add_release_membership(release.id, ReleaseMemberKind::Task, task.id.to_string())
            .expect("task membership adds");
        database
            .add_release_membership(release.id, ReleaseMemberKind::Note, note.id.to_string())
            .expect("note membership adds");
        assert_eq!(database.release_memberships().expect("memberships list").len(), 4);
        database
            .add_note_link(note.id, LinkedRecordKind::Spec, spec.id.to_string())
            .expect("link adds");
        assert_eq!(database.note_links().expect("links list").len(), 1);
    }

    #[test]
    fn connected_dependencies_preserve_links_and_reject_cycles() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let first = database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: None,
                plan_id: None,
                phase_id: None,
                parent_id: None,
                title: "First".to_owned(),
                body: String::new(),
                priority: TaskPriority::Normal,
                position: 0,
            })
            .expect("first creates");
        let second = database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: None,
                plan_id: None,
                phase_id: None,
                parent_id: None,
                title: "Second".to_owned(),
                body: String::new(),
                priority: TaskPriority::Normal,
                position: 1,
            })
            .expect("second creates");
        database
            .add_planning_dependency(first.id, second.id)
            .expect("dependency creates");
        assert_eq!(database.planning_dependencies().expect("dependencies read").len(), 1);
        let error = database
            .add_planning_dependency(second.id, first.id)
            .expect_err("dependency cycle rejects");
        assert!(matches!(
            error,
            StorageError::InvalidPlanningDependency(DomainError::DependencyCycle { .. })
        ));
    }
}
