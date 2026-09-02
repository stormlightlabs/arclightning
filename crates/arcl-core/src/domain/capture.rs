use serde::{Deserialize, Serialize};

use super::{CaptureId, DomainError, ProjectId, validate_title};

/// The lifecycle of an inbox capture.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureStatus {
    Captured,
    Promoted,
    Discarded,
}

impl CaptureStatus {
    /// Return the stable value stored in SQLite and exposed by adapters.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Captured => "captured",
            Self::Promoted => "promoted",
            Self::Discarded => "discarded",
        }
    }

    /// Parse a persisted capture status.
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "captured" => Ok(Self::Captured),
            "promoted" => Ok(Self::Promoted),
            "discarded" => Ok(Self::Discarded),
            _ => Err(DomainError::InvalidStatus { entity: "capture", value: value.to_owned() }),
        }
    }

    /// Apply an inbox lifecycle action without changing promotion provenance.
    pub fn apply(self, action: CaptureAction) -> Result<Self, DomainError> {
        match (self, action) {
            (Self::Captured, CaptureAction::Discard) => Ok(Self::Discarded),
            (Self::Discarded, CaptureAction::Discard) => Ok(Self::Discarded),
            (status, CaptureAction::Discard) => Err(DomainError::InvalidTransition {
                entity: "capture",
                action: action.as_str(),
                from: status.as_str().to_owned(),
            }),
        }
    }
}

/// An inbox lifecycle operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureAction {
    Discard,
}

impl CaptureAction {
    /// Return the stable action name used in validation errors.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Discard => "discard",
        }
    }
}

/// An unstructured thought waiting in the project inbox.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Capture {
    pub id: CaptureId,
    pub project_id: ProjectId,
    pub title: String,
    /// Markdown captured before it is promoted or discarded.
    pub body: String,
    pub status: CaptureStatus,
    /// RFC 3339 creation timestamp supplied by SQLite.
    pub created_at: String,
}

impl Capture {
    /// Create a captured inbox record.
    pub fn new(project_id: ProjectId, title: String, body: String, created_at: String) -> Result<Self, DomainError> {
        validate_title(&title)?;
        Ok(Self { id: CaptureId::new(), project_id, title, body, status: CaptureStatus::Captured, created_at })
    }
}
