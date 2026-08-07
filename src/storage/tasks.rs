use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
};

use rusqlite::{Connection, OptionalExtension, params};

use crate::domain::{self, DomainError, MilestoneId, Task, TaskAction, TaskId, TaskPriority, TaskStatus};

use super::{Result, StorageError, dependencies};

/// Optional fields accepted by a task update.
#[derive(Clone, Debug, Default)]
pub struct TaskUpdate {
    /// Replacement task title.
    pub title: Option<String>,
    /// Replacement Markdown description.
    pub description: Option<String>,
    /// Replacement task priority.
    pub priority: Option<TaskPriority>,
    /// Replacement display position.
    pub position: Option<i64>,
    /// Destination milestone for the complete descendant subtree.
    pub milestone_id: Option<MilestoneId>,
    /// Parent operation: `None` preserves the parent, `Some(None)` clears it.
    pub parent_change: Option<Option<TaskId>>,
}

/// Fields used to create a task and its optional direct blockers atomically.
#[derive(Clone, Debug)]
pub struct TaskCreate {
    /// Owning milestone.
    pub milestone_id: MilestoneId,
    /// Optional parent task.
    pub parent_id: Option<TaskId>,
    /// Task title.
    pub title: String,
    /// Markdown description.
    pub description: String,
    /// Task priority.
    pub priority: TaskPriority,
    /// Display position.
    pub position: i64,
    /// Direct blockers that must be completed before this task is ready.
    pub blockers: Vec<TaskId>,
}

pub(super) struct RawTask {
    id: String,
    milestone_id: String,
    parent_id: Option<String>,
    title: String,
    description: String,
    status: String,
    priority: String,
    position: i64,
    handoff: String,
    evidence: String,
}

impl RawTask {
    pub(super) fn decode(self) -> Result<Task> {
        let id = TaskId::parse(&self.id)
            .map_err(DomainError::from)
            .map_err(StorageError::InvalidTask)?;
        let milestone_id = MilestoneId::parse(&self.milestone_id)
            .map_err(DomainError::from)
            .map_err(StorageError::InvalidTask)?;
        let parent_id = self
            .parent_id
            .map(|parent_id| {
                TaskId::parse(&parent_id)
                    .map_err(DomainError::from)
                    .map_err(StorageError::InvalidTask)
            })
            .transpose()?;
        let status = TaskStatus::from_str(&self.status).map_err(StorageError::InvalidTask)?;
        let priority = TaskPriority::parse(&self.priority).map_err(StorageError::InvalidTask)?;
        Task::try_from(domain::TaskParts {
            id,
            milestone_id,
            parent_id,
            title: self.title,
            description: self.description,
            status,
            priority,
            position: self.position,
            handoff: self.handoff,
            evidence: self.evidence,
        })
        .map_err(StorageError::InvalidTask)
    }
}

pub fn find(conn: &Connection, id: &TaskId) -> Result<Option<Task>> {
    read_one(conn, id)
}

pub fn list(conn: &Connection) -> Result<Vec<Task>> {
    let mut statement = conn.prepare(
        "SELECT id, milestone_id, parent_id, title, description, status, priority, position, handoff, evidence
         FROM tasks ORDER BY milestone_id, position, id",
    )?;
    let rows = statement.query_map([], row_to_raw_task)?;
    let raw_tasks = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    raw_tasks.into_iter().map(|r| r.decode()).collect()
}

pub fn create(
    connection: &mut Connection, milestone_id: MilestoneId, parent_id: Option<TaskId>, title: String,
    description: String, priority: TaskPriority, position: i64,
) -> Result<Task> {
    create_with_dependencies(
        connection,
        TaskCreate { milestone_id, parent_id, title, description, priority, position, blockers: Vec::new() },
    )
}

