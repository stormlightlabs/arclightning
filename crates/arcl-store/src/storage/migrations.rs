use rusqlite::Connection;

use super::{CURRENT_VERSION, StorageError};

struct Migration {
    version: i32,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration { version: 1, sql: include_str!("migrations/001_schema.sql") },
    Migration { version: 2, sql: include_str!("migrations/002_plan_keys.sql") },
    Migration { version: 3, sql: include_str!("migrations/003_remove_legacy_model.sql") },
    Migration { version: 4, sql: include_str!("migrations/004_workspace_files.sql") },
    Migration { version: 5, sql: include_str!("migrations/005_task_assignee.sql") },
    Migration { version: 6, sql: include_str!("migrations/006_task_notes.sql") },
    Migration { version: 7, sql: include_str!("migrations/007_task_gates.sql") },
    Migration { version: 8, sql: include_str!("migrations/008_capture_source.sql") },
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
    use crate::storage::{CURRENT_VERSION, DEFAULT_PROJECT_ID, Database};

    #[test]
    fn plan_key_migration_upgrades_a_version_one_database() {
        let mut connection = rusqlite::Connection::open_in_memory().expect("in-memory SQLite opens");
        connection
            .execute_batch(include_str!("migrations/001_schema.sql"))
            .expect("version one schema creates");
        connection
            .pragma_update(None, "user_version", 1)
            .expect("version one marker writes");

        super::apply(&mut connection).expect("plan key migration applies");
        let phase_key: String = connection
            .query_row(
                "SELECT name FROM pragma_table_info('phases') WHERE name = 'plan_key'",
                [],
                |row| row.get(0),
            )
            .expect("phase plan key exists");
        let task_key: String = connection
            .query_row(
                "SELECT name FROM pragma_table_info('planning_tasks') WHERE name = 'plan_key'",
                [],
                |row| row.get(0),
            )
            .expect("task plan key exists");
        assert_eq!(phase_key, "plan_key");
        assert_eq!(task_key, "plan_key");
    }

    #[test]
    fn legacy_records_are_migrated_before_the_old_tables_are_removed() {
        let mut connection = rusqlite::Connection::open_in_memory().expect("in-memory SQLite opens");
        connection
            .execute_batch(include_str!("migrations/001_schema.sql"))
            .expect("version one schema creates");
        connection
            .execute_batch(include_str!("migrations/002_plan_keys.sql"))
            .expect("version two schema creates");
        connection
            .pragma_update(None, "user_version", 2)
            .expect("version marker writes");

        connection
            .execute_batch(
                "INSERT INTO releases (id, title, description, status)
             VALUES ('arcl-r-01K0B2ZWTX7JX9PH7W5G1S6A9Q', 'Release', 'Release body', 'open');
             INSERT INTO notes (id, project_id, title, body)
             VALUES ('arcl-n-01K0B2ZWTX7JX9PH7W5G1S6A9Q', 'arcl-pj-00000000000000000000000000',
                     'Notes', '# Existing note body');
             INSERT INTO release_memberships (project_id, release_id, record_kind, record_id)
             VALUES ('arcl-pj-00000000000000000000000000', 'arcl-r-01K0B2ZWTX7JX9PH7W5G1S6A9Q',
                     'note', 'arcl-n-01K0B2ZWTX7JX9PH7W5G1S6A9Q');
             INSERT INTO record_links (project_id, source_kind, source_id, target_kind, target_id)
             VALUES ('arcl-pj-00000000000000000000000000', 'note',
                     'arcl-n-01K0B2ZWTX7JX9PH7W5G1S6A9Q', 'release',
                     'arcl-r-01K0B2ZWTX7JX9PH7W5G1S6A9Q');
             INSERT INTO ideas (id, title, description, status)
             VALUES ('arcl-i-01K0B3N4QSC9R7K6W8X2M5YH1Z', 'Capture', '# Capture body', 'promoted');
             INSERT INTO epics (id, release_id, title, description, spec_path, status)
             VALUES ('arcl-e-01K0B2ZWTX7JX9PH7W5G1S6A9Q', 'arcl-r-01K0B2ZWTX7JX9PH7W5G1S6A9Q',
                     'Spec', '# Spec body', 'specs/feature.md', 'open');
             INSERT INTO idea_promotions (idea_id, epic_id)
             VALUES ('arcl-i-01K0B3N4QSC9R7K6W8X2M5YH1Z', 'arcl-e-01K0B2ZWTX7JX9PH7W5G1S6A9Q');
             INSERT INTO milestones (id, epic_id, title, description, status, position)
             VALUES ('arcl-m-01K0B2ZWTX7JX9PH7W5G1S6A9Q', 'arcl-e-01K0B2ZWTX7JX9PH7W5G1S6A9Q',
                     'Phase', '# Phase body', 'open', 0);
             INSERT INTO tasks
                 (id, milestone_id, title, description, status, priority, position, handoff, evidence)
             VALUES ('arcl-t-01K0B31M6VGK4YH8VKT4C0D2DS', 'arcl-m-01K0B2ZWTX7JX9PH7W5G1S6A9Q',
                     'Blocker', '# Blocker body', 'completed', 'normal', 0, '', '# Done');
             INSERT INTO tasks
                 (id, milestone_id, parent_id, title, description, status, priority, position, handoff, evidence)
             VALUES ('arcl-t-01K0B31M6VGK4YH8VKT4C0D2DR', 'arcl-m-01K0B2ZWTX7JX9PH7W5G1S6A9Q',
                     'arcl-t-01K0B31M6VGK4YH8VKT4C0D2DS', 'Task', '# Task body', 'parked', 'high', 1,
                     '# Resume here', '# Partial evidence');
             INSERT INTO task_dependencies (task_id, blocker_id)
             VALUES ('arcl-t-01K0B31M6VGK4YH8VKT4C0D2DR', 'arcl-t-01K0B31M6VGK4YH8VKT4C0D2DS');",
            )
            .expect("legacy records insert");

        super::apply(&mut connection).expect("contract migration applies");
        for table in [
            "ideas",
            "epics",
            "milestones",
            "tasks",
            "idea_promotions",
            "task_dependencies",
        ] {
            let exists: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                    [table],
                    |row| row.get(0),
                )
                .expect("schema reads");
            assert!(!exists, "legacy table {table} still exists");
        }

