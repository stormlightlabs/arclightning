use rusqlite::{Connection, OptionalExtension, Transaction, params, params_from_iter};

use arcl_core::domain::{DomainError, Task, TaskDependency, TaskId};

use super::{ReadyFilter, Result, StorageError, tasks};

/// Return every stored dependency in deterministic task/blocker order.
pub fn list(conn: &Connection) -> Result<Vec<TaskDependency>> {
    let mut statement = conn.prepare(
        "SELECT task_id, blocker_id
         FROM task_dependencies
         ORDER BY task_id, blocker_id",
    )?;
    let rows = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
    rows.map(|row| {
        let (task_id, blocker_id) = row?;
        decode_dependency(&task_id, &blocker_id)
    })
    .collect()
}

/// Return the blockers directly attached to one task.
pub fn list_for_task(conn: &Connection, task_id: TaskId) -> Result<Vec<TaskDependency>> {
    let mut statement = conn.prepare(
        "SELECT task_id, blocker_id
         FROM task_dependencies
         WHERE task_id = ?1
         ORDER BY blocker_id",
    )?;
    let rows = statement.query_map([task_id.to_string()], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    rows.map(|row| {
        let (task_id, blocker_id) = row?;
        decode_dependency(&task_id, &blocker_id)
    })
    .collect()
}

/// Add a dependency after validating both targets, uniqueness, and cycles.
pub fn add(connection: &mut Connection, task_id: TaskId, blocker_id: TaskId) -> Result<TaskDependency> {
    let transaction = connection.transaction()?;
    let dependency = insert_validated(&transaction, task_id, blocker_id)?;
    transaction.commit()?;
    Ok(dependency)
}

/// Add one dependency inside an existing task-creation transaction.
pub fn add_to_transaction(
    transaction: &Transaction<'_>, task_id: TaskId, blocker_id: TaskId,
) -> Result<TaskDependency> {
    insert_validated(transaction, task_id, blocker_id)
}

/// Remove an existing dependency after validating both task targets.
pub fn remove(connection: &mut Connection, task_id: TaskId, blocker_id: TaskId) -> Result<TaskDependency> {
    let dependency = TaskDependency::new(task_id, blocker_id).map_err(StorageError::InvalidDependency)?;
    let transaction = connection.transaction()?;
    ensure_task_exists(&transaction, task_id)?;
    ensure_task_exists(&transaction, blocker_id)?;

    let exists: bool = transaction.query_row(
        "SELECT EXISTS(
             SELECT 1 FROM task_dependencies WHERE task_id = ?1 AND blocker_id = ?2
         )",
        params![task_id.to_string(), blocker_id.to_string()],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(StorageError::DependencyNotFound { task: task_id.to_string(), blocker: blocker_id.to_string() });
    }

    transaction.execute(
        "DELETE FROM task_dependencies WHERE task_id = ?1 AND blocker_id = ?2",
        params![task_id.to_string(), blocker_id.to_string()],
    )?;
    transaction.commit()?;
    Ok(dependency)
}

/// Return tasks that have at least one direct blocker which is not completed.
pub fn blocked(conn: &Connection) -> Result<Vec<Task>> {
    let mut statement = conn.prepare(
        "SELECT id, milestone_id, parent_id, title, description, status, priority, position, plan_key, handoff, evidence
         FROM tasks AS task
         WHERE EXISTS (
             SELECT 1
             FROM task_dependencies AS dependency
             LEFT JOIN tasks AS blocker ON blocker.id = dependency.blocker_id
             WHERE dependency.task_id = task.id
               AND (blocker.id IS NULL OR blocker.status <> 'completed')
         )
         ORDER BY id",
    )?;
    let rows = statement.query_map([], tasks::row_to_raw_task)?;
    let raw_tasks = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    raw_tasks.into_iter().map(tasks::RawTask::decode).collect()
}

