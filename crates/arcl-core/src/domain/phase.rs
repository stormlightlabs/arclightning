use serde::Serialize;

use super::{ContainerStatus, DomainError, PhaseId, PlanId, ProjectId, validate_position, validate_title};

/// An optional ordered grouping within a persistent plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Phase {
    pub id: PhaseId,
    pub project_id: ProjectId,
    pub plan_id: PlanId,
    /// The optional stable key used by structured plan application.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_key: Option<String>,
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
        Ok(Self {
            id: PhaseId::new(),
            project_id,
            plan_id,
            plan_key: None,
            title,
            body,
            status: ContainerStatus::Open,
            position,
        })
    }

    /// Attach the stable input key used by plan application.
    pub fn with_plan_key(mut self, plan_key: String) -> Self {
        self.plan_key = Some(plan_key);
        self
    }
}
