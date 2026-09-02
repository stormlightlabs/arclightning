use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::Serialize;

use super::{Result, StorageError, capture, note, planning_task, spec};
use arcl_core::domain::{
    Capture, CaptureId, CapturePromotion, CapturePromotionTarget, CaptureStatus, DomainError, Note, NoteId, PhaseId,
    PlanId, PlanningTask, ProjectId, Spec, SpecId, TaskId, TaskPriority,
};

/// The requested destination for a capture promotion.
#[derive(Clone, Debug)]
pub enum CapturePromotionInput {
    /// Create a specification owned by the capture.
    Spec {
        title: String,
        body: String,
        acceptance_criteria: String,
    },
    /// Create a task owned by the project and optionally attached to planning records.
    Task(CaptureTaskPromotion),
    /// Create a note owned by the project.
    Note { title: String, body: String },
}

impl CapturePromotionInput {
    /// Return the stable kind of destination requested by this promotion.
    const fn promotion_kind(&self) -> &'static str {
        match self {
            Self::Spec { .. } => "spec",
            Self::Task(_) => "task",
            Self::Note { .. } => "note",
        }
    }
}

/// The task fields used when promoting a capture directly to a task.
#[derive(Clone, Debug)]
pub struct CaptureTaskPromotion {
    /// Optional specification ancestry.
    pub spec_id: Option<SpecId>,
    /// Optional plan ancestry.
    pub plan_id: Option<PlanId>,
    /// Optional phase ancestry; a plan is required when this is present.
    pub phase_id: Option<PhaseId>,
    /// Optional parent task ancestry.
    pub parent_id: Option<TaskId>,
    pub title: String,
    /// Markdown task content copied from the capture or supplied by the caller.
    pub body: String,
    pub priority: TaskPriority,
    pub position: i64,
}

/// The record created or found by a capture promotion.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", content = "record", rename_all = "snake_case")]
pub enum CapturePromotionRecord {
    Spec(Spec),
    Task(PlanningTask),
    Note(Note),
}

/// The result of a capture promotion, including both sides of provenance.
#[derive(Clone, Debug, Serialize)]
pub struct CapturePromotionResult {
    /// The source capture after its status changed to promoted.
    pub capture: Capture,
    /// The durable provenance edge between source and destination.
    pub promotion: CapturePromotion,
    /// The destination record, either newly created or returned idempotently.
    pub record: CapturePromotionRecord,
}

