use serde::Serialize;

use super::{ContainerStatus, DomainError, PhaseId, PlanId, ProjectId, validate_position, validate_title};

/// An optional ordered grouping within a persistent plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Phase {
    pub id: PhaseId,
    pub project_id: ProjectId,
    pub plan_id: PlanId,
    pub title: String,
    /// The editable Markdown phase body.
    pub body: String,
    pub status: ContainerStatus,
    pub position: i64,
}

impl Phase {
    /// Create an open phase in a plan.
    pub fn new(
        project_id: ProjectId, plan_id: PlanId, title: String, body: String, position: i64,
    ) -> Result<Self, DomainError> {
        validate_title(&title)?;
        validate_position(position)?;
        Ok(Self { id: PhaseId::new(), project_id, plan_id, title, body, status: ContainerStatus::Open, position })
    }
}
