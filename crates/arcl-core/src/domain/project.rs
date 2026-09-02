use serde::Serialize;

use super::{DomainError, ProjectId, validate_title};

/// The project that owns one Arc Lightning database.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Project {
    /// The stable project identifier.
    pub id: ProjectId,
    /// The display name used by project adapters.
    pub name: String,
}

impl Project {
    /// Reconstruct a project from validated storage values.
    pub fn from_parts(id: ProjectId, name: String) -> Result<Self, DomainError> {
        validate_title(&name)?;
        Ok(Self { id, name })
    }
}
