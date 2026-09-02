use serde::Serialize;

use super::{CaptureId, ContainerStatus, DomainError, ProjectId, SpecId, validate_title};

/// A specification owned and edited by Arc Lightning.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Spec {
    pub id: SpecId,
    pub project_id: ProjectId,
    pub title: String,
    /// The editable Markdown specification body.
    pub body: String,
    /// Markdown acceptance criteria owned by the specification.
    pub acceptance_criteria: String,
    pub status: ContainerStatus,
    /// The capture from which this specification was promoted, if any.
    pub source_capture_id: Option<CaptureId>,
}

impl Spec {
    /// Create an open specification with an owned Markdown body.
    pub fn new(
        project_id: ProjectId, title: String, body: String, acceptance_criteria: String,
    ) -> Result<Self, DomainError> {
        validate_title(&title)?;
        Ok(Self {
            id: SpecId::new(),
            project_id,
            title,
            body,
            acceptance_criteria,
            status: ContainerStatus::Open,
            source_capture_id: None,
        })
    }
}
