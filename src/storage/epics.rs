use rusqlite::{Connection, OptionalExtension, params};

use crate::domain::{ContainerStatus, DomainError, Epic, EpicId, ReleaseId};

use super::StorageError;

type Result<T> = std::result::Result<T, StorageError>;

pub fn find(connection: &Connection, id: &EpicId) -> Result<Option<Epic>> {
    read_one(connection, id)
}

pub fn list(connection: &Connection) -> Result<Vec<Epic>> {
    let mut statement =
        connection.prepare("SELECT id, release_id, title, description, spec_path, status FROM epics ORDER BY id")?;
    let rows = statement.query_map([], row_to_raw_epic)?;
    let raw_epics = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    raw_epics.into_iter().map(decode_epic).collect()
}

pub fn create(
    connection: &mut Connection, title: String, description: String, spec_path: String, release_id: Option<ReleaseId>,
) -> Result<Epic> {
    let epic = Epic::new(title, description, spec_path, release_id).map_err(StorageError::InvalidEpic)?;
    let transaction = connection.transaction()?;
    ensure_release_exists(&transaction, epic.release_id)?;
    ensure_spec_available(&transaction, &epic.spec_path, None)?;
    transaction.execute(
        "INSERT INTO epics (id, release_id, title, description, spec_path, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            epic.id.to_string(),
            epic.release_id.map(|id| id.to_string()),
            epic.title,
            epic.description,
            epic.spec_path,
            epic.status.as_str()
        ],
    )?;
    transaction.commit()?;
    Ok(epic)
}

pub fn update(
    connection: &mut Connection, id: EpicId, title: Option<String>, description: Option<String>,
    spec_path: Option<String>, release_change: Option<Option<ReleaseId>>,
) -> Result<Epic> {
    if title.is_none() && description.is_none() && spec_path.is_none() && release_change.is_none() {
        return Err(StorageError::InvalidEpic(DomainError::NoFieldsToUpdate {
            entity: "epic",
        }));
    }
    if let Some(title) = &title {
        crate::domain::validate_title(title).map_err(StorageError::InvalidEpic)?;
    }
    if let Some(spec_path) = &spec_path
        && spec_path.is_empty()
    {
        return Err(StorageError::InvalidEpic(DomainError::EmptySpecPath));
    }

    let transaction = connection.transaction()?;
    let current = read_one(&transaction, &id)?.ok_or_else(|| StorageError::EpicNotFound { id: id.to_string() })?;
    let next_release = release_change.unwrap_or(current.release_id);
    let next_title = title.as_deref().unwrap_or(&current.title);
    let next_description = description.as_deref().unwrap_or(&current.description);
    let next_spec_path = spec_path.as_deref().unwrap_or(&current.spec_path);

    ensure_release_exists(&transaction, next_release)?;
    if next_spec_path != current.spec_path {
        ensure_spec_available(&transaction, next_spec_path, Some(id))?;
    }

    transaction.execute(
        "UPDATE epics SET release_id = ?1, title = ?2, description = ?3, spec_path = ?4 WHERE id = ?5",
        params![
            next_release.map(|release_id| release_id.to_string()),
            next_title,
            next_description,
            next_spec_path,
            id.to_string()
        ],
    )?;
    transaction.commit()?;

    Ok(Epic {
        id,
        release_id: next_release,
        title: next_title.to_owned(),
        description: next_description.to_owned(),
        spec_path: next_spec_path.to_owned(),
        status: current.status,
    })
}

fn read_one(connection: &Connection, id: &EpicId) -> Result<Option<Epic>> {
    let raw_epic = connection
        .query_row(
            "SELECT id, release_id, title, description, spec_path, status FROM epics WHERE id = ?1",
            params![id.to_string()],
            row_to_raw_epic,
        )
        .optional()?;
    raw_epic.map(decode_epic).transpose()
}

// FIXME: this transitional tuple is awful
fn row_to_raw_epic(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<(String, Option<String>, String, String, String, String)> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}

fn decode_epic(
    (id, release_id, title, description, spec_path, status): (String, Option<String>, String, String, String, String),
) -> Result<Epic> {
    let id = EpicId::parse(&id)
        .map_err(DomainError::from)
        .map_err(StorageError::InvalidEpic)?;
    let release_id = release_id
        .map(|id| {
            ReleaseId::parse(&id)
                .map_err(DomainError::from)
                .map_err(StorageError::InvalidEpic)
        })
        .transpose()?;
    let status = ContainerStatus::parse("epic", &status).map_err(StorageError::InvalidEpic)?;
    Epic::from_parts(id, release_id, title, description, spec_path, status).map_err(StorageError::InvalidEpic)
}

fn ensure_release_exists(connection: &Connection, release_id: Option<ReleaseId>) -> Result<()> {
    let Some(release_id) = release_id else { return Ok(()) };
    let exists: Option<String> = connection
        .query_row(
            "SELECT id FROM releases WHERE id = ?1",
            params![release_id.to_string()],
            |row| row.get(0),
        )
        .optional()?;
    if exists.is_none() {
        return Err(StorageError::ReleaseNotFound { id: release_id.to_string() });
    }
    Ok(())
}

fn ensure_spec_available(connection: &Connection, spec_path: &str, except: Option<EpicId>) -> Result<()> {
    let existing: Option<(String,)> = connection
        .query_row("SELECT id FROM epics WHERE spec_path = ?1", params![spec_path], |row| {
            Ok((row.get(0)?,))
        })
        .optional()?;
    if let Some((existing_id,)) = existing
        && except.is_none_or(|id| existing_id != id.to_string())
    {
        return Err(StorageError::DuplicateSpec { path: spec_path.to_owned() });
    }
    Ok(())
}
