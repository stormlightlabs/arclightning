use serde::Serialize;

use super::{DomainError, TaskId};

/// A directed blocking relationship between two tasks.
///
/// `task_id` is the task that cannot proceed, and `blocker_id` is the task
/// whose completion satisfies the relationship.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
pub struct TaskDependency {
    /// The task whose readiness is gated.
    pub task_id: TaskId,
    /// The task whose completed status satisfies the dependency.
    pub blocker_id: TaskId,
}

impl TaskDependency {
    /// Construct a dependency after rejecting a task that blocks itself.
    pub fn new(task_id: TaskId, blocker_id: TaskId) -> Result<Self, DomainError> {
        if task_id == blocker_id {
            return Err(DomainError::SelfDependency { task: task_id.to_string() });
        }
        Ok(Self { task_id, blocker_id })
    }
}
