use rusqlite::{Connection, OptionalExtension, params};

use crate::domain::{ContainerStatus, DomainError, Release, ReleaseId};

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
