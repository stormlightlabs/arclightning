use rusqlite::{Connection, OptionalExtension, params};

use crate::domain::{ContainerAction, ContainerStatus, DomainError, EpicId, Milestone, MilestoneId};

use super::{Result, StorageError};

type RawMilestoneRow = (String, String, Option<String>, String, String, String, i64);

pub fn find(connection: &Connection, id: &MilestoneId) -> Result<Option<Milestone>> {
    read_one(connection, id)
}

pub fn list(connection: &Connection) -> Result<Vec<Milestone>> {
    let mut statement = connection.prepare(
        "SELECT id, epic_id, plan_key, title, description, status, position
         FROM milestones ORDER BY position, id",
    )?;
    let rows = statement.query_map([], row_to_raw_milestone)?;
    let raw_milestones = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    raw_milestones.into_iter().map(decode_milestone).collect()
}

pub fn create(
    connection: &mut Connection, epic_id: EpicId, title: String, description: String, position: i64,
) -> Result<Milestone> {
    let milestone = Milestone::new(epic_id, title, description, position).map_err(StorageError::InvalidMilestone)?;
    let transaction = connection.transaction()?;
    ensure_epic_exists(&transaction, milestone.epic_id)?;
    transaction.execute(
        "INSERT INTO milestones (id, epic_id, title, description, status, position)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            milestone.id.to_string(),
            milestone.epic_id.to_string(),
            milestone.title,
            milestone.description,
            milestone.status.as_str(),
            milestone.position,
        ],
    )?;
    transaction.commit()?;
    Ok(milestone)
}

pub fn update(
    connection: &mut Connection, id: MilestoneId, title: Option<String>, description: Option<String>,
    position: Option<i64>,
) -> Result<Milestone> {
    if title.is_none() && description.is_none() && position.is_none() {
        return Err(StorageError::InvalidMilestone(DomainError::NoFieldsToUpdate {
            entity: "milestone",
        }));
    }
    if let Some(title) = &title {
        crate::domain::validate_title(title).map_err(StorageError::InvalidMilestone)?;
    }
    if let Some(position) = position {
        crate::domain::validate_position(position).map_err(StorageError::InvalidMilestone)?;
    }

    let transaction = connection.transaction()?;
    let current = read_one(&transaction, &id)?.ok_or_else(|| StorageError::MilestoneNotFound { id: id.to_string() })?;
    let next_title = title.as_deref().unwrap_or(&current.title);
    let next_description = description.as_deref().unwrap_or(&current.description);
    let next_position = position.unwrap_or(current.position);

    transaction.execute(
        "UPDATE milestones SET title = ?1, description = ?2, position = ?3 WHERE id = ?4",
        params![next_title, next_description, next_position, id.to_string()],
    )?;
    transaction.commit()?;

    Ok(Milestone {
        id,
        epic_id: current.epic_id,
        title: next_title.to_owned(),
        description: next_description.to_owned(),
        status: current.status,
        position: next_position,
        plan_key: current.plan_key,
    })
}

pub fn transition(
    connection: &mut Connection, id: MilestoneId, action: ContainerAction, allow_open_children: bool,
) -> Result<Milestone> {
    let transaction = connection.transaction()?;
    let current = read_one(&transaction, &id)?.ok_or_else(|| StorageError::MilestoneNotFound { id: id.to_string() })?;
    let next_status = current
        .status
        .apply("milestone", action)
        .map_err(StorageError::InvalidMilestone)?;
    if next_status == current.status {
        return Ok(current);
    }
    let has_open_children: bool = transaction.query_row(
        "SELECT EXISTS (SELECT 1 FROM tasks WHERE milestone_id = ?1 AND status NOT IN ('completed', 'cancelled'))",
        params![id.to_string()],
        |row| row.get(0),
    )?;
    if !allow_open_children && has_open_children {
        return Err(StorageError::InvalidMilestone(DomainError::OpenDescendants {
            entity: "milestone",
            id: id.to_string(),
            action: action.as_str(),
        }));
    }
    transaction.execute(
        "UPDATE milestones SET status = ?1 WHERE id = ?2",
        params![next_status.as_str(), id.to_string()],
    )?;
    transaction.commit()?;
    Ok(Milestone { status: next_status, ..current })
}

fn read_one(connection: &Connection, id: &MilestoneId) -> Result<Option<Milestone>> {
    let raw_milestone = connection
        .query_row(
            "SELECT id, epic_id, plan_key, title, description, status, position FROM milestones WHERE id = ?1",
            params![id.to_string()],
            row_to_raw_milestone,
        )
        .optional()?;
    raw_milestone.map(decode_milestone).transpose()
}

fn row_to_raw_milestone(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawMilestoneRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
    ))
}

fn decode_milestone(
    (id, epic_id, plan_key, title, description, status, position): RawMilestoneRow,
) -> Result<Milestone> {
    let id = MilestoneId::parse(&id)
        .map_err(DomainError::from)
        .map_err(StorageError::InvalidMilestone)?;
    let epic_id = EpicId::parse(&epic_id)
        .map_err(DomainError::from)
        .map_err(StorageError::InvalidMilestone)?;
    let status = ContainerStatus::parse("milestone", &status).map_err(StorageError::InvalidMilestone)?;
    Milestone::from_parts(id, epic_id, title, description, status, position, plan_key)
        .map_err(StorageError::InvalidMilestone)
}

fn ensure_epic_exists(connection: &Connection, epic_id: EpicId) -> Result<()> {
    let exists: Option<String> = connection
        .query_row(
            "SELECT id FROM epics WHERE id = ?1",
            params![epic_id.to_string()],
            |row| row.get(0),
        )
        .optional()?;
    if exists.is_none() {
        return Err(StorageError::EpicNotFound { id: epic_id.to_string() });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::domain::{DomainError, MilestoneId};

    use super::super::{Database, StorageError};

    #[test]
    fn milestone_mutations_validate_ownership_and_order_ties_by_id() {
        let mut database = Database::open_in_memory().expect("database opens");
        let epic = database
            .create_epic("Epic".to_owned(), String::new(), "spec.md".to_owned(), None)
            .expect("epic creates");
        let first = database
            .create_milestone(epic.id, "First".to_owned(), String::new(), 10)
            .expect("first milestone creates");
        let second = database
            .create_milestone(epic.id, "Second".to_owned(), String::new(), 10)
            .expect("second milestone creates");
        let milestones = database.milestones().expect("milestones list");
        assert_eq!(milestones[0].id, first.id.min(second.id));
        assert_eq!(
            database
                .milestone(first.id)
                .expect("milestone reads")
                .expect("milestone exists")
                .epic_id,
            epic.id
        );

        let error = database
            .create_milestone(crate::domain::EpicId::new(), "Missing".to_owned(), String::new(), 0)
            .expect_err("missing epic is rejected");
        assert!(matches!(error, StorageError::EpicNotFound { .. }));

        let error = database
            .create_milestone(epic.id, "Negative".to_owned(), String::new(), -1)
            .expect_err("negative position is rejected");
        assert!(matches!(
            error,
            StorageError::InvalidMilestone(DomainError::InvalidPosition { .. })
        ));
    }

    #[test]
    fn milestone_ids_are_validated_by_storage_reads() {
        assert!(MilestoneId::parse("arcl-m-not-a-ulid").is_err());
    }
}