/// Read capture promotion provenance as explicit typed relationships.
pub(super) fn capture_promotions(connection: &Connection, project_id: ProjectId) -> Result<Vec<CapturePromotion>> {
    let mut statement = connection.prepare(
        "SELECT project_id, capture_id, target_kind, target_id
         FROM capture_promotions WHERE project_id = ?1 ORDER BY capture_id",
    )?;
    let rows = statement.query_map([project_id.to_string()], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    rows.map(|row| {
        let (project, capture, kind, target) = row?;
        let project_id = ProjectId::parse(&project)
            .map_err(DomainError::from)
            .map_err(StorageError::InvalidCapture)?;
        let capture_id = CaptureId::parse(&capture)
            .map_err(DomainError::from)
            .map_err(StorageError::InvalidCapture)?;
        let target = promotion_target(&kind, &target)?;
        Ok(CapturePromotion { project_id, capture_id, target })
    })
    .collect()
}

/// Promote a capture to exactly one owned planning record in one transaction.
///
/// The capture and provenance edge are committed with the destination record.
/// Repeating the same destination kind returns the existing record; asking for
/// a different destination is rejected rather than creating ambiguous history.
pub(super) fn promote_capture(
    connection: &mut Connection, id: CaptureId, input: CapturePromotionInput,
) -> Result<CapturePromotionResult> {
    let requested_kind = input.promotion_kind();
    let tx = connection.transaction()?;
    let current = capture(&tx, id)?.ok_or_else(|| StorageError::CaptureNotFound { id: id.to_string() })?;
    let relationship = tx
        .query_row(
            "SELECT target_kind, target_id FROM capture_promotions WHERE capture_id = ?1",
            [id.to_string()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;

    match (current.status, relationship) {
        (CaptureStatus::Discarded, _) => {
            Err(StorageError::CaptureNotPromotable { id: id.to_string(), status: current.status.as_str().to_owned() })
        }
        (CaptureStatus::Promoted, None) | (CaptureStatus::Captured, Some(_)) => {
            Err(StorageError::InconsistentCapturePromotion { id: id.to_string() })
        }
        (CaptureStatus::Promoted, Some((existing_kind, target_id))) => {
            let target = promotion_target(&existing_kind, &target_id)?;
            if target.promotion_kind() != requested_kind {
                return Err(StorageError::AmbiguousCapturePromotion {
                    capture: id.to_string(),
                    existing: target.promotion_kind(),
                    requested: requested_kind,
                });
            }
            let record = read_promotion_record(&tx, current.project_id, target, id)?;
            tx.commit()?;
            Ok(CapturePromotionResult {
                capture: current.clone(),
                promotion: CapturePromotion { project_id: current.project_id, capture_id: id, target },
                record,
            })
        }
        (CaptureStatus::Captured, None) => {
            let (target, record) = create_promotion_record(&tx, current.project_id, id, input)?;
            let target_id = target.promotion_target_id();
            tx.execute(
                "INSERT INTO capture_promotions (capture_id, project_id, target_kind, target_id)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    id.to_string(),
                    current.project_id.to_string(),
                    requested_kind,
                    target_id
                ],
            )?;
            tx.execute(
                "UPDATE captures SET status = 'promoted' WHERE id = ?1",
                [id.to_string()],
            )?;
            tx.commit()?;
            let capture = Capture { status: CaptureStatus::Promoted, ..current };
            Ok(CapturePromotionResult {
                capture: capture.clone(),
                promotion: CapturePromotion { project_id: capture.project_id, capture_id: id, target },
                record,
            })
        }
    }
}

fn promotion_target(kind: &str, id: &str) -> Result<CapturePromotionTarget> {
    match kind {
        "spec" => Ok(CapturePromotionTarget::Spec(
            SpecId::parse(id)
                .map_err(DomainError::from)
                .map_err(StorageError::InvalidCapture)?,
        )),
        "task" => Ok(CapturePromotionTarget::Task(
            TaskId::parse(id)
                .map_err(DomainError::from)
                .map_err(StorageError::InvalidCapture)?,
        )),
        "note" => Ok(CapturePromotionTarget::Note(
            NoteId::parse(id)
                .map_err(DomainError::from)
                .map_err(StorageError::InvalidCapture)?,
        )),
        _ => Err(StorageError::InvalidCapture(DomainError::InvalidStatus {
            entity: "capture promotion target",
            value: kind.to_owned(),
        })),
    }
}

fn read_promotion_record(
    connection: &Connection, project_id: ProjectId, target: CapturePromotionTarget, capture_id: CaptureId,
) -> Result<CapturePromotionRecord> {
    match target {
        CapturePromotionTarget::Spec(id) => {
            let record = spec(connection, id)?
                .filter(|record| record.project_id == project_id)
                .ok_or_else(|| StorageError::InconsistentCapturePromotion { id: capture_id.to_string() })?;
            Ok(CapturePromotionRecord::Spec(record))
        }
        CapturePromotionTarget::Task(id) => {
            let record = planning_task(connection, id)?
                .filter(|record| record.project_id == project_id)
                .ok_or_else(|| StorageError::InconsistentCapturePromotion { id: capture_id.to_string() })?;
            Ok(CapturePromotionRecord::Task(record))
        }
        CapturePromotionTarget::Note(id) => {
            let record = note(connection, id)?
                .filter(|record| record.project_id == project_id)
                .ok_or_else(|| StorageError::InconsistentCapturePromotion { id: capture_id.to_string() })?;
            Ok(CapturePromotionRecord::Note(record))
        }
    }
}

fn create_promotion_record(
    connection: &Transaction<'_>, project_id: ProjectId, capture_id: CaptureId, input: CapturePromotionInput,
) -> Result<(CapturePromotionTarget, CapturePromotionRecord)> {
    match input {
        CapturePromotionInput::Spec { title, body, acceptance_criteria } => {
            let mut spec =
                Spec::new(project_id, title, body, acceptance_criteria).map_err(StorageError::InvalidSpec)?;
            spec.source_capture_id = Some(capture_id);
            connection.execute(
                "INSERT INTO specs (id, project_id, title, body, acceptance_criteria, status, source_capture_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    spec.id.to_string(),
                    spec.project_id.to_string(),
                    spec.title,
                    spec.body,
                    spec.acceptance_criteria,
                    spec.status.as_str(),
                    capture_id.to_string()
                ],
            )?;
            Ok((
                CapturePromotionTarget::Spec(spec.id),
                CapturePromotionRecord::Spec(spec),
            ))
        }
        CapturePromotionInput::Task(task_input) => {
            let task = PlanningTask::new(
                project_id,
                task_input.spec_id,
                task_input.plan_id,
                task_input.phase_id,
                task_input.parent_id,
                task_input.title,
                task_input.body,
                task_input.priority,
                task_input.position,
            )
            .map_err(StorageError::InvalidPlanningTask)?;
            super::validate_task_ancestry(connection, &task)?;
            super::insert_task(connection, &task)?;
            Ok((
                CapturePromotionTarget::Task(task.id),
                CapturePromotionRecord::Task(task),
            ))
        }
        CapturePromotionInput::Note { title, body } => {
            let note = Note::new(project_id, title, body).map_err(StorageError::InvalidNote)?;
            connection.execute(
                "INSERT INTO notes (id, project_id, title, body) VALUES (?1, ?2, ?3, ?4)",
                params![note.id.to_string(), note.project_id.to_string(), note.title, note.body],
            )?;
            Ok((
                CapturePromotionTarget::Note(note.id),
                CapturePromotionRecord::Note(note),
            ))
        }
    }
}
