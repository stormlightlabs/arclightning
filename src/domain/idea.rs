use serde::{Deserialize, Serialize};

use super::error::invalid_transition;
use super::{DomainError, IdeaId, validate_title};

/// The lifecycle of an idea in the local inbox.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdeaStatus {
    Captured,
    Promoted,
    Discarded,
}

impl IdeaStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Captured => "captured",
            Self::Promoted => "promoted",
            Self::Discarded => "discarded",
        }
    }

    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "captured" => Ok(Self::Captured),
            "promoted" => Ok(Self::Promoted),
            "discarded" => Ok(Self::Discarded),
            _ => Err(DomainError::InvalidStatus { entity: "idea", value: value.to_owned() }),
        }
    }

    pub fn apply(self, action: IdeaAction) -> Result<Self, DomainError> {
        match (self, action) {
            (Self::Captured, IdeaAction::Discard) => Ok(Self::Discarded),
            (Self::Discarded, IdeaAction::Discard) => Ok(Self::Discarded),
            (status, IdeaAction::Discard) => Err(invalid_transition("idea", action.as_str(), status.as_str())),
        }
    }

    pub fn validate_update(self) -> Result<(), DomainError> {
        if self == Self::Discarded {
            Err(invalid_transition("idea", "update", self.as_str()))
        } else {
            Ok(())
        }
    }
}

/// A lifecycle operation supported by the idea inbox.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdeaAction {
    Discard,
}

impl IdeaAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Discard => "discard",
        }
    }
}

/// An idea captured in the local project inbox.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Idea {
    pub id: IdeaId,
    pub title: String,
    pub description: String,
    pub status: IdeaStatus,
}

impl Idea {
    pub fn new(title: String, description: String) -> Result<Self, DomainError> {
        validate_title(&title)?;
        Ok(Self { id: IdeaId::new(), title, description, status: IdeaStatus::Captured })
    }

    pub fn from_parts(id: IdeaId, title: String, description: String, status: IdeaStatus) -> Result<Self, DomainError> {
        validate_title(&title)?;
        Ok(Self { id, title, description, status })
    }
}

#[cfg(test)]
mod tests {
    use super::{IdeaAction, IdeaStatus};

    #[test]
    fn ideas_follow_the_capture_and_discard_lifecycle() {
        assert_eq!(
            IdeaStatus::Captured.apply(IdeaAction::Discard),
            Ok(IdeaStatus::Discarded)
        );
        assert_eq!(
            IdeaStatus::Discarded.apply(IdeaAction::Discard),
            Ok(IdeaStatus::Discarded)
        );
        assert!(IdeaStatus::Promoted.apply(IdeaAction::Discard).is_err());
    }

    #[test]
    fn discarded_ideas_cannot_be_updated() {
        assert!(IdeaStatus::Discarded.validate_update().is_err());
        assert!(IdeaStatus::Captured.validate_update().is_ok());
    }
}
