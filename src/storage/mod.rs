mod dependencies;
mod epics;
mod ideas;
mod migrations;
mod milestones;
mod promotions;
mod queries;
mod releases;
mod tasks;

pub use promotions::Promotion;
pub use queries::{CheckReport, ContextView, Graph, ListFilter, ListItem, Readiness, ShowView, TaskView, TreeNode};
pub use tasks::{TaskCreate, TaskUpdate};

use std::{path::Path, time::Duration};

use rusqlite::Connection;
use thiserror::Error;

use crate::domain::{
    ContainerAction, DomainError, Epic, EpicId, Idea, IdeaId, Milestone, MilestoneId, Release, ReleaseId, Task,
    TaskAction, TaskDependency, TaskId, TaskPriority,
};

/// The newest SQLite schema version understood by the application.
pub const CURRENT_VERSION: i32 = 8;

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

pub type Result<T> = std::result::Result<T, StorageError>;

/// One exact file stored as the last successful snapshot base.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotBaseFile {
    /// The path relative to the snapshot root.
    pub path: String,
    /// The exact bytes observed after the successful export.
    pub content: Vec<u8>,
}

/// Infrastructure failures from the SQLite storage boundary.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("stored idea is invalid: {0}")]
    InvalidIdea(#[from] DomainError),
    #[error("stored release is invalid: {0}")]
    InvalidRelease(DomainError),
    #[error("stored epic is invalid: {0}")]
    InvalidEpic(DomainError),
    #[error("stored milestone is invalid: {0}")]
    InvalidMilestone(DomainError),
    #[error("stored task is invalid: {0}")]
    InvalidTask(DomainError),
    #[error("stored task dependency is invalid: {0}")]
    InvalidDependency(DomainError),
    #[error("idea `{id}` was not found")]
    IdeaNotFound { id: String },
    #[error("release `{id}` was not found")]
    ReleaseNotFound { id: String },
    #[error("epic `{id}` was not found")]
    EpicNotFound { id: String },
    #[error("milestone `{id}` was not found")]
    MilestoneNotFound { id: String },
    #[error("task `{id}` was not found")]
    TaskNotFound { id: String },
    #[error("dependency from task `{task}` to blocker `{blocker}` was not found")]
    DependencyNotFound { task: String, blocker: String },
    #[error("spec path `{path}` is already linked to another epic")]
    DuplicateSpec { path: String },
    #[error("idea `{id}` cannot be promoted from status `{status}`")]
    IdeaNotPromotable { id: String, status: String },
    #[error("idea `{id}` has an inconsistent promotion relationship")]
    InconsistentPromotion { id: String },
    #[error("database user_version {found} is newer than this application supports (latest {latest})")]
    NewerDatabase { found: i32, latest: i32 },
    #[error("migration sequence is missing version {expected} before version {found}")]
    MigrationGap { expected: i32, found: i32 },
}

