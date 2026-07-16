mod epics;
mod ideas;
mod migrations;
mod milestones;
mod releases;
mod tasks;

pub use tasks::TaskUpdate;

use std::{path::Path, time::Duration};

use rusqlite::Connection;
use thiserror::Error;

use crate::domain::{
    DomainError, Epic, EpicId, Idea, IdeaId, Milestone, MilestoneId, Release, ReleaseId, Task, TaskId, TaskPriority,
};

pub const CURRENT_VERSION: i32 = 5;

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

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
    #[error("spec path `{path}` is already linked to another epic")]
    DuplicateSpec { path: String },
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
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let mut connection = Connection::open(path)?;
        configure(&mut connection)?;
        migrations::apply(&mut connection)?;
        Ok(Self { connection })
    }

    pub fn open_in_memory() -> Result<Self, StorageError> {
        let mut connection = Connection::open_in_memory()?;
        configure(&mut connection)?;
        migrations::apply(&mut connection)?;
        Ok(Self { connection })
    }

    pub fn schema_version(&self) -> Result<i32, StorageError> {
        Ok(self
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    pub fn idea(&self, id: IdeaId) -> Result<Option<Idea>, StorageError> {
        ideas::find(&self.connection, &id)
    }

    pub fn ideas(&self) -> Result<Vec<Idea>, StorageError> {
        ideas::list(&self.connection)
    }

    pub fn create_idea(&mut self, title: String, description: String) -> Result<Idea, StorageError> {
        ideas::create(&mut self.connection, title, description)
    }

    pub fn update_idea(
        &mut self, id: IdeaId, title: Option<String>, description: Option<String>,
    ) -> Result<Idea, StorageError> {
        ideas::update(&mut self.connection, id, title, description)
    }

    pub fn discard_idea(&mut self, id: IdeaId) -> Result<Idea, StorageError> {
        ideas::discard(&mut self.connection, id)
    }

    /// Read one release by its validated identifier.
    pub fn release(&self, id: ReleaseId) -> Result<Option<Release>, StorageError> {
        releases::find(&self.connection, &id)
    }

    /// Read all releases in deterministic identifier order.
    pub fn releases(&self) -> Result<Vec<Release>, StorageError> {
        releases::list(&self.connection)
    }

    /// Create an open release in a transaction.
    pub fn create_release(&mut self, title: String, description: String) -> Result<Release, StorageError> {
        releases::create(&mut self.connection, title, description)
    }

    /// Update a release's title and/or Markdown description in a transaction.
    pub fn update_release(
        &mut self, id: ReleaseId, title: Option<String>, description: Option<String>,
    ) -> Result<Release, StorageError> {
        releases::update(&mut self.connection, id, title, description)
    }

    /// Read one epic by its validated identifier.
    pub fn epic(&self, id: EpicId) -> Result<Option<Epic>, StorageError> {
        epics::find(&self.connection, &id)
    }

    /// Read all epics in deterministic identifier order.
    pub fn epics(&self) -> Result<Vec<Epic>, StorageError> {
        epics::list(&self.connection)
    }

    /// Create an open epic after validating its release association and unique spec path.
    pub fn create_epic(
        &mut self, title: String, description: String, spec_path: String, release_id: Option<ReleaseId>,
    ) -> Result<Epic, StorageError> {
        epics::create(&mut self.connection, title, description, spec_path, release_id)
    }

    /// Update an epic without modifying the linked Markdown file.
    pub fn update_epic(
        &mut self, id: EpicId, title: Option<String>, description: Option<String>, spec_path: Option<String>,
        release_change: Option<Option<ReleaseId>>,
    ) -> Result<Epic, StorageError> {
        epics::update(&mut self.connection, id, title, description, spec_path, release_change)
    }

    /// Read one milestone by its validated identifier.
    pub fn milestone(&self, id: MilestoneId) -> Result<Option<Milestone>, StorageError> {
        milestones::find(&self.connection, &id)
    }

    /// Read milestones in position order, breaking ties by ULID.
    pub fn milestones(&self) -> Result<Vec<Milestone>, StorageError> {
        milestones::list(&self.connection)
    }

    /// Create an open milestone owned by an existing epic.
    pub fn create_milestone(
        &mut self, epic_id: EpicId, title: String, description: String, position: i64,
    ) -> Result<Milestone, StorageError> {
        milestones::create(&mut self.connection, epic_id, title, description, position)
    }

    /// Update a milestone's title, Markdown description, and position atomically.
    pub fn update_milestone(
        &mut self, id: MilestoneId, title: Option<String>, description: Option<String>, position: Option<i64>,
    ) -> Result<Milestone, StorageError> {
        milestones::update(&mut self.connection, id, title, description, position)
    }

    /// Read one task or subtask by its validated identifier.
    pub fn task(&self, id: TaskId) -> Result<Option<Task>, StorageError> {
        tasks::find(&self.connection, &id)
    }

    /// Read tasks in milestone, position, and ULID order.
    pub fn tasks(&self) -> Result<Vec<Task>, StorageError> {
        tasks::list(&self.connection)
    }

    /// Create a pending task, validating its milestone and optional parent.
    pub fn create_task(
        &mut self, milestone_id: MilestoneId, parent_id: Option<TaskId>, title: String, description: String,
        priority: TaskPriority, position: i64,
    ) -> Result<Task, StorageError> {
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

    /// Update a task and atomically move its complete descendant subtree when needed.
    pub fn update_task(&mut self, id: TaskId, update: TaskUpdate) -> Result<Task, StorageError> {
        tasks::update(&mut self.connection, id, update)
    }

    #[cfg(test)]
    pub fn connection(&self) -> &Connection {
        &self.connection
    }
}

fn configure(connection: &mut Connection) -> Result<(), StorageError> {
    connection.pragma_update(None, "foreign_keys", true)?;
    connection.busy_timeout(BUSY_TIMEOUT)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{CURRENT_VERSION, Database};

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
    }
}