/// Return tasks that satisfy every derived ready-work rule.
pub fn ready(conn: &Connection, filter: &ReadyFilter) -> Result<Vec<Task>> {
    let mut sql = String::from(
        "WITH RECURSIVE ancestors(task_id, ancestor_id) AS (
             SELECT id, parent_id FROM tasks WHERE parent_id IS NOT NULL
             UNION
             SELECT ancestors.task_id, parent.parent_id
             FROM ancestors
             JOIN tasks AS parent ON parent.id = ancestors.ancestor_id
             WHERE parent.parent_id IS NOT NULL
         )
         SELECT task.id, task.milestone_id, task.parent_id, task.title, task.description,
                task.status, task.priority, task.position, task.plan_key, task.handoff, task.evidence
         FROM tasks AS task
         JOIN milestones AS milestone ON milestone.id = task.milestone_id
         JOIN epics AS epic ON epic.id = milestone.epic_id
         LEFT JOIN releases AS release ON release.id = epic.release_id
         WHERE task.status = 'pending'
           AND milestone.status = 'open'
           AND epic.status = 'open'
           AND (epic.release_id IS NULL OR release.status = 'open')
           AND NOT EXISTS (
               SELECT 1 FROM tasks AS child WHERE child.parent_id = task.id
           )
           AND NOT EXISTS (
               SELECT 1
               FROM ancestors
               JOIN tasks AS ancestor ON ancestor.id = ancestors.ancestor_id
               WHERE ancestors.task_id = task.id
                 AND ancestor.status IN ('parked', 'completed', 'cancelled')
           )
           AND NOT EXISTS (
               SELECT 1
               FROM task_dependencies AS dependency
               LEFT JOIN tasks AS blocker ON blocker.id = dependency.blocker_id
               WHERE dependency.task_id = task.id
                 AND (blocker.id IS NULL OR blocker.status <> 'completed')
           )",
    );

    let mut values = Vec::new();
    if !filter.priorities.is_empty() {
        let placeholders = (0..filter.priorities.len()).map(|_| "?").collect::<Vec<_>>().join(", ");
        sql.push_str(" AND task.priority IN (");
        sql.push_str(&placeholders);
        sql.push(')');
        values.extend(filter.priorities.iter().map(|priority| priority.as_str().to_owned()));
    }
    if let Some(release_id) = filter.release_id {
        sql.push_str(" AND epic.release_id = ?");
        values.push(release_id.to_string());
    }
    if let Some(epic_id) = filter.epic_id {
        sql.push_str(" AND epic.id = ?");
        values.push(epic_id.to_string());
    }
    if let Some(milestone_id) = filter.milestone_id {
        sql.push_str(" AND milestone.id = ?");
        values.push(milestone_id.to_string());
    }
    if let Some(parent_id) = filter.parent_id {
        sql.push_str(" AND task.parent_id = ?");
        values.push(parent_id.to_string());
    }
    sql.push_str(
        " ORDER BY CASE task.priority
                       WHEN 'critical' THEN 0
                       WHEN 'high' THEN 1
                       WHEN 'normal' THEN 2
                       WHEN 'low' THEN 3
                   END,
                   milestone.position, task.position, task.id",
    );

    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(values.iter()), tasks::row_to_raw_task)?;
    let raw_tasks = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    raw_tasks.into_iter().map(tasks::RawTask::decode).collect()
}

/// Return whether one task has an unfinished direct blocker.
pub fn is_blocked(conn: &Connection, task_id: TaskId) -> Result<bool> {
    ensure_task_exists(conn, task_id)?;
    Ok(conn.query_row(
        "SELECT EXISTS(
             SELECT 1
             FROM task_dependencies AS dependency
             LEFT JOIN tasks AS blocker ON blocker.id = dependency.blocker_id
             WHERE dependency.task_id = ?1
               AND (blocker.id IS NULL OR blocker.status <> 'completed')
         )",
        [task_id.to_string()],
        |row| row.get(0),
    )?)
}

