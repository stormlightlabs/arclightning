use serde::{Deserialize, Serialize};

use super::{
    DomainError, PhaseId, PlanId, ProjectId, SpecId, TaskId, TaskPriority, TaskStatus, validate_position,
    validate_title,
};

/// A task in the connected planning model.
///
/// All placement fields are optional after the required project. A task may
/// therefore be project-level, attached to a spec, attached to a plan, inside
/// a phase, or nested below another task.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PlanningTask {
    pub id: TaskId,
    pub project_id: ProjectId,
    pub spec_id: Option<SpecId>,
    pub plan_id: Option<PlanId>,
    pub phase_id: Option<PhaseId>,
    pub parent_id: Option<TaskId>,
    /// The optional stable key used by structured plan application.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_key: Option<String>,
    pub title: String,
    /// The editable Markdown task body.
    pub body: String,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub position: i64,
    /// The latest Markdown note left for the next person to resume work.
    pub handoff: String,
    /// Markdown evidence recorded when work was completed.
    pub evidence: String,
    /// Who claimed the task when starting it. Parking, handing off,
    /// completing and cancelling the task clear it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    /// When the task was completed, as an RFC 3339 UTC timestamp. A task
    /// completed before arcl recorded completion times has none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    /// When `arcl compact` archived the task, as an RFC 3339 UTC timestamp.
    /// Only a completed task is archived. Default reads leave archived tasks
    /// out, and `--all` shows them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
}

impl PlanningTask {
    /// Create a pending task at any valid planning level.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        project_id: ProjectId, spec_id: Option<SpecId>, plan_id: Option<PlanId>, phase_id: Option<PhaseId>,
        parent_id: Option<TaskId>, title: String, body: String, priority: TaskPriority, position: i64,
    ) -> Result<Self, DomainError> {
        validate_title(&title)?;
        validate_position(position)?;
        Ok(Self {
            id: TaskId::new(),
            project_id,
            spec_id,
            plan_id,
            phase_id,
            parent_id,
            plan_key: None,
            title,
            body,
            status: TaskStatus::Pending,
            priority,
            position,
            handoff: String::new(),
            evidence: String::new(),
            assignee: None,
            completed_at: None,
            archived_at: None,
        })
    }
}
