use serde::Serialize;

use super::{ContainerStatus, DomainError, ReleaseId, validate_title};

/// A release grouping one or more epics.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Release {
    pub id: ReleaseId,
    pub title: String,
    pub description: String,
    pub status: ContainerStatus,
}

impl Release {
    /// Create an open release with a generated identifier.
    pub fn new(title: String, description: String) -> Result<Self, DomainError> {
        validate_title(&title)?;
        Ok(Self { id: ReleaseId::new(), title, description, status: ContainerStatus::Open })
    }

    pub fn from_parts(
        id: ReleaseId, title: String, description: String, status: ContainerStatus,
    ) -> Result<Self, DomainError> {
        validate_title(&title)?;
        Ok(Self { id, title, description, status })
    }
}