fn insert_validated(conn: &Connection, task_id: TaskId, blocker_id: TaskId) -> Result<TaskDependency> {
    let dependency = TaskDependency::new(task_id, blocker_id).map_err(StorageError::InvalidDependency)?;
    ensure_task_exists(conn, task_id)?;
    ensure_task_exists(conn, blocker_id)?;

    let duplicate: bool = conn.query_row(
        "SELECT EXISTS(
             SELECT 1 FROM task_dependencies WHERE task_id = ?1 AND blocker_id = ?2
         )",
        params![task_id.to_string(), blocker_id.to_string()],
        |row| row.get(0),
    )?;
    if duplicate {
        return Err(StorageError::InvalidDependency(DomainError::DuplicateDependency {
            task: task_id.to_string(),
            blocker: blocker_id.to_string(),
        }));
    }

    let creates_cycle: bool = conn.query_row(
        "WITH RECURSIVE reachable(id) AS (
             SELECT ?2
             UNION
             SELECT dependency.blocker_id
             FROM task_dependencies AS dependency
             JOIN reachable ON dependency.task_id = reachable.id
         )
         SELECT EXISTS(SELECT 1 FROM reachable WHERE id = ?1)",
        params![task_id.to_string(), blocker_id.to_string()],
        |row| row.get(0),
    )?;
    if creates_cycle {
        return Err(StorageError::InvalidDependency(DomainError::DependencyCycle {
            task: task_id.to_string(),
            blocker: blocker_id.to_string(),
        }));
    }

    conn.execute(
        "INSERT INTO task_dependencies (task_id, blocker_id) VALUES (?1, ?2)",
        params![task_id.to_string(), blocker_id.to_string()],
    )?;
    Ok(dependency)
}

fn ensure_task_exists(conn: &Connection, task_id: TaskId) -> Result<()> {
    let exists: Option<String> = conn
        .query_row("SELECT id FROM tasks WHERE id = ?1", [task_id.to_string()], |row| {
            row.get(0)
        })
        .optional()?;
    if exists.is_none() {
        return Err(StorageError::TaskNotFound { id: task_id.to_string() });
    }
    Ok(())
}

fn decode_dependency(task_id: &str, blocker_id: &str) -> Result<TaskDependency> {
    let task_id = TaskId::parse(task_id)
        .map_err(DomainError::from)
        .map_err(StorageError::InvalidDependency)?;
    let blocker_id = TaskId::parse(blocker_id)
        .map_err(DomainError::from)
        .map_err(StorageError::InvalidDependency)?;
    TaskDependency::new(task_id, blocker_id).map_err(StorageError::InvalidDependency)
}

#[cfg(test)]
mod tests {
    use crate::storage::{Database, StorageError};
    use arcl_core::domain::{DomainError, TaskAction, TaskPriority};

    fn graph() -> (Database, arcl_core::domain::MilestoneId) {
        let mut database = Database::open_in_memory().expect("database opens");
        let epic = database
            .create_epic("Epic".to_owned(), String::new(), "spec.md".to_owned(), None)
            .expect("epic creates");
        let milestone = database
            .create_milestone(epic.id, "Milestone".to_owned(), String::new(), 0)
            .expect("milestone creates");
        (database, milestone.id)
    }

    #[test]
    fn dependencies_validate_targets_uniqueness_and_transitive_cycles_atomically() {
        let (mut database, milestone) = graph();
        let first = database
            .create_task(
                milestone,
                None,
                "First".to_owned(),
                String::new(),
                TaskPriority::Normal,
                0,
            )
            .expect("first creates");
        let second = database
            .create_task(
                milestone,
                None,
                "Second".to_owned(),
                String::new(),
                TaskPriority::Normal,
                1,
            )
            .expect("second creates");
        let third = database
            .create_task(
                milestone,
                None,
                "Third".to_owned(),
                String::new(),
                TaskPriority::Normal,
                2,
            )
            .expect("third creates");

        database
            .add_dependency(first.id, second.id)
            .expect("first dependency adds");
        database
            .add_dependency(second.id, third.id)
            .expect("second dependency adds");
        let error = database
            .add_dependency(third.id, first.id)
            .expect_err("transitive cycle is rejected");
        assert!(matches!(
            error,
            StorageError::InvalidDependency(DomainError::DependencyCycle { .. })
        ));
        assert_eq!(database.task_dependencies().expect("dependencies list").len(), 2);

        let duplicate = database
            .add_dependency(first.id, second.id)
            .expect_err("duplicate is rejected");
        assert!(matches!(
            duplicate,
            StorageError::InvalidDependency(DomainError::DuplicateDependency { .. })
        ));
        let missing = arcl_core::domain::TaskId::new();
        assert!(matches!(
            database.add_dependency(first.id, missing),
            Err(StorageError::TaskNotFound { .. })
        ));
        assert!(matches!(
            database.add_dependency(first.id, first.id),
            Err(StorageError::InvalidDependency(DomainError::SelfDependency { .. }))
        ));
    }

