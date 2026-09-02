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
    Migration { version: 9, sql: include_str!("migrations/009_connected_planning.sql") },
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

#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::{DEFAULT_PROJECT_ID, Database};
    use rusqlite::Connection;
    use tempfile::tempdir;

    fn v1_database(path: &std::path::Path) {
        let connection = Connection::open(path).expect("database opens");
        for (version, sql) in [
            (1, include_str!("migrations/001_foundation.sql")),
            (2, include_str!("migrations/002_ideas.sql")),
            (3, include_str!("migrations/003_releases_epics.sql")),
            (4, include_str!("migrations/004_milestones.sql")),
            (5, include_str!("migrations/005_tasks.sql")),
            (6, include_str!("migrations/006_task_dependencies.sql")),
            (7, include_str!("migrations/007_idea_promotions.sql")),
            (8, include_str!("migrations/008_snapshot_files.sql")),
        ] {
            connection.execute_batch(sql).expect("old migration applies");
            connection
                .pragma_update(None, "user_version", version)
                .expect("old version applies");
        }
        connection
            .execute(
                "INSERT INTO releases (id, title, description, status) VALUES (?1, 'Release', 'Release body', 'open')",
                ["arcl-r-01K0B2ZWTX7JX9PH7W5G1S6A9Q"],
            )
            .expect("release inserts");
        connection
            .execute(
                "INSERT INTO ideas (id, title, description, status) VALUES (?1, 'Capture', 'Capture body', 'promoted')",
                ["arcl-i-01K0B3N4QSC9R7K6W8X2M5YH1Z"],
            )
            .expect("idea inserts");
        connection.execute(
            "INSERT INTO epics (id, release_id, title, description, spec_path, status) VALUES (?1, ?2, 'Spec', 'Tracker body', 'specs/feature.md', 'open')",
            rusqlite::params!["arcl-e-01K0B2ZWTX7JX9PH7W5G1S6A9Q", "arcl-r-01K0B2ZWTX7JX9PH7W5G1S6A9Q"],
        ).expect("epic inserts");
        connection.execute(
            "INSERT INTO milestones (id, epic_id, title, description, status, position) VALUES (?1, ?2, 'Phase', 'Phase body', 'open', 0)",
            rusqlite::params!["arcl-m-01K0B2ZWTX7JX9PH7W5G1S6A9Q", "arcl-e-01K0B2ZWTX7JX9PH7W5G1S6A9Q"],
        ).expect("milestone inserts");
        connection.execute(
            "INSERT INTO tasks (id, milestone_id, title, description, status, priority, position, handoff, evidence) VALUES (?1, ?2, 'Task', 'Task body', 'pending', 'high', 0, 'Resume', 'Evidence')",
            rusqlite::params!["arcl-t-01K0B31M6VGK4YH8VKT4C0D2DR", "arcl-m-01K0B2ZWTX7JX9PH7W5G1S6A9Q"],
        ).expect("task inserts");
        connection
            .execute(
                "INSERT INTO tasks (id, milestone_id, title, description, status, priority, position, handoff, evidence) VALUES (?1, ?2, 'Blocker', 'Blocker body', 'completed', 'normal', 1, '', 'Done')",
                rusqlite::params!["arcl-t-01K0B31M6VGK4YH8VKT4C0D2DS", "arcl-m-01K0B2ZWTX7JX9PH7W5G1S6A9Q"],
            )
            .expect("blocker inserts");
        connection
            .execute(
                "INSERT INTO task_dependencies (task_id, blocker_id) VALUES (?1, ?2)",
                rusqlite::params!["arcl-t-01K0B31M6VGK4YH8VKT4C0D2DR", "arcl-t-01K0B31M6VGK4YH8VKT4C0D2DS"],
            )
            .expect("dependency inserts");
        connection
            .execute(
                "INSERT INTO idea_promotions (idea_id, epic_id) VALUES (?1, ?2)",
                rusqlite::params!["arcl-i-01K0B3N4QSC9R7K6W8X2M5YH1Z", "arcl-e-01K0B2ZWTX7JX9PH7W5G1S6A9Q"],
            )
            .expect("promotion inserts");
    }

    #[test]
    fn current_projects_upgrade_into_the_connected_model_with_mappings() {
        let directory = tempdir().expect("temporary directory creates");
        let arcl = directory.path().join(".arcl");
        fs::create_dir_all(&arcl).expect("Arc directory creates");
        fs::create_dir_all(directory.path().join("specs")).expect("spec directory creates");
        fs::write(
            directory.path().join("specs/feature.md"),
            "# Owned spec\n\nExternal body\n",
        )
        .expect("spec writes");
        let database_path = arcl.join("arcl.db");
        v1_database(&database_path);

        let database = Database::open(&database_path).expect("database upgrades");
        assert_eq!(database.schema_version().expect("version reads"), 9);
        assert_eq!(
            database.project().expect("project reads").id.to_string(),
            DEFAULT_PROJECT_ID
        );
        assert_eq!(database.captures().expect("captures read")[0].body, "Capture body");
        assert_eq!(database.capture_promotions().expect("promotions read").len(), 1);
        assert_eq!(
            database.specs().expect("specs read")[0].body,
            "# Owned spec\n\nExternal body\n"
        );
        assert_eq!(database.plans().expect("plans read").len(), 1);
        assert_eq!(database.phases().expect("phases read").len(), 1);
        let task = &database.planning_tasks().expect("planning tasks read")[0];
        assert_eq!(task.body, "Task body");
        assert_eq!(task.handoff, "Resume");
        assert_eq!(task.evidence, "Evidence");
        assert_eq!(database.planning_dependencies().expect("dependencies read").len(), 1);
        assert!(
            database
                .release_memberships()
                .expect("memberships read")
                .iter()
                .any(|edge| edge.record_kind.as_str() == "spec")
        );
        assert!(
            database
                .legacy_id_mappings()
                .expect("mappings read")
                .iter()
                .any(|mapping| mapping.legacy_kind == "epic" && mapping.current_kind == "spec")
        );
    }
}
