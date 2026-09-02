use serde::Serialize;

use super::{ContainerStatus, DomainError, PlanId, ProjectId, SpecId, validate_title};

/// A persistent implementation plan belonging to one specification.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Plan {
    pub id: PlanId,
    pub project_id: ProjectId,
    pub spec_id: SpecId,
    pub title: String,
    /// The editable Markdown implementation plan body.
    pub body: String,
    pub status: ContainerStatus,
}

impl Plan {
    /// Create an open plan for an existing specification.
    pub fn new(project_id: ProjectId, spec_id: SpecId, title: String, body: String) -> Result<Self, DomainError> {
        validate_title(&title)?;
        Ok(Self { id: PlanId::new(), project_id, spec_id, title, body, status: ContainerStatus::Open })
    }
}
