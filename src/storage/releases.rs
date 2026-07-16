use rusqlite::{Connection, OptionalExtension, params};

use crate::domain::{ContainerAction, ContainerStatus, DomainError, Release, ReleaseId};

use super::StorageError;

pub fn find(connection: &Connection, id: &ReleaseId) -> Result<Option<Release>, StorageError> {
    read_one(connection, id)
}

pub fn list(connection: &Connection) -> Result<Vec<Release>, StorageError> {
    let mut statement = connection.prepare("SELECT id, title, description, status FROM releases ORDER BY id")?;
    let rows = statement.query_map([], row_to_raw_release)?;
    let raw_releases = rows.collect::<Result<Vec<_>, _>>()?;
    raw_releases.into_iter().map(decode_release).collect()
}

pub fn create(connection: &mut Connection, title: String, description: String) -> Result<Release, StorageError> {
    let release = Release::new(title, description).map_err(StorageError::InvalidRelease)?;
    let transaction = connection.transaction()?;
    transaction.execute(
        "INSERT INTO releases (id, title, description, status) VALUES (?1, ?2, ?3, ?4)",
        params![
            release.id.to_string(),
            release.title,
            release.description,
            release.status.as_str()
        ],
    )?;
    transaction.commit()?;
    Ok(release)
}

pub fn update(
    connection: &mut Connection, id: ReleaseId, title: Option<String>, description: Option<String>,
) -> Result<Release, StorageError> {
    if title.is_none() && description.is_none() {
        return Err(StorageError::InvalidRelease(DomainError::NoFieldsToUpdate {
            entity: "release",
        }));
    }
    if let Some(title) = &title {
        crate::domain::validate_title(title).map_err(StorageError::InvalidRelease)?;
    }

    let transaction = connection.transaction()?;
    let current = read_one(&transaction, &id)?.ok_or_else(|| StorageError::ReleaseNotFound { id: id.to_string() })?;
    let next_title = title.as_deref().unwrap_or(&current.title);
    let next_description = description.as_deref().unwrap_or(&current.description);

    transaction.execute(
        "UPDATE releases SET title = ?1, description = ?2 WHERE id = ?3",
        params![next_title, next_description, id.to_string()],
    )?;
    transaction.commit()?;

    Ok(Release { id, title: next_title.to_owned(), description: next_description.to_owned(), status: current.status })
}

pub fn transition(
    connection: &mut Connection, id: ReleaseId, action: ContainerAction, allow_open_children: bool,
) -> Result<Release, StorageError> {
    let transaction = connection.transaction()?;
    let current = read_one(&transaction, &id)?.ok_or_else(|| StorageError::ReleaseNotFound { id: id.to_string() })?;
    let next_status = current
        .status
        .apply("release", action)
        .map_err(StorageError::InvalidRelease)?;
    if next_status == current.status {
        return Ok(current);
    }
    if !allow_open_children && has_open_descendants(&transaction, &id)? {
        return Err(StorageError::InvalidRelease(DomainError::OpenDescendants {
            entity: "release",
            id: id.to_string(),
            action: action.as_str(),
        }));
    }
    transaction.execute(
        "UPDATE releases SET status = ?1 WHERE id = ?2",
        params![next_status.as_str(), id.to_string()],
    )?;
    transaction.commit()?;
    Ok(Release { status: next_status, ..current })
}

fn has_open_descendants(connection: &Connection, id: &ReleaseId) -> Result<bool, StorageError> {
    Ok(connection.query_row(
        "SELECT EXISTS (
             SELECT 1 FROM epics e WHERE e.release_id = ?1 AND e.status = 'open'
             UNION ALL
             SELECT 1 FROM milestones m JOIN epics e ON e.id = m.epic_id
             WHERE e.release_id = ?1 AND m.status = 'open'
             UNION ALL
             SELECT 1 FROM tasks t
             JOIN milestones m ON m.id = t.milestone_id
             JOIN epics e ON e.id = m.epic_id
             WHERE e.release_id = ?1 AND t.status NOT IN ('completed', 'cancelled')
         )",
        params![id.to_string()],
        |row| row.get(0),
    )?)
}

fn read_one(connection: &Connection, id: &ReleaseId) -> Result<Option<Release>, StorageError> {
    let raw_release = connection
        .query_row(
            "SELECT id, title, description, status FROM releases WHERE id = ?1",
            params![id.to_string()],
            row_to_raw_release,
        )
        .optional()?;
    raw_release.map(decode_release).transpose()
}

fn row_to_raw_release(row: &rusqlite::Row<'_>) -> rusqlite::Result<(String, String, String, String)> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
}

fn decode_release((id, title, description, status): (String, String, String, String)) -> Result<Release, StorageError> {
    let id = ReleaseId::parse(&id)
        .map_err(DomainError::from)
        .map_err(StorageError::InvalidRelease)?;
    let status = ContainerStatus::parse("release", &status).map_err(StorageError::InvalidRelease)?;
    Release::from_parts(id, title, description, status).map_err(StorageError::InvalidRelease)
}
