mod ideas;
mod migrations;

use std::{path::Path, time::Duration};

use rusqlite::Connection;
use thiserror::Error;

use crate::domain::{DomainError, Idea, IdeaId};

pub const CURRENT_VERSION: i32 = 2;

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Infrastructure failures from the SQLite storage boundary.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("stored idea is invalid: {0}")]
    InvalidIdea(#[from] DomainError),
    #[error("idea `{id}` was not found")]
    IdeaNotFound { id: String },
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

    #[cfg(test)]
    pub(crate) fn connection(&self) -> &Connection {
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
    }
}
