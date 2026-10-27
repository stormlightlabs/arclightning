use thiserror::Error;

use super::IdError;

/// The longest task assignee name, in characters.
pub const MAX_ASSIGNEE_CHARS: usize = 64;

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
    /// A task claim with an empty name, a name longer than
    /// [`MAX_ASSIGNEE_CHARS`], or one that holds a control character such as
    /// a newline or a format character such as a zero-width space.
    #[error(
        "assignee `{}` is invalid; use a non-empty name of at most 64 characters without control or format characters",
        value.escape_debug()
    )]
    InvalidAssignee { value: String },
    #[error(transparent)]
    InvalidId(#[from] IdError),
}

pub fn validate_title(title: &str) -> Result<()> {
    if title.trim().is_empty() { Err(DomainError::EmptyTitle) } else { Ok(()) }
}

/// Return `name` trimmed, or refuse an empty name, one longer than
/// [`MAX_ASSIGNEE_CHARS`] characters, or one with a control or format
/// character.
pub fn validate_assignee(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > MAX_ASSIGNEE_CHARS
        || trimmed.chars().any(|c| c.is_control() || is_format_character(c))
    {
        Err(DomainError::InvalidAssignee { value: name.to_owned() })
    } else {
        Ok(trimmed.to_owned())
    }
}

/// Return whether `c` is one of the common Unicode format characters
/// (general category Cf) that hide in text or reorder it: zero-width
/// characters, directional marks and embeddings, invisible operators,
/// directional isolates and the byte order mark. The list is explicit because
/// the standard library has no general category lookup.
fn is_format_character(c: char) -> bool {
    matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
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

    #[test]
    fn assignee_refuses_format_characters() {
        for name in [
            "co\u{200B}dex",
            "\u{202E}xedoc",
            "co\u{2066}dex",
            "\u{FEFF}codex",
            "co\u{2060}dex",
        ] {
            assert!(
                matches!(validate_assignee(name), Err(DomainError::InvalidAssignee { .. })),
                "{}",
                name.escape_unicode()
            );
        }
    }

    #[test]
    fn assignee_is_capped_at_64_characters() {
        let longest = "é".repeat(MAX_ASSIGNEE_CHARS);
        assert_eq!(validate_assignee(&longest), Ok(longest.clone()));
        assert!(matches!(
            validate_assignee(&format!("{longest}e")),
            Err(DomainError::InvalidAssignee { .. })
        ));
    }
}
