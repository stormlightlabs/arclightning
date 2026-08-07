use rusqlite::Connection;

use super::{CURRENT_VERSION, StorageError};

struct Migration {
    version: i32,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration { version: 1, sql: include_str!("migrations/001_foundation.sql") },
    Migration { version: 2, sql: include_str!("migrations/002_ideas.sql") },
    Migration { version: 3, sql: include_str!("migrations/003_releases_epics.sql") },
    Migration { version: 4, sql: include_str!("migrations/004_milestones.sql") },
    Migration { version: 5, sql: include_str!("migrations/005_tasks.sql") },
    Migration { version: 6, sql: include_str!("migrations/006_task_dependencies.sql") },
    Migration { version: 7, sql: include_str!("migrations/007_idea_promotions.sql") },
    Migration { version: 8, sql: include_str!("migrations/008_snapshot_files.sql") },
];

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
