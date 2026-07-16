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

    pub fn apply(self, action: ContainerAction) -> Result<Self, DomainError> {
        match (self, action) {
            (Self::Open, ContainerAction::Complete) => Ok(Self::Completed),
            (Self::Open, ContainerAction::Cancel) => Ok(Self::Cancelled),
            (Self::Completed, ContainerAction::Complete) => Ok(Self::Completed),
            (Self::Cancelled, ContainerAction::Cancel) => Ok(Self::Cancelled),
            (status, ContainerAction::Complete) => Err(invalid_transition("container", "complete", status.as_str())),
            (status, ContainerAction::Cancel) => Err(invalid_transition("container", "cancel", status.as_str())),
        }
    }
}

/// A container transition requested by an application service.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContainerAction {
    Complete,
    Cancel,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_lifecycle_preserves_the_roadmap_transitions() {
        assert_eq!(TaskStatus::Pending.apply(TaskAction::Start), Ok(TaskStatus::InProgress));
        assert_eq!(TaskStatus::Parked.apply(TaskAction::Unpark), Ok(TaskStatus::Pending));
        assert!(TaskStatus::Parked.apply(TaskAction::Complete).is_err());
        assert_eq!(
            TaskStatus::Completed.apply(TaskAction::Complete),
            Ok(TaskStatus::Completed)
        );
    }
}