pub fn create_with_dependencies(connection: &mut Connection, create: TaskCreate) -> Result<Task> {
    let TaskCreate { milestone_id, parent_id, title, description, priority, position, blockers } = create;
    let task = Task::new(milestone_id, parent_id, title, description, priority, position)
        .map_err(StorageError::InvalidTask)?;
    let transaction = connection.transaction()?;
    ensure_milestone_exists(&transaction, task.milestone_id)?;
    if let Some(parent_id) = task.parent_id {
        let parent = read_one(&transaction, &parent_id)?
            .ok_or_else(|| StorageError::TaskNotFound { id: parent_id.to_string() })?;
        validate_parent_milestone(task.id, task.milestone_id, parent.id, parent.milestone_id)?;
    }
    insert_task(&transaction, &task)?;
    for blocker_id in blockers {
        dependencies::add_to_transaction(&transaction, task.id, blocker_id)?;
    }
    transaction.commit()?;
    Ok(task)
}

pub fn update(conn: &mut Connection, id: TaskId, update: TaskUpdate) -> Result<Task> {
    let TaskUpdate { title, description, priority, position, milestone_id, parent_change } = update;
    if title.is_none()
        && description.is_none()
        && priority.is_none()
        && position.is_none()
        && milestone_id.is_none()
        && parent_change.is_none()
    {
        return Err(StorageError::InvalidTask(DomainError::NoFieldsToUpdate {
            entity: "task",
        }));
    }
    if let Some(title) = &title {
        domain::validate_title(title).map_err(StorageError::InvalidTask)?;
    }
    if let Some(position) = position {
        domain::validate_position(position).map_err(StorageError::InvalidTask)?;
    }

    let transaction = conn.transaction()?;
    let current = read_one(&transaction, &id)?.ok_or_else(|| StorageError::TaskNotFound { id: id.to_string() })?;
    let all_tasks = list(&transaction)?;
    let by_id = all_tasks
        .into_iter()
        .map(|task| (task.id, task))
        .collect::<HashMap<_, _>>();
    let subtree = collect_subtree(id, &by_id)?;
    for descendant_id in &subtree {
        let descendant = by_id
            .get(descendant_id)
            .ok_or_else(|| StorageError::TaskNotFound { id: descendant_id.to_string() })?;
        if descendant.milestone_id != current.milestone_id {
            return Err(StorageError::InvalidTask(DomainError::SubtreeDifferentMilestones {
                task: id.to_string(),
            }));
        }
    }

    let next_milestone = milestone_id.unwrap_or(current.milestone_id);
    ensure_milestone_exists(&transaction, next_milestone)?;
    let next_parent = parent_change.unwrap_or(current.parent_id);
    if let Some(parent_id) = next_parent {
        if parent_id == id {
            return Err(StorageError::InvalidTask(DomainError::SelfParent {
                task: id.to_string(),
            }));
        }
        let parent = by_id
            .get(&parent_id)
            .ok_or_else(|| StorageError::TaskNotFound { id: parent_id.to_string() })?;
        validate_parent_milestone(id, next_milestone, parent.id, parent.milestone_id)?;
        if subtree.contains(&parent_id) {
            return Err(StorageError::InvalidTask(DomainError::ParentCycle {
                task: id.to_string(),
                parent: parent_id.to_string(),
            }));
        }
        validate_parent_chain(id, parent_id, &by_id)?;
    }

    let next_title = title.as_deref().unwrap_or(&current.title);
    let next_description = description.as_deref().unwrap_or(&current.description);
    let next_priority = priority.unwrap_or(current.priority);
    let next_position = position.unwrap_or(current.position);

    for descendant_id in &subtree {
        transaction.execute(
            "UPDATE tasks SET milestone_id = ?1 WHERE id = ?2",
            params![next_milestone.to_string(), descendant_id.to_string()],
        )?;
    }
    transaction.execute(
        "UPDATE tasks
         SET milestone_id = ?1, parent_id = ?2, title = ?3, description = ?4, priority = ?5, position = ?6
         WHERE id = ?7",
        params![
            next_milestone.to_string(),
            next_parent.map(|parent_id| parent_id.to_string()),
            next_title,
            next_description,
            next_priority.as_str(),
            next_position,
            id.to_string(),
        ],
    )?;
    transaction.commit()?;

    Ok(Task {
        id,
        milestone_id: next_milestone,
        parent_id: next_parent,
        title: next_title.to_owned(),
        description: next_description.to_owned(),
        status: current.status,
        priority: next_priority,
        position: next_position,
        handoff: current.handoff,
        evidence: current.evidence,
    })
}

