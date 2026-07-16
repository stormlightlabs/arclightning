use thiserror::Error;

use super::IdError;

type Result<T> = std::result::Result<T, DomainError>;

/// Errors caused by invalid domain values or state transitions.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum DomainError {
    #[error("title cannot be empty")]
    EmptyTitle,
    #[error("{entity} update requires at least one field")]
    NoFieldsToUpdate { entity: &'static str },
    #[error("invalid {entity} status `{value}`")]
    InvalidStatus { entity: &'static str, value: String },
    #[error("position {position} is invalid; positions must be non-negative")]
    InvalidPosition { position: i64 },
    #[error("cannot {action} {entity} while it is {from}")]
    InvalidTransition {
        entity: &'static str,
        action: &'static str,
        from: String,
    },
    #[error(transparent)]
    InvalidId(#[from] IdError),
}

pub fn validate_title(title: &str) -> Result<()> {
    if title.trim().is_empty() { Err(DomainError::EmptyTitle) } else { Ok(()) }
}

pub fn validate_position(position: i64) -> Result<()> {
    if position < 0 { Err(DomainError::InvalidPosition { position }) } else { Ok(()) }
}

pub(super) fn invalid_transition(entity: &'static str, action: &'static str, from: impl Into<String>) -> DomainError {
    DomainError::InvalidTransition { entity, action, from: from.into() }
}
