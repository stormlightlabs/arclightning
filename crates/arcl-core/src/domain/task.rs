use serde::Serialize;

use super::{DomainError, MilestoneId, TaskId, TaskPriority, TaskStatus, validate_position, validate_title};

/// Validated storage fields used to reconstruct a task.
pub struct TaskParts {
    pub id: TaskId,
    pub milestone_id: MilestoneId,
    pub parent_id: Option<TaskId>,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub position: i64,
    pub plan_key: Option<String>,
    pub handoff: String,
    pub evidence: String,
}

/// A task or independently tracked subtask in a milestone.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Task {
    pub id: TaskId,
    pub milestone_id: MilestoneId,
    pub parent_id: Option<TaskId>,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub position: i64,
    /// The optional plan key used to match this task during plan application.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_key: Option<String>,
    /// The latest Markdown note left for the next person to resume work.
    pub handoff: String,
    /// Optional Markdown evidence recorded when work was completed.
    pub evidence: String,
}

impl Task {
    /// Create a pending task, optionally attached to a parent task.
    pub fn new(
        milestone_id: MilestoneId, parent_id: Option<TaskId>, title: String, description: String,
        priority: TaskPriority, position: i64,
    ) -> Result<Self, DomainError> {
        validate_title(&title)?;
        validate_position(position)?;
        Ok(Self {
            id: TaskId::new(),
            milestone_id,
            parent_id,
            title,
            description,
            status: TaskStatus::Pending,
            priority,
            position,
            plan_key: None,
            handoff: String::new(),
            evidence: String::new(),
        })
    }
}

impl TryFrom<TaskParts> for Task {
    type Error = DomainError;

    fn try_from(parts: TaskParts) -> Result<Self, Self::Error> {
        validate_title(&parts.title)?;
        validate_position(parts.position)?;
        Ok(Self {
            id: parts.id,
            milestone_id: parts.milestone_id,
            parent_id: parts.parent_id,
            title: parts.title,
            description: parts.description,
            status: parts.status,
            priority: parts.priority,
            position: parts.position,
            plan_key: parts.plan_key,
            handoff: parts.handoff,
            evidence: parts.evidence,
        })
    }
}
