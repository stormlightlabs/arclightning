use rusqlite::Connection;

use super::{CURRENT_VERSION, StorageError};

struct Migration {
    version: i32,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[Migration { version: 1, sql: include_str!("migrations/001_schema.sql") }];

pub fn apply(connection: &mut Connection) -> Result<(), StorageError> {
    let current: i32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if current > CURRENT_VERSION {
        return Err(StorageError::NewerDatabase { found: current, latest: CURRENT_VERSION });
    }

    let mut expected = current + 1;
    for migration in MIGRATIONS {
        if migration.version < expected {
            continue;
        }
        if migration.version != expected {
            return Err(StorageError::MigrationGap { expected, found: migration.version });
        }

        let transaction = connection.transaction()?;
        transaction.execute_batch(migration.sql)?;
        transaction.pragma_update(None, "user_version", migration.version)?;
        transaction.commit()?;
        expected += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{CURRENT_VERSION, DEFAULT_PROJECT_ID, Database};

    #[test]
    fn baseline_creates_the_complete_schema() {
        let database = Database::open_in_memory().expect("database opens");

        assert_eq!(database.schema_version().expect("version reads"), CURRENT_VERSION);
        assert_eq!(
            database.project().expect("project reads").id.to_string(),
            DEFAULT_PROJECT_ID
        );
        assert!(database.captures().expect("captures read").is_empty());
        assert!(database.specs().expect("specs read").is_empty());
        assert!(database.plans().expect("plans read").is_empty());
        assert!(database.phases().expect("phases read").is_empty());
        assert!(database.planning_tasks().expect("planning tasks read").is_empty());
        assert!(database.notes().expect("notes read").is_empty());
    }
}
