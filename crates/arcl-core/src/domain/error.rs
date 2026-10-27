use thiserror::Error;

use super::IdError;

type Result<T> = std::result::Result<T, DomainError>;

/// Errors caused by invalid domain values or state transitions.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum DomainError {
    #[error("title cannot be empty")]
    EmptyTitle,
    #[error("spec path cannot be empty")]
    EmptySpecPath,
    #[error("{entity} update requires at least one field")]
    NoFieldsToUpdate { entity: &'static str },
    #[error("invalid {entity} status `{value}`")]
    InvalidStatus { entity: &'static str, value: String },
    #[error("position {position} is invalid; positions must be non-negative")]
    InvalidPosition { position: i64 },
    #[error("invalid task priority `{value}`")]
    InvalidPriority { value: String },
    #[error("task `{task}` cannot be its own parent")]
    SelfParent { task: String },
    #[error("task `{task}` and parent `{parent}` must belong to the same milestone")]
    DifferentMilestone { task: String, parent: String },
    #[error("{entity} `{id}` and related record `{related}` must belong to the same project")]
    DifferentProject {
        entity: String,
        id: String,
        related: String,
    },
    #[error("task `{task}` has contradictory `{relationship}` ancestry")]
    ContradictoryAncestry { task: String, relationship: String },
    #[error("{entity} `{id}` is `{status}`; it must be open for this operation")]
    InvalidContainerState {
        entity: &'static str,
        id: String,
        status: String,
    },
    #[error("parenting task `{task}` to `{parent}` would create a cycle")]
    ParentCycle { task: String, parent: String },
    #[error("task `{task}` cannot depend on itself")]
    SelfDependency { task: String },
    /// Blocker edges form a cycle; each task in `path` is blocked by the next.
    #[error("blocker edges would form a cycle: {}; each task is blocked by the next", .path.join(" -> "))]
    BlockerCycle { path: Vec<String> },
    /// A task is blocked by its own ancestor or descendant. `chain` runs from
    /// the descendant up to the ancestor.
    #[error("task `{task}` cannot be blocked by `{blocker}`, its {kinship}; parent chain: {}", .chain.join(" -> "))]
    BlockerAncestry {
        task: String,
        blocker: String,
        kinship: &'static str,
        chain: Vec<String>,
    },
    #[error("task `{task}` is already blocked by `{blocker}`")]
    DuplicateDependency { task: String, blocker: String },
    #[error("task subtree rooted at `{task}` contains tasks from different milestones")]
    SubtreeDifferentMilestones { task: String },
    #[error("cannot {action} {entity} while it is {from}")]
    InvalidTransition {
        entity: &'static str,
        action: &'static str,
        from: String,
    },
    #[error(
        "cannot {action} {entity} `{id}` while it has non-terminal descendants; pass --allow-open-children to override"
    )]
    OpenDescendants {
        entity: &'static str,
        id: String,
        action: &'static str,
    },
    /// Starting a pending task that readiness reports as not ready, without `--force`.
    #[error("cannot start task `{id}` while it is not ready: {}; pass --force to override", reasons.join("; "))]
    NotReady { id: String, reasons: Vec<String> },
    /// Completing a task with its subtasks, while a subtask is parked or the
    /// subtree has an unfinished blocker from outside it.
    #[error("cannot complete task `{id}` with its subtasks: {}", reasons.join("; "))]
    SubtasksNotCompletable { id: String, reasons: Vec<String> },
    /// A task claim with an empty name, or one that holds a control
    /// character such as a newline.
    #[error("assignee `{value}` is invalid; use a non-empty name without control characters")]
    InvalidAssignee { value: String },
    #[error(transparent)]
    InvalidId(#[from] IdError),
}

pub fn validate_title(title: &str) -> Result<()> {
    if title.trim().is_empty() { Err(DomainError::EmptyTitle) } else { Ok(()) }
}

/// Return `name` trimmed, or refuse an empty name or one with a control
/// character.
pub fn validate_assignee(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_control) {
        Err(DomainError::InvalidAssignee { value: name.to_owned() })
    } else {
        Ok(trimmed.to_owned())
    }
}

pub fn validate_position(position: i64) -> Result<()> {
    if position < 0 { Err(DomainError::InvalidPosition { position }) } else { Ok(()) }
}

pub fn invalid_transition(entity: &'static str, action: &'static str, from: impl Into<String>) -> DomainError {
    DomainError::InvalidTransition { entity, action, from: from.into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assignee_is_trimmed_and_must_hold_a_name() {
        assert_eq!(validate_assignee("  codex  "), Ok("codex".to_owned()));
        assert!(matches!(
            validate_assignee("   "),
            Err(DomainError::InvalidAssignee { .. })
        ));
        assert!(matches!(
            validate_assignee("a\nb"),
            Err(DomainError::InvalidAssignee { .. })
        ));
    }
}
