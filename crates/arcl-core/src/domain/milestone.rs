use serde::Serialize;

use super::{ContainerStatus, DomainError, EpicId, MilestoneId, validate_position, validate_title};

/// An ordered implementation stage belonging to one epic.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Milestone {
    pub id: MilestoneId,
    pub epic_id: EpicId,
    pub title: String,
    pub description: String,
    pub status: ContainerStatus,
    pub position: i64,
    /// The optional plan key used to match this milestone during plan application.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_key: Option<String>,
}

impl Milestone {
    /// Create an open milestone for an existing epic.
    pub fn new(epic_id: EpicId, title: String, description: String, position: i64) -> Result<Self, DomainError> {
        validate_title(&title)?;
        validate_position(position)?;
        Ok(Self {
            id: MilestoneId::new(),
            epic_id,
            title,
            description,
            status: ContainerStatus::Open,
            position,
            plan_key: None,
        })
    }

    /// Reconstruct a milestone from validated storage values.
    pub fn from_parts(
        id: MilestoneId, epic_id: EpicId, title: String, description: String, status: ContainerStatus, position: i64,
        plan_key: Option<String>,
    ) -> Result<Self, DomainError> {
        validate_title(&title)?;
        validate_position(position)?;
        Ok(Self { id, epic_id, title, description, status, position, plan_key })
    }
}