pub fn transition(
    connection: &mut Connection, id: TaskId, action: TaskAction, allow_open_children: bool, evidence: Option<String>,
) -> Result<Task> {
    let transaction = connection.transaction()?;
    let current = read_one(&transaction, &id)?.ok_or_else(|| StorageError::TaskNotFound { id: id.to_string() })?;
    let next_status = current.status.apply(action).map_err(StorageError::InvalidTask)?;
    if next_status == current.status && evidence.is_none() {
        return Ok(current);
    }
    if matches!(action, TaskAction::Complete | TaskAction::Cancel) && !allow_open_children {
        let has_open_descendants: bool = transaction.query_row(
            "WITH RECURSIVE descendants(id, status) AS (
                 SELECT id, status FROM tasks WHERE parent_id = ?1
                 UNION ALL
                 SELECT task.id, task.status FROM tasks task
                 JOIN descendants parent ON task.parent_id = parent.id
             )
             SELECT EXISTS (SELECT 1 FROM descendants WHERE status NOT IN ('completed', 'cancelled'))",
            params![id.to_string()],
            |row| row.get(0),
        )?;
        if has_open_descendants {
            return Err(StorageError::InvalidTask(DomainError::OpenDescendants {
                entity: "task",
                id: id.to_string(),
                action: action.as_str(),
            }));
        }
    }
    if next_status == current.status {
        transaction.execute(
            "UPDATE tasks SET evidence = ?1 WHERE id = ?2",
            params![evidence.as_deref().unwrap_or(&current.evidence), id.to_string()],
        )?;
    } else {
        transaction.execute(
            "UPDATE tasks SET status = ?1, evidence = COALESCE(?2, evidence) WHERE id = ?3",
            params![next_status.as_str(), evidence, id.to_string()],
        )?;
    }
    transaction.commit()?;
    Ok(Task { status: next_status, evidence: evidence.unwrap_or(current.evidence), ..current })
}

/// Store a handoff note and park an in-progress task without a partial update.
pub fn handoff(connection: &mut Connection, id: TaskId, note: String) -> Result<Task> {
    let transaction = connection.transaction()?;
    let current = read_one(&transaction, &id)?.ok_or_else(|| StorageError::TaskNotFound { id: id.to_string() })?;
    if current.status != crate::domain::TaskStatus::InProgress {
        return Err(StorageError::InvalidTask(DomainError::InvalidTransition {
            entity: "task",
            action: "handoff",
            from: current.status.as_str().to_owned(),
        }));
    }
    transaction.execute(
        "UPDATE tasks SET handoff = ?1, status = 'parked' WHERE id = ?2",
        params![note, id.to_string()],
    )?;
    transaction.commit()?;
    Ok(Task { status: crate::domain::TaskStatus::Parked, handoff: note, ..current })
}

fn read_one(conn: &Connection, id: &TaskId) -> Result<Option<Task>> {
    let raw_task = conn
        .query_row(
            "SELECT id, milestone_id, parent_id, title, description, status, priority, position, handoff, evidence
             FROM tasks WHERE id = ?1",
            params![id.to_string()],
            row_to_raw_task,
        )
        .optional()?;
    raw_task.map(|r| r.decode()).transpose()
}

pub(super) fn row_to_raw_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawTask> {
    Ok(RawTask {
        id: row.get(0)?,
        milestone_id: row.get(1)?,
        parent_id: row.get(2)?,
        title: row.get(3)?,
        description: row.get(4)?,
        status: row.get(5)?,
        priority: row.get(6)?,
        position: row.get(7)?,
        handoff: row.get(8)?,
        evidence: row.get(9)?,
    })
}

fn insert_task(connection: &Connection, task: &Task) -> Result<()> {
    connection.execute(
        "INSERT INTO tasks
         (id, milestone_id, parent_id, title, description, status, priority, position, handoff, evidence)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            task.id.to_string(),
            task.milestone_id.to_string(),
            task.parent_id.map(|parent_id| parent_id.to_string()),
            task.title,
            task.description,
            task.status.as_str(),
            task.priority.as_str(),
            task.position,
            task.handoff,
            task.evidence,
        ],
    )?;
    Ok(())
}

