use serde::Serialize;

use super::{DomainError, NoteId, ProjectId, validate_title};

/// A Markdown research, decision, reference, or debugging record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Note {
    pub id: NoteId,
    pub project_id: ProjectId,
    pub title: String,
    /// The editable Markdown note body.
    pub body: String,
}

impl Note {
    /// Create a note owned by a project.
    pub fn new(project_id: ProjectId, title: String, body: String) -> Result<Self, DomainError> {
        validate_title(&title)?;
        Ok(Self { id: NoteId::new(), project_id, title, body })
    }
}
