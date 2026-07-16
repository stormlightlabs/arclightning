use serde::Serialize;

use super::{ContainerStatus, DomainError, EpicId, ReleaseId, validate_title};

/// An epic linked to one Markdown specification in the worktree.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Epic {
    pub id: EpicId,
    pub release_id: Option<ReleaseId>,
    pub title: String,
    pub description: String,
    pub spec_path: String,
    pub status: ContainerStatus,
}

impl Epic {
    /// Create an open epic for an already validated worktree-relative spec path.
    pub fn new(
        title: String, description: String, spec_path: String, release_id: Option<ReleaseId>,
    ) -> Result<Self, DomainError> {
        validate_title(&title)?;
        validate_spec_path(&spec_path)?;
        Ok(Self { id: EpicId::new(), release_id, title, description, spec_path, status: ContainerStatus::Open })
    }

    pub fn from_parts(
        id: EpicId, release_id: Option<ReleaseId>, title: String, description: String, spec_path: String,
        status: ContainerStatus,
    ) -> Result<Self, DomainError> {
        validate_title(&title)?;
        validate_spec_path(&spec_path)?;
        Ok(Self { id, release_id, title, description, spec_path, status })
    }
}

fn validate_spec_path(path: &str) -> Result<(), DomainError> {
    if path.is_empty() { Err(DomainError::EmptySpecPath) } else { Ok(()) }
}
