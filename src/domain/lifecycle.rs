use std::str::FromStr;

use serde::{Deserialize, Serialize};

use super::error::{DomainError, invalid_transition};

/// The lifecycle of a task or subtask.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Parked,
    Completed,
    Cancelled,
}

impl TaskStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Parked => "parked",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn apply(self, action: TaskAction) -> Result<Self, DomainError> {
        let next = match (self, action) {
            (Self::Pending, TaskAction::Start) => Self::InProgress,
            (Self::InProgress, TaskAction::Start) => Self::InProgress,
            (Self::Pending | Self::InProgress, TaskAction::Park) => Self::Parked,
            (Self::Parked, TaskAction::Unpark) => Self::Pending,
            (Self::Pending | Self::InProgress, TaskAction::Complete) => Self::Completed,
            (Self::Completed, TaskAction::Complete) => Self::Completed,
            (Self::Pending | Self::InProgress | Self::Parked, TaskAction::Cancel) => Self::Cancelled,
            (Self::Cancelled, TaskAction::Cancel) => Self::Cancelled,
            (status, action) => {
                return Err(invalid_transition("task", action.as_str(), status.as_str()));
            }
        };
        Ok(next)
    }
}

impl FromStr for TaskStatus {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "in_progress" => Ok(Self::InProgress),
            "parked" => Ok(Self::Parked),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(DomainError::InvalidStatus { entity: "task", value: s.to_owned() }),
        }
    }
}

/// A task transition requested by an application service.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskAction {
    Start,
    Park,
    Unpark,
    Complete,
    Cancel,
}

impl TaskAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Park => "park",
            Self::Unpark => "unpark",
            Self::Complete => "complete",
            Self::Cancel => "cancel",
        }
    }
}

/// The lifecycle of a release, epic, or milestone.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerStatus {
    Open,
    Completed,
    Cancelled,
}

impl ContainerStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }

    /// Parse a persisted container status while preserving the entity name in errors.
    pub fn parse(entity: &'static str, value: &str) -> Result<Self, DomainError> {
        match value {
            "open" => Ok(Self::Open),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(DomainError::InvalidStatus { entity, value: value.to_owned() }),
        }
    }

    pub fn apply(self, entity: &'static str, action: ContainerAction) -> Result<Self, DomainError> {
        match (self, action) {
            (Self::Open, ContainerAction::Complete) => Ok(Self::Completed),
            (Self::Open, ContainerAction::Cancel) => Ok(Self::Cancelled),
            (Self::Completed, ContainerAction::Complete) => Ok(Self::Completed),
            (Self::Cancelled, ContainerAction::Cancel) => Ok(Self::Cancelled),
            (status, ContainerAction::Complete) => Err(invalid_transition(entity, "complete", status.as_str())),
            (status, ContainerAction::Cancel) => Err(invalid_transition(entity, "cancel", status.as_str())),
        }
    }
}

/// A container transition requested by an application service.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContainerAction {
    Complete,
    Cancel,
}

impl ContainerAction {
    /// Return the command verb used in lifecycle errors and output.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Cancel => "cancel",
        }
    }
}

/// The stable task-priority ordering used by ready-work queries.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskPriority {
    Critical,
    High,
    #[default]
    Normal,
    Low,
}

impl TaskPriority {
    /// Return the stable value stored in SQLite and exposed by JSON output.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::High => "high",
            Self::Normal => "normal",
            Self::Low => "low",
        }
    }

    /// Parse a persisted or command-line task priority.
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "critical" => Ok(Self::Critical),
            "high" => Ok(Self::High),
            "normal" => Ok(Self::Normal),
            "low" => Ok(Self::Low),
            _ => Err(DomainError::InvalidPriority { value: value.to_owned() }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_lifecycle_preserves_the_roadmap_transitions() {
        use TaskAction::{Cancel, Complete, Park, Start, Unpark};
        use TaskStatus::{Cancelled, Completed, InProgress, Parked, Pending};

        let allowed = [
            (Pending, Start, InProgress),
            (InProgress, Start, InProgress),
            (Pending, Park, Parked),
            (InProgress, Park, Parked),
            (Parked, Unpark, Pending),
            (Pending, Complete, Completed),
            (InProgress, Complete, Completed),
            (Completed, Complete, Completed),
            (Pending, Cancel, Cancelled),
            (InProgress, Cancel, Cancelled),
            (Parked, Cancel, Cancelled),
            (Cancelled, Cancel, Cancelled),
        ];
        for (status, action, expected) in allowed {
            assert_eq!(status.apply(action), Ok(expected), "{status:?} {action:?}");
        }

        for status in [Pending, InProgress, Parked, Completed, Cancelled] {
            for action in [Start, Park, Unpark, Complete, Cancel] {
                if !allowed
                    .iter()
                    .any(|(from, candidate, _)| *from == status && *candidate == action)
                {
                    assert!(status.apply(action).is_err(), "{status:?} {action:?}");
                }
            }
        }
    }

    #[test]
    fn container_terminal_transitions_are_idempotent_but_not_interchangeable() {
        assert_eq!(
            ContainerStatus::Open.apply("release", ContainerAction::Complete),
            Ok(ContainerStatus::Completed)
        );
        assert_eq!(
            ContainerStatus::Completed.apply("release", ContainerAction::Complete),
            Ok(ContainerStatus::Completed)
        );
        assert!(
            ContainerStatus::Completed
                .apply("release", ContainerAction::Cancel)
                .is_err()
        );
        assert!(
            ContainerStatus::Cancelled
                .apply("release", ContainerAction::Complete)
                .is_err()
        );
    }
}