        let graph = super::super::connected::graph(&connection).expect("connected graph reads");
        assert_eq!(graph.captures[0].body, "# Capture body");
        assert_eq!(graph.specs[0].body, "# Spec body");
        assert_eq!(graph.phases[0].body, "# Phase body");
        let task = graph
            .tasks
            .iter()
            .find(|task| task.title == "Task")
            .expect("task migrated");
        assert_eq!(task.body, "# Task body");
        assert_eq!(task.handoff, "# Resume here");
        assert_eq!(task.evidence, "# Partial evidence");
        assert!(task.parent_id.is_some());
        assert_eq!(graph.dependencies.len(), 1);
        assert_eq!(graph.capture_promotions.len(), 1);
        assert_eq!(graph.notes[0].body, "# Existing note body");
        assert_eq!(graph.release_memberships.len(), 2);
        assert_eq!(graph.links.len(), 1);
    }

    #[test]
    fn workspace_migration_keeps_the_stored_base() {
        let mut connection = rusqlite::Connection::open_in_memory().expect("in-memory SQLite opens");
        for sql in [
            include_str!("migrations/001_schema.sql"),
            include_str!("migrations/002_plan_keys.sql"),
            include_str!("migrations/003_remove_legacy_model.sql"),
        ] {
            connection.execute_batch(sql).expect("earlier schema creates");
        }
        connection
            .pragma_update(None, "user_version", 3)
            .expect("version marker writes");
        connection
            .execute(
                "INSERT INTO snapshot_files (path, content) VALUES ('notes/a.md', x'2b2b2b')",
                [],
            )
            .expect("base file inserts");

        super::apply(&mut connection).expect("workspace migration applies");
        let content: Vec<u8> = connection
            .query_row(
                "SELECT content FROM workspace_files WHERE path = 'notes/a.md'",
                [],
                |row| row.get(0),
            )
            .expect("base file moves");
        let state: String = connection
            .query_row("SELECT value FROM meta WHERE key = 'workspace-sync-state'", [], |row| {
                row.get(0)
            })
            .expect("sync state key moves");
        assert_eq!(content, b"+++");
        assert_eq!(state, "clean");
    }

    #[test]
    fn assignee_migration_keeps_existing_tasks_unclaimed() {
        let mut connection = rusqlite::Connection::open_in_memory().expect("in-memory SQLite opens");
        for sql in [
            include_str!("migrations/001_schema.sql"),
            include_str!("migrations/002_plan_keys.sql"),
            include_str!("migrations/003_remove_legacy_model.sql"),
            include_str!("migrations/004_workspace_files.sql"),
        ] {
            connection.execute_batch(sql).expect("earlier schema creates");
        }
        connection
            .pragma_update(None, "user_version", 4)
            .expect("version marker writes");
        connection
            .execute(
                "INSERT INTO planning_tasks (id, project_id, title, body, status, priority, position, handoff, evidence)
                 VALUES ('arcl-t-01K0B31M6VGK4YH8VKT4C0D2DS', ?1, 'Task', '', 'in_progress', 'normal', 0, '', '')",
                [DEFAULT_PROJECT_ID],
            )
            .expect("task inserts");

        super::apply(&mut connection).expect("assignee migration applies");
        let graph = super::super::connected::graph(&connection).expect("connected graph reads");
        assert_eq!(graph.tasks[0].title, "Task");
        assert_eq!(graph.tasks[0].assignee, None);
        let format: String = connection
            .query_row(
                "SELECT value FROM meta WHERE key = 'database-format-version'",
                [],
                |row| row.get(0),
            )
            .expect("format version reads");
        assert_eq!(format, CURRENT_VERSION.to_string());
    }

    #[test]
    fn task_notes_migration_leaves_existing_tasks_without_notes() {
        let mut connection = rusqlite::Connection::open_in_memory().expect("in-memory SQLite opens");
        for sql in [
            include_str!("migrations/001_schema.sql"),
            include_str!("migrations/002_plan_keys.sql"),
            include_str!("migrations/003_remove_legacy_model.sql"),
            include_str!("migrations/004_workspace_files.sql"),
            include_str!("migrations/005_task_assignee.sql"),
        ] {
            connection.execute_batch(sql).expect("earlier schema creates");
        }
        connection
            .pragma_update(None, "user_version", 5)
            .expect("version marker writes");
        connection
            .execute(
                "INSERT INTO planning_tasks (id, project_id, title, body, status, priority, position, handoff, evidence)
                 VALUES ('arcl-t-01K0B31M6VGK4YH8VKT4C0D2DS', ?1, 'Task', '', 'completed', 'normal', 0, '', '')",
                [DEFAULT_PROJECT_ID],
            )
            .expect("task inserts");

        super::apply(&mut connection).expect("task notes migration applies");
        let graph = super::super::connected::graph(&connection).expect("connected graph reads");
        assert_eq!(graph.tasks.len(), 1);
        assert!(graph.task_notes.is_empty());
        let format: String = connection
            .query_row(
                "SELECT value FROM meta WHERE key = 'database-format-version'",
                [],
                |row| row.get(0),
            )
            .expect("format version reads");
        assert_eq!(format, CURRENT_VERSION.to_string());
    }

    #[test]
    fn task_gates_migration_leaves_existing_tasks_without_gates() {
        let mut connection = rusqlite::Connection::open_in_memory().expect("in-memory SQLite opens");
        for sql in [
            include_str!("migrations/001_schema.sql"),
            include_str!("migrations/002_plan_keys.sql"),
            include_str!("migrations/003_remove_legacy_model.sql"),
            include_str!("migrations/004_workspace_files.sql"),
            include_str!("migrations/005_task_assignee.sql"),
            include_str!("migrations/006_task_notes.sql"),
        ] {
            connection.execute_batch(sql).expect("earlier schema creates");
        }
        connection
            .pragma_update(None, "user_version", 6)
            .expect("version marker writes");
        connection
            .execute(
                "INSERT INTO planning_tasks (id, project_id, title, body, status, priority, position, handoff, evidence)
                 VALUES ('arcl-t-01K0B31M6VGK4YH8VKT4C0D2DS', ?1, 'Task', '', 'pending', 'normal', 0, '', '')",
                [DEFAULT_PROJECT_ID],
            )
            .expect("task inserts");

        super::apply(&mut connection).expect("task gates migration applies");
        let graph = super::super::connected::graph(&connection).expect("connected graph reads");
        assert_eq!(graph.tasks.len(), 1);
        assert!(graph.gates.is_empty());
        let format: String = connection
            .query_row(
                "SELECT value FROM meta WHERE key = 'database-format-version'",
                [],
                |row| row.get(0),
            )
            .expect("format version reads");
        assert_eq!(format, CURRENT_VERSION.to_string());
    }

    #[test]
    fn capture_source_migration_leaves_existing_captures_without_a_source() {
        let mut connection = rusqlite::Connection::open_in_memory().expect("in-memory SQLite opens");
        for sql in [
            include_str!("migrations/001_schema.sql"),
            include_str!("migrations/002_plan_keys.sql"),
            include_str!("migrations/003_remove_legacy_model.sql"),
            include_str!("migrations/004_workspace_files.sql"),
            include_str!("migrations/005_task_assignee.sql"),
            include_str!("migrations/006_task_notes.sql"),
            include_str!("migrations/007_task_gates.sql"),
        ] {
            connection.execute_batch(sql).expect("earlier schema creates");
        }
        connection
            .pragma_update(None, "user_version", 7)
            .expect("version marker writes");
        connection
            .execute(
                "INSERT INTO captures (id, project_id, title, body, status, created_at)
                 VALUES ('arcl-c-01K0B3N4QSC9R7K6W8X2M5YH1Z', ?1, 'Idea', '', 'captured', '2026-01-01T00:00:00.000Z')",
                [DEFAULT_PROJECT_ID],
            )
            .expect("capture inserts");

        super::apply(&mut connection).expect("capture source migration applies");
        let graph = super::super::connected::graph(&connection).expect("connected graph reads");
        assert_eq!(graph.captures.len(), 1);
        assert_eq!(graph.captures[0].source_task_id, None);
        let format: String = connection
            .query_row(
                "SELECT value FROM meta WHERE key = 'database-format-version'",
                [],
                |row| row.get(0),
            )
            .expect("format version reads");
        assert_eq!(format, CURRENT_VERSION.to_string());
    }

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