/// The single SQLite connection owner for one application command.
#[derive(Debug)]
pub struct Database {
    connection: Connection,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut connection = Connection::open(path)?;
        configure(&mut connection)?;
        migrations::apply(&mut connection)?;
        Ok(Self { connection })
    }

    pub fn open_in_memory() -> Result<Self> {
        let mut connection = Connection::open_in_memory()?;
        configure(&mut connection)?;
        migrations::apply(&mut connection)?;
        Ok(Self { connection })
    }

    pub fn schema_version(&self) -> Result<i32> {
        Ok(self
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    /// Read the exact files recorded by the last successful snapshot export.
    pub fn snapshot_base(&self) -> Result<Vec<SnapshotBaseFile>> {
        let mut statement = self
            .connection
            .prepare("SELECT path, content FROM snapshot_files ORDER BY path")?;
        let rows = statement.query_map([], |row| {
            Ok(SnapshotBaseFile { path: row.get(0)?, content: row.get(1)? })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Replace the complete work graph and refresh its snapshot base in one transaction.
    pub fn replace_graph_and_snapshot_base(&mut self, graph: &Graph, files: &[SnapshotBaseFile]) -> Result<()> {
        let transaction = self.connection.transaction()?;
        transaction.execute_batch(
            "DELETE FROM task_dependencies;
             DELETE FROM idea_promotions;
             UPDATE tasks SET parent_id = NULL;
             DELETE FROM tasks;
             DELETE FROM milestones;
             DELETE FROM epics;
             DELETE FROM releases;
             DELETE FROM ideas;",
        )?;

        for release in &graph.releases {
            transaction.execute(
                "INSERT INTO releases (id, title, description, status) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![
                    release.id.to_string(),
                    release.title,
                    release.description,
                    release.status.as_str()
                ],
            )?;
        }
        for idea in &graph.ideas {
            transaction.execute(
                "INSERT INTO ideas (id, title, description, status) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![idea.id.to_string(), idea.title, idea.description, idea.status.as_str()],
            )?;
        }
        for epic in &graph.epics {
            transaction.execute(
                "INSERT INTO epics (id, release_id, title, description, spec_path, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    epic.id.to_string(),
                    epic.release_id.map(|id| id.to_string()),
                    epic.title,
                    epic.description,
                    epic.spec_path,
                    epic.status.as_str(),
                ],
            )?;
        }
        for idea in &graph.ideas {
            if let Some(epic_id) = idea.promoted_to {
                transaction.execute(
                    "INSERT INTO idea_promotions (idea_id, epic_id) VALUES (?1, ?2)",
                    rusqlite::params![idea.id.to_string(), epic_id.to_string()],
                )?;
            }
        }
        for milestone in &graph.milestones {
            transaction.execute(
                "INSERT INTO milestones (id, epic_id, plan_key, title, description, status, position)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    milestone.id.to_string(),
                    milestone.epic_id.to_string(),
                    milestone.plan_key,
                    milestone.title,
                    milestone.description,
                    milestone.status.as_str(),
                    milestone.position,
                ],
            )?;
        }
        for task in &graph.tasks {
            transaction.execute(
                "INSERT INTO tasks
                 (id, milestone_id, parent_id, plan_key, title, description, status, priority, position, handoff, evidence)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                rusqlite::params![
                    task.id.to_string(),
                    task.milestone_id.to_string(),
                    Option::<String>::None,
                    task.plan_key,
                    task.title,
                    task.description,
                    task.status.as_str(),
                    task.priority.as_str(),
                    task.position,
                    task.handoff,
                    task.evidence,
                ],
            )?;
        }
        for task in &graph.tasks {
            if let Some(parent_id) = task.parent_id {
                transaction.execute(
                    "UPDATE tasks SET parent_id = ?2 WHERE id = ?1",
                    rusqlite::params![task.id.to_string(), parent_id.to_string()],
                )?;
            }
        }
        for dependency in &graph.dependencies {
            transaction.execute(
                "INSERT INTO task_dependencies (task_id, blocker_id) VALUES (?1, ?2)",
                rusqlite::params![dependency.task_id.to_string(), dependency.blocker_id.to_string()],
            )?;
        }

        transaction.execute("DELETE FROM snapshot_files", [])?;
        for file in files {
            transaction.execute(
                "INSERT INTO snapshot_files (path, content) VALUES (?1, ?2)",
                rusqlite::params![&file.path, &file.content],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Replace the stored snapshot base after filesystem export has completed.
    pub fn replace_snapshot_base(&mut self, files: &[SnapshotBaseFile]) -> Result<()> {
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM snapshot_files", [])?;
        for file in files {
            transaction.execute(
                "INSERT INTO snapshot_files (path, content) VALUES (?1, ?2)",
                rusqlite::params![&file.path, &file.content],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn idea(&self, id: IdeaId) -> Result<Option<Idea>> {
        ideas::find(&self.connection, &id)
    }

    pub fn ideas(&self) -> Result<Vec<Idea>> {
        ideas::list(&self.connection)
    }

    pub fn create_idea(&mut self, title: String, description: String) -> Result<Idea> {
        ideas::create(&mut self.connection, title, description)
    }

    pub fn update_idea(&mut self, id: IdeaId, title: Option<String>, description: Option<String>) -> Result<Idea> {
        ideas::update(&mut self.connection, id, title, description)
    }

    pub fn discard_idea(&mut self, id: IdeaId) -> Result<Idea> {
        ideas::discard(&mut self.connection, id)
    }

    /// Promote an idea into a linked epic in one transaction.
    pub fn promote_idea(
        &mut self, id: IdeaId, title: String, description: String, spec_path: String, release_id: Option<ReleaseId>,
    ) -> Result<Promotion> {
        promotions::promote(&mut self.connection, id, title, description, spec_path, release_id)
    }

    /// Read one release by its validated identifier.
    pub fn release(&self, id: ReleaseId) -> Result<Option<Release>> {
        releases::find(&self.connection, &id)
    }

    /// Read all releases in deterministic identifier order.
    pub fn releases(&self) -> Result<Vec<Release>> {
        releases::list(&self.connection)
    }

    /// Create an open release in a transaction.
    pub fn create_release(&mut self, title: String, description: String) -> Result<Release> {
        releases::create(&mut self.connection, title, description)
    }

    /// Update a release's title and/or Markdown description in a transaction.
    pub fn update_release(
        &mut self, id: ReleaseId, title: Option<String>, description: Option<String>,
    ) -> Result<Release> {
        releases::update(&mut self.connection, id, title, description)
    }

    /// Complete or cancel a release, guarding all non-terminal descendants.
    pub fn transition_release(
        &mut self, id: ReleaseId, action: ContainerAction, allow_open_children: bool,
    ) -> Result<Release> {
        releases::transition(&mut self.connection, id, action, allow_open_children)
    }

    /// Read one epic by its validated identifier.
    pub fn epic(&self, id: EpicId) -> Result<Option<Epic>> {
        epics::find(&self.connection, &id)
    }

    /// Read all epics in deterministic identifier order.
    pub fn epics(&self) -> Result<Vec<Epic>> {
        epics::list(&self.connection)
    }

    /// Create an open epic after validating its release association and unique spec path.
    pub fn create_epic(
        &mut self, title: String, description: String, spec_path: String, release_id: Option<ReleaseId>,
    ) -> Result<Epic> {
        epics::create(&mut self.connection, title, description, spec_path, release_id)
    }

    /// Update an epic without modifying the linked Markdown file.
    pub fn update_epic(
        &mut self, id: EpicId, title: Option<String>, description: Option<String>, spec_path: Option<String>,
        release_change: Option<Option<ReleaseId>>,
    ) -> Result<Epic> {
        epics::update(&mut self.connection, id, title, description, spec_path, release_change)
    }

    /// Complete or cancel an epic, guarding all non-terminal descendants.
    pub fn transition_epic(&mut self, id: EpicId, action: ContainerAction, allow_open_children: bool) -> Result<Epic> {
        epics::transition(&mut self.connection, id, action, allow_open_children)
    }

    /// Read one milestone by its validated identifier.
    pub fn milestone(&self, id: MilestoneId) -> Result<Option<Milestone>> {
        milestones::find(&self.connection, &id)
    }

    /// Read milestones in position order, breaking ties by ULID.
    pub fn milestones(&self) -> Result<Vec<Milestone>> {
        milestones::list(&self.connection)
    }

    /// Create an open milestone owned by an existing epic.
    pub fn create_milestone(
        &mut self, epic_id: EpicId, title: String, description: String, position: i64,
    ) -> Result<Milestone> {
        milestones::create(&mut self.connection, epic_id, title, description, position)
    }

    /// Update a milestone's title, Markdown description, and position atomically.
    pub fn update_milestone(
        &mut self, id: MilestoneId, title: Option<String>, description: Option<String>, position: Option<i64>,
    ) -> Result<Milestone> {
        milestones::update(&mut self.connection, id, title, description, position)
    }

    /// Complete or cancel a milestone, guarding all non-terminal tasks.
    pub fn transition_milestone(
        &mut self, id: MilestoneId, action: ContainerAction, allow_open_children: bool,
    ) -> Result<Milestone> {
        milestones::transition(&mut self.connection, id, action, allow_open_children)
    }

    /// Read one task or subtask by its validated identifier.
    pub fn task(&self, id: TaskId) -> Result<Option<Task>> {
        tasks::find(&self.connection, &id)
    }

    /// Read tasks in milestone, position, and ULID order.
    pub fn tasks(&self) -> Result<Vec<Task>> {
        tasks::list(&self.connection)
    }

    /// Create a pending task, validating its milestone and optional parent.
    pub fn create_task(
        &mut self, milestone_id: MilestoneId, parent_id: Option<TaskId>, title: String, description: String,
        priority: TaskPriority, position: i64,
    ) -> Result<Task> {
        tasks::create(
            &mut self.connection,
            milestone_id,
            parent_id,
            title,
            description,
            priority,
            position,
        )
    }

    /// Create a pending task and its blocking relationships atomically.
    pub fn create_task_with_dependencies(&mut self, create: TaskCreate) -> Result<Task> {
        tasks::create_with_dependencies(&mut self.connection, create)
    }

    /// Update a task and atomically move its complete descendant subtree when needed.
    pub fn update_task(&mut self, id: TaskId, update: TaskUpdate) -> Result<Task> {
        tasks::update(&mut self.connection, id, update)
    }

    /// Apply one task lifecycle action, guarding descendants for terminal transitions.
    pub fn transition_task(&mut self, id: TaskId, action: TaskAction, allow_open_children: bool) -> Result<Task> {
        tasks::transition(&mut self.connection, id, action, allow_open_children, None)
    }

    /// Complete a task and store optional Markdown evidence atomically.
    pub fn complete_task(&mut self, id: TaskId, allow_open_children: bool, evidence: Option<String>) -> Result<Task> {
        tasks::transition(
            &mut self.connection,
            id,
            TaskAction::Complete,
            allow_open_children,
            evidence,
        )
    }

    /// Store a resume note and park an in-progress task atomically.
    pub fn handoff_task(&mut self, id: TaskId, note: String) -> Result<Task> {
        tasks::handoff(&mut self.connection, id, note)
    }

    /// Add a validated blocking relationship in a transaction.
    pub fn add_dependency(&mut self, task_id: TaskId, blocker_id: TaskId) -> Result<TaskDependency> {
        dependencies::add(&mut self.connection, task_id, blocker_id)
    }

    /// Remove a validated blocking relationship in a transaction.
    pub fn remove_dependency(&mut self, task_id: TaskId, blocker_id: TaskId) -> Result<TaskDependency> {
        dependencies::remove(&mut self.connection, task_id, blocker_id)
    }

    /// Read every blocking relationship in deterministic order.
    pub fn task_dependencies(&self) -> Result<Vec<TaskDependency>> {
        dependencies::list(&self.connection)
    }

    /// Read the direct blockers attached to one task.
    pub fn dependencies(&self, task_id: TaskId) -> Result<Vec<TaskDependency>> {
        dependencies::list_for_task(&self.connection, task_id)
    }

    /// Compute whether a task has an unfinished direct blocker.
    pub fn task_is_blocked(&self, task_id: TaskId) -> Result<bool> {
        dependencies::is_blocked(&self.connection, task_id)
    }

    /// Read all tasks with an unfinished direct blocker.
    pub fn blocked_tasks(&self) -> Result<Vec<Task>> {
        dependencies::blocked(&self.connection)
    }

    /// Read all actionable leaf tasks using the default ready-work filters.
    pub fn ready_tasks(&self) -> Result<Vec<Task>> {
        dependencies::ready(&self.connection, &ReadyFilter::default())
    }

    /// Read actionable leaf tasks matching the supplied filters.
    pub fn ready_tasks_filtered(&self, filter: &ReadyFilter) -> Result<Vec<Task>> {
        dependencies::ready(&self.connection, filter)
    }

    /// Load the complete local graph with one read pass per entity collection.
    pub fn graph(&self) -> Result<Graph> {
        queries::load(&self.connection)
    }

    /// Build a task inspection view from the loaded graph.
    pub fn task_view(&self, id: TaskId) -> Result<TaskView> {
        queries::task_view(&self.connection, id)
    }

    /// Build a prefix-routed record inspection view.
    pub fn show(&self, id: &str) -> Result<ShowView> {
        queries::show(&self.connection, id)
    }

    /// Build the bounded context packet for a task.
    pub fn context(&self, id: TaskId) -> Result<ContextView> {
        queries::context(&self.connection, id)
    }

    /// Validate database and graph invariants without changing any rows.
    pub fn check(&self) -> Result<CheckReport> {
        queries::check(&self.connection)
    }

    #[cfg(test)]
    pub fn connection(&self) -> &Connection {
        &self.connection
    }
}

fn configure(connection: &mut Connection) -> Result<()> {
    connection.pragma_update(None, "foreign_keys", true)?;
    connection.busy_timeout(BUSY_TIMEOUT)?;
    Ok(())
}

/// Optional filters for derived ready-work queries.
#[derive(Clone, Debug, Default)]
pub struct ReadyFilter {
    /// Restrict results to one or more task priorities.
    pub priorities: Vec<TaskPriority>,
    /// Restrict results to tasks belonging to an epic's release.
    pub release_id: Option<ReleaseId>,
    /// Restrict results to one epic.
    pub epic_id: Option<EpicId>,
    /// Restrict results to one milestone.
    pub milestone_id: Option<MilestoneId>,
    /// Restrict results to direct children of one parent task.
    pub parent_id: Option<TaskId>,
}

#[cfg(test)]
mod tests {
    use crate::domain::{ContainerAction, ContainerStatus, DomainError, TaskPriority, TaskStatus};

    use super::{CURRENT_VERSION, Database, StorageError};

    #[test]
    fn opening_a_database_applies_embedded_migrations() {
        let database = Database::open_in_memory().expect("in-memory SQLite opens");
        assert_eq!(database.schema_version().expect("version is readable"), CURRENT_VERSION);

        let foreign_keys: i32 = database
            .connection()
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .expect("foreign keys pragma is readable");
        assert_eq!(foreign_keys, 1);

        let busy_timeout: i32 = database
            .connection()
            .pragma_query_value(None, "busy_timeout", |row| row.get(0))
            .expect("busy timeout pragma is readable");
        assert_eq!(busy_timeout, 5_000);

        let format_version: String = database
            .connection()
            .query_row(
                "SELECT value FROM meta WHERE key = 'database-format-version'",
                [],
                |row| row.get(0),
            )
            .expect("foundation migration creates the format marker");
        assert_eq!(format_version, CURRENT_VERSION.to_string());

        let ideas_table: String = database
            .connection()
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'ideas'",
                [],
                |row| row.get(0),
            )
            .expect("ideas migration creates the ideas table");
        assert_eq!(ideas_table, "ideas");

        let epics_table: String = database
            .connection()
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'epics'",
                [],
                |row| row.get(0),
            )
            .expect("epics migration creates the epics table");
        assert_eq!(epics_table, "epics");

        let dependencies_table: String = database
            .connection()
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'task_dependencies'",
                [],
                |row| row.get(0),
            )
            .expect("dependency migration creates the task_dependencies table");
        assert_eq!(dependencies_table, "task_dependencies");
    }

    #[test]
    fn container_guards_cover_the_full_descendant_graph_without_cascading() {
        let mut database = Database::open_in_memory().expect("database opens");
        let release = database
            .create_release("Release".to_owned(), String::new())
            .expect("release creates");
        let epic = database
            .create_epic("Epic".to_owned(), String::new(), "spec.md".to_owned(), Some(release.id))
            .expect("epic creates");
        let milestone = database
            .create_milestone(epic.id, "Milestone".to_owned(), String::new(), 0)
            .expect("milestone creates");
        let task = database
            .create_task(
                milestone.id,
                None,
                "Task".to_owned(),
                String::new(),
                TaskPriority::Normal,
                0,
            )
            .expect("task creates");

        let error = database
            .transition_release(release.id, ContainerAction::Complete, false)
            .expect_err("deep open descendants block release");
        assert!(matches!(
            error,
            StorageError::InvalidRelease(DomainError::OpenDescendants { .. })
        ));
        assert_eq!(
            database
                .release(release.id)
                .expect("release reads")
                .expect("release exists")
                .status,
            ContainerStatus::Open
        );

        let completed = database
            .transition_release(release.id, ContainerAction::Complete, true)
            .expect("override completes release");
        assert_eq!(completed.status, ContainerStatus::Completed);
        assert_eq!(
            database.epic(epic.id).expect("epic reads").expect("epic exists").status,
            ContainerStatus::Open
        );
        assert_eq!(
            database
                .milestone(milestone.id)
                .expect("milestone reads")
                .expect("milestone exists")
                .status,
            ContainerStatus::Open
        );
        assert_eq!(
            database.task(task.id).expect("task reads").expect("task exists").status,
            TaskStatus::Pending
        );
        assert!(
            database
                .transition_release(release.id, ContainerAction::Cancel, true)
                .is_err()
        );
        assert_eq!(
            database
                .transition_release(release.id, ContainerAction::Complete, false)
                .expect("completion repeats")
                .status,
            ContainerStatus::Completed
        );
    }

    #[test]
    fn epic_and_milestone_guards_allow_terminal_descendants_only() {
        let mut database = Database::open_in_memory().expect("database opens");
        let epic = database
            .create_epic("Epic".to_owned(), String::new(), "spec.md".to_owned(), None)
            .expect("epic creates");
        let milestone = database
            .create_milestone(epic.id, "Milestone".to_owned(), String::new(), 0)
            .expect("milestone creates");
        let task = database
            .create_task(
                milestone.id,
                None,
                "Task".to_owned(),
                String::new(),
                TaskPriority::Normal,
                0,
            )
            .expect("task creates");

        assert!(matches!(
            database.transition_milestone(milestone.id, ContainerAction::Complete, false),
            Err(StorageError::InvalidMilestone(DomainError::OpenDescendants { .. }))
        ));
        database
            .transition_task(task.id, crate::domain::TaskAction::Complete, false)
            .expect("task completes");
        database
            .transition_milestone(milestone.id, ContainerAction::Complete, false)
            .expect("milestone completes");
        database
            .transition_epic(epic.id, ContainerAction::Cancel, false)
            .expect("epic cancels with terminal descendants");
        assert_eq!(
            database.epic(epic.id).expect("epic reads").expect("epic exists").status,
            ContainerStatus::Cancelled
        );
    }
}