    #[test]
    fn cancelled_blockers_stay_blocked_until_completed() {
        let (mut database, milestone) = graph();
        let blocker = database
            .create_task(
                milestone,
                None,
                "Blocker".to_owned(),
                String::new(),
                TaskPriority::Normal,
                0,
            )
            .expect("blocker creates");
        let task = database
            .create_task(
                milestone,
                None,
                "Task".to_owned(),
                String::new(),
                TaskPriority::Normal,
                1,
            )
            .expect("task creates");
        database.add_dependency(task.id, blocker.id).expect("dependency adds");
        assert!(database.task_is_blocked(task.id).expect("blocked state reads"));
        database
            .transition_task(blocker.id, TaskAction::Cancel, false)
            .expect("blocker cancels");
        assert!(
            database
                .task_is_blocked(task.id)
                .expect("cancelled blocker stays unsatisfied")
        );
    }

    #[test]
    fn ready_work_applies_graph_rules_and_deterministic_ordering() {
        let mut database = Database::open_in_memory().expect("database opens");
        let epic = database
            .create_epic("Epic".to_owned(), String::new(), "spec.md".to_owned(), None)
            .expect("epic creates");
        let later = database
            .create_milestone(epic.id, "Later".to_owned(), String::new(), 20)
            .expect("later milestone creates");
        let earlier = database
            .create_milestone(epic.id, "Earlier".to_owned(), String::new(), 10)
            .expect("earlier milestone creates");

        let normal = database
            .create_task(
                earlier.id,
                None,
                "Normal".to_owned(),
                String::new(),
                TaskPriority::Normal,
                0,
            )
            .expect("normal task creates");
        let critical = database
            .create_task(
                later.id,
                None,
                "Critical".to_owned(),
                String::new(),
                TaskPriority::Critical,
                99,
            )
            .expect("critical task creates");
        let high = database
            .create_task(
                earlier.id,
                None,
                "High".to_owned(),
                String::new(),
                TaskPriority::High,
                99,
            )
            .expect("high task creates");
        let blocker = database
            .create_task(
                earlier.id,
                None,
                "Blocker".to_owned(),
                String::new(),
                TaskPriority::Low,
                0,
            )
            .expect("blocker creates");
        let blocked = database
            .create_task(
                earlier.id,
                None,
                "Blocked".to_owned(),
                String::new(),
                TaskPriority::Critical,
                0,
            )
            .expect("blocked task creates");
        database
            .add_dependency(blocked.id, blocker.id)
            .expect("dependency adds");
        database
            .transition_task(blocker.id, TaskAction::Park, false)
            .expect("blocker parks");

        let parent = database
            .create_task(
                earlier.id,
                None,
                "Parent".to_owned(),
                String::new(),
                TaskPriority::Critical,
                1,
            )
            .expect("parent creates");
        let child = database
            .create_task(
                earlier.id,
                Some(parent.id),
                "Child".to_owned(),
                String::new(),
                TaskPriority::Critical,
                0,
            )
            .expect("child creates");
        let parked_parent = database
            .create_task(
                earlier.id,
                None,
                "Parked parent".to_owned(),
                String::new(),
                TaskPriority::Critical,
                2,
            )
            .expect("parked parent creates");
        let parked_child = database
            .create_task(
                earlier.id,
                Some(parked_parent.id),
                "Parked child".to_owned(),
                String::new(),
                TaskPriority::Critical,
                0,
            )
            .expect("parked child creates");
        database
            .transition_task(parked_parent.id, TaskAction::Park, false)
            .expect("parent parks");

        let ready = database.ready_tasks().expect("ready tasks query");
        assert_eq!(
            ready.iter().map(|task| task.id).collect::<Vec<_>>(),
            vec![child.id, critical.id, high.id, normal.id]
        );
        assert!(!ready.iter().any(|task| task.id == blocked.id));
        assert!(!ready.iter().any(|task| task.id == parent.id));
        assert!(!ready.iter().any(|task| task.id == parked_child.id));

        database
            .transition_task(blocker.id, TaskAction::Unpark, false)
            .expect("blocker unparks");
        database
            .transition_task(blocker.id, TaskAction::Complete, false)
            .expect("blocker completes");
        assert!(
            database
                .ready_tasks()
                .expect("ready tasks refresh")
                .iter()
                .any(|task| task.id == blocked.id)
        );
    }
}