fn ensure_milestone_exists(connection: &Connection, milestone_id: MilestoneId) -> Result<()> {
    let exists: Option<String> = connection
        .query_row(
            "SELECT id FROM milestones WHERE id = ?1",
            params![milestone_id.to_string()],
            |row| row.get(0),
        )
        .optional()?;
    if exists.is_none() {
        return Err(StorageError::MilestoneNotFound { id: milestone_id.to_string() });
    }
    Ok(())
}

fn validate_parent_milestone(
    task_id: TaskId, task_milestone_id: MilestoneId, parent_id: TaskId, parent_milestone_id: MilestoneId,
) -> Result<()> {
    if task_milestone_id != parent_milestone_id {
        return Err(StorageError::InvalidTask(DomainError::DifferentMilestone {
            task: task_id.to_string(),
            parent: parent_id.to_string(),
        }));
    }
    Ok(())
}

fn collect_subtree(root: TaskId, tasks: &HashMap<TaskId, Task>) -> Result<Vec<TaskId>> {
    let mut children = HashMap::<TaskId, Vec<TaskId>>::new();
    for task in tasks.values() {
        if let Some(parent_id) = task.parent_id {
            children.entry(parent_id).or_default().push(task.id);
        }
    }
    for child_ids in children.values_mut() {
        child_ids.sort_unstable();
    }

    let mut subtree = Vec::new();
    let mut visiting = HashSet::new();
    collect_children(root, &children, &mut visiting, &mut subtree)?;
    Ok(subtree)
}

fn collect_children(
    task_id: TaskId, children: &HashMap<TaskId, Vec<TaskId>>, visiting: &mut HashSet<TaskId>, subtree: &mut Vec<TaskId>,
) -> Result<()> {
    if !visiting.insert(task_id) {
        return Err(StorageError::InvalidTask(DomainError::ParentCycle {
            task: task_id.to_string(),
            parent: task_id.to_string(),
        }));
    }
    subtree.push(task_id);
    if let Some(child_ids) = children.get(&task_id) {
        for child_id in child_ids {
            collect_children(*child_id, children, visiting, subtree)?;
        }
    }
    visiting.remove(&task_id);
    Ok(())
}

fn validate_parent_chain(root: TaskId, parent_id: TaskId, tasks: &HashMap<TaskId, Task>) -> Result<()> {
    let mut seen = HashSet::new();
    let mut current = Some(parent_id);
    while let Some(task_id) = current {
        if task_id == root || !seen.insert(task_id) {
            return Err(StorageError::InvalidTask(DomainError::ParentCycle {
                task: root.to_string(),
                parent: parent_id.to_string(),
            }));
        }
        current = tasks.get(&task_id).and_then(|task| task.parent_id);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{Database, StorageError, TaskUpdate};
    use crate::domain::{self, DomainError, TaskAction, TaskPriority, TaskStatus};

    fn graph() -> (Database, domain::MilestoneId, domain::MilestoneId) {
        let mut database = Database::open_in_memory().expect("database opens");
        let epic = database
            .create_epic("Epic".to_owned(), String::new(), "spec.md".to_owned(), None)
            .expect("epic creates");
        let first = database
            .create_milestone(epic.id, "First".to_owned(), String::new(), 0)
            .expect("first milestone creates");
        let second = database
            .create_milestone(epic.id, "Second".to_owned(), String::new(), 1)
            .expect("second milestone creates");
        (database, first.id, second.id)
    }

    #[test]
    fn tasks_store_subtasks_and_reject_cross_milestone_parents() {
        let (mut database, first, second) = graph();
        let parent = database
            .create_task(first, None, "Parent".to_owned(), String::new(), TaskPriority::High, 10)
            .expect("parent creates");
        let child = database
            .create_task(
                first,
                Some(parent.id),
                "Child".to_owned(),
                "- prose checkbox".to_owned(),
                TaskPriority::Normal,
                10,
            )
            .expect("child creates");
        assert_eq!(child.parent_id, Some(parent.id));
        assert_eq!(database.tasks().expect("tasks list").len(), 2);

        let error = database
            .create_task(
                second,
                Some(parent.id),
                "Invalid".to_owned(),
                String::new(),
                TaskPriority::Normal,
                0,
            )
            .expect_err("cross-milestone parent is rejected");
        assert!(matches!(
            error,
            StorageError::InvalidTask(DomainError::DifferentMilestone { .. })
        ));
    }

    #[test]
    fn moving_a_parent_moves_its_subtree_atomically_and_cycle_attempts_do_not_change_rows() {
        let (mut database, first, second) = graph();
        let parent = database
            .create_task(first, None, "Parent".to_owned(), String::new(), TaskPriority::Normal, 0)
            .expect("parent creates");
        let child = database
            .create_task(
                first,
                Some(parent.id),
                "Child".to_owned(),
                String::new(),
                TaskPriority::Normal,
                0,
            )
            .expect("child creates");
        let grandchild = database
            .create_task(
                first,
                Some(child.id),
                "Grandchild".to_owned(),
                String::new(),
                TaskPriority::Normal,
                0,
            )
            .expect("grandchild creates");

        let moved = database
            .update_task(
                parent.id,
                TaskUpdate { milestone_id: Some(second), parent_change: Some(None), ..TaskUpdate::default() },
            )
            .expect("root subtree moves");
        assert_eq!(moved.milestone_id, second);
        assert_eq!(
            database
                .task(child.id)
                .expect("child reads")
                .expect("child exists")
                .milestone_id,
            second
        );
        assert_eq!(
            database
                .task(grandchild.id)
                .expect("grandchild reads")
                .expect("grandchild exists")
                .milestone_id,
            second
        );

        let before = database.tasks().expect("tasks snapshot");
        let error = database
            .update_task(
                child.id,
                TaskUpdate { parent_change: Some(Some(grandchild.id)), ..TaskUpdate::default() },
            )
            .expect_err("descendant parent is rejected");
        assert!(matches!(
            error,
            StorageError::InvalidTask(DomainError::ParentCycle { .. })
        ));
        assert_eq!(database.tasks().expect("tasks after failure"), before);
    }

    #[test]
    fn invalid_task_updates_do_not_write() {
        let (mut database, first, _) = graph();
        let task = database
            .create_task(first, None, "Task".to_owned(), String::new(), TaskPriority::Normal, 0)
            .expect("task creates");
        let before = database.task(task.id).expect("task reads");
        let error = database
            .update_task(
                task.id,
                TaskUpdate { title: Some("  ".to_owned()), ..TaskUpdate::default() },
            )
            .expect_err("empty title is rejected");
        assert!(matches!(error, StorageError::InvalidTask(DomainError::EmptyTitle)));
        assert_eq!(database.task(task.id).expect("task reads"), before);
    }

    #[test]
    fn task_lifecycle_transitions_and_failures_are_atomic() {
        let (mut database, milestone, _) = graph();
        let task = database
            .create_task(
                milestone,
                None,
                "Task".to_owned(),
                String::new(),
                TaskPriority::Normal,
                0,
            )
            .expect("task creates");

        let parked = database
            .transition_task(task.id, TaskAction::Park, false)
            .expect("pending task parks");
        assert_eq!(parked.status, TaskStatus::Parked);
        let error = database
            .transition_task(task.id, TaskAction::Complete, false)
            .expect_err("parked task cannot complete");
        assert!(matches!(
            error,
            StorageError::InvalidTask(DomainError::InvalidTransition { .. })
        ));
        assert_eq!(
            database.task(task.id).expect("task reads").expect("task exists").status,
            TaskStatus::Parked
        );

        assert_eq!(
            database
                .transition_task(task.id, TaskAction::Unpark, false)
                .expect("task unparks")
                .status,
            TaskStatus::Pending
        );
        assert_eq!(
            database
                .transition_task(task.id, TaskAction::Start, false)
                .expect("task starts")
                .status,
            TaskStatus::InProgress
        );
        assert_eq!(
            database
                .transition_task(task.id, TaskAction::Complete, false)
                .expect("task completes")
                .status,
            TaskStatus::Completed
        );
        assert_eq!(
            database
                .transition_task(task.id, TaskAction::Complete, false)
                .expect("complete repeats")
                .status,
            TaskStatus::Completed
        );
        assert!(database.transition_task(task.id, TaskAction::Cancel, false).is_err());
        assert_eq!(
            database.task(task.id).expect("task reads").expect("task exists").status,
            TaskStatus::Completed
        );
    }

    #[test]
    fn handoff_parks_active_work_and_completion_stores_evidence_atomically() {
        let (mut database, milestone, _) = graph();
        let task = database
            .create_task(
                milestone,
                None,
                "Task".to_owned(),
                String::new(),
                TaskPriority::Normal,
                0,
            )
            .expect("task creates");
        let error = database
            .handoff_task(task.id, "should fail".to_owned())
            .expect_err("pending work cannot be handed off");
        assert!(matches!(
            error,
            StorageError::InvalidTask(DomainError::InvalidTransition { .. })
        ));
        database
            .transition_task(task.id, TaskAction::Start, false)
            .expect("task starts");
        let parked = database
            .handoff_task(task.id, "resume here".to_owned())
            .expect("handoff parks task");
        assert_eq!(parked.status, TaskStatus::Parked);
        assert_eq!(parked.handoff, "resume here");
        database
            .transition_task(task.id, TaskAction::Unpark, false)
            .expect("task unparks");
        database
            .transition_task(task.id, TaskAction::Start, false)
            .expect("task restarts");
        let completed = database
            .complete_task(task.id, false, Some("verified".to_owned()))
            .expect("task completes");
        assert_eq!(completed.evidence, "verified");
        assert_eq!(completed.handoff, "resume here");
        let stored = database.task(task.id).expect("task reads").expect("task exists");
        assert_eq!(stored.evidence, "verified");
        assert_eq!(stored.handoff, "resume here");
    }

    #[test]
    fn evidence_does_not_bypass_open_child_guards() {
        let (mut database, milestone, _) = graph();
        let parent = database
            .create_task(
                milestone,
                None,
                "Parent".to_owned(),
                String::new(),
                TaskPriority::Normal,
                0,
            )
            .expect("parent creates");
        database
            .create_task(
                milestone,
                Some(parent.id),
                "Child".to_owned(),
                String::new(),
                TaskPriority::Normal,
                0,
            )
            .expect("child creates");
        assert!(matches!(
            database.complete_task(parent.id, false, Some("must not save".to_owned())),
            Err(StorageError::InvalidTask(DomainError::OpenDescendants { .. }))
        ));
        assert_eq!(
            database
                .task(parent.id)
                .expect("task reads")
                .expect("task exists")
                .evidence,
            ""
        );
    }

    #[test]
    fn parent_terminal_transitions_require_an_override_and_never_cascade() {
        let (mut database, milestone, _) = graph();
        let parent = database
            .create_task(
                milestone,
                None,
                "Parent".to_owned(),
                String::new(),
                TaskPriority::Normal,
                0,
            )
            .expect("parent creates");
        let child = database
            .create_task(
                milestone,
                Some(parent.id),
                "Child".to_owned(),
                String::new(),
                TaskPriority::Normal,
                0,
            )
            .expect("child creates");

        let error = database
            .transition_task(parent.id, TaskAction::Cancel, false)
            .expect_err("open child blocks cancellation");
        assert!(matches!(
            error,
            StorageError::InvalidTask(DomainError::OpenDescendants { .. })
        ));
        assert_eq!(
            database
                .task(parent.id)
                .expect("parent reads")
                .expect("parent exists")
                .status,
            TaskStatus::Pending
        );

        let cancelled = database
            .transition_task(parent.id, TaskAction::Cancel, true)
            .expect("override cancels parent");
        assert_eq!(cancelled.status, TaskStatus::Cancelled);
        assert_eq!(
            database
                .task(child.id)
                .expect("child reads")
                .expect("child exists")
                .status,
            TaskStatus::Pending
        );
        assert_eq!(
            database
                .transition_task(parent.id, TaskAction::Cancel, false)
                .expect("terminal repetition stays idempotent")
                .status,
            TaskStatus::Cancelled
        );
    }
}
