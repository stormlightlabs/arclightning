use rusqlite::{Connection, OptionalExtension, params};

use crate::domain::{DomainError, Idea, IdeaAction, IdeaId, IdeaStatus, validate_title};

use super::StorageError;

pub fn find(connection: &Connection, id: &IdeaId) -> Result<Option<Idea>, StorageError> {
    read_one(connection, id)
}

pub fn list(connection: &Connection) -> Result<Vec<Idea>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT i.id, i.title, i.description, i.status, p.epic_id
         FROM ideas i LEFT JOIN idea_promotions p ON p.idea_id = i.id ORDER BY i.id",
    )?;
    let rows = statement.query_map([], row_to_raw_idea)?;
    let raw_ideas = rows.collect::<Result<Vec<_>, _>>()?;
    raw_ideas.into_iter().map(decode_idea).collect()
}

pub fn create(connection: &mut Connection, title: String, description: String) -> Result<Idea, StorageError> {
    let idea = Idea::new(title, description)?;
    let transaction = connection.transaction()?;
    transaction.execute(
        "INSERT INTO ideas (id, title, description, status) VALUES (?1, ?2, ?3, ?4)",
        params![idea.id.to_string(), idea.title, idea.description, idea.status.as_str()],
    )?;
    transaction.commit()?;
    Ok(idea)
}

pub fn update(
    connection: &mut Connection, id: IdeaId, title: Option<String>, description: Option<String>,
) -> Result<Idea, StorageError> {
    if title.is_none() && description.is_none() {
        return Err(StorageError::InvalidIdea(DomainError::NoFieldsToUpdate {
            entity: "idea",
        }));
    }
    if let Some(title) = &title {
        validate_title(title)?;
    }

    let transaction = connection.transaction()?;
    let current = read_one(&transaction, &id)?.ok_or_else(|| StorageError::IdeaNotFound { id: id.to_string() })?;
    current.status.validate_update()?;
    let next_title = title.as_deref().unwrap_or(&current.title);
    let next_description = description.as_deref().unwrap_or(&current.description);

    transaction.execute(
        "UPDATE ideas SET title = ?1, description = ?2 WHERE id = ?3",
        params![next_title, next_description, id.to_string()],
    )?;
    transaction.commit()?;

    Ok(Idea {
        id,
        title: next_title.to_owned(),
        description: next_description.to_owned(),
        status: current.status,
        promoted_to: current.promoted_to,
    })
}

pub fn discard(connection: &mut Connection, id: IdeaId) -> Result<Idea, StorageError> {
    let transaction = connection.transaction()?;
    let current = read_one(&transaction, &id)?.ok_or_else(|| StorageError::IdeaNotFound { id: id.to_string() })?;
    let next_status = current.status.apply(IdeaAction::Discard)?;

    if next_status != current.status {
        transaction.execute(
            "UPDATE ideas SET status = ?1 WHERE id = ?2",
            params![next_status.as_str(), id.to_string()],
        )?;
    }
    transaction.commit()?;

    Ok(Idea { status: next_status, ..current })
}

fn read_one(connection: &Connection, id: &IdeaId) -> Result<Option<Idea>, StorageError> {
    let raw_idea = connection
        .query_row(
            "SELECT i.id, i.title, i.description, i.status, p.epic_id
             FROM ideas i LEFT JOIN idea_promotions p ON p.idea_id = i.id
             WHERE i.id = ?1",
            params![id.to_string()],
            row_to_raw_idea,
        )
        .optional()?;
    raw_idea.map(decode_idea).transpose()
}

fn row_to_raw_idea(row: &rusqlite::Row<'_>) -> rusqlite::Result<(String, String, String, String, Option<String>)> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
}

fn decode_idea(
    (id, title, description, status, promoted_to): (String, String, String, String, Option<String>),
) -> Result<Idea, StorageError> {
    let id = IdeaId::parse(&id).map_err(DomainError::from)?;
    let status = IdeaStatus::parse(&status)?;
    let mut idea = Idea::from_parts(id, title, description, status).map_err(StorageError::from)?;
    idea.promoted_to = promoted_to
        .map(|id| crate::domain::EpicId::parse(&id).map_err(DomainError::from))
        .transpose()?;
    Ok(idea)
}

#[cfg(test)]
mod tests {
    use super::super::Database;
    use crate::{
        domain::{DomainError, IdeaStatus},
        storage::StorageError,
    };

    #[test]
    fn idea_mutations_are_transactional_and_discard_is_idempotent() {
        let mut database = Database::open_in_memory().expect("database opens");
        let idea = database
            .create_idea("First thought".to_owned(), "**Markdown**".to_owned())
            .expect("idea creates");

        let updated = database
            .update_idea(idea.id, Some("Renamed".to_owned()), None)
            .expect("idea updates");
        assert_eq!(updated.title, "Renamed");
        assert_eq!(updated.description, "**Markdown**");

        let discarded = database.discard_idea(idea.id).expect("idea discards");
        assert_eq!(discarded.status, IdeaStatus::Discarded);
        assert_eq!(
            database.discard_idea(idea.id).expect("discard repeats").status,
            IdeaStatus::Discarded
        );

        let error = database
            .update_idea(idea.id, Some("should fail".to_owned()), None)
            .expect_err("discarded ideas cannot be updated");
        assert!(matches!(
            error,
            StorageError::InvalidIdea(DomainError::InvalidTransition { .. })
        ));
        assert_eq!(
            database.idea(idea.id).expect("idea reads").expect("idea exists").title,
            "Renamed"
        );
    }

    #[test]
    fn invalid_create_does_not_insert_an_idea() {
        let mut database = Database::open_in_memory().expect("database opens");
        let error = database
            .create_idea("  ".to_owned(), "description".to_owned())
            .expect_err("empty title is rejected");
        assert!(matches!(error, StorageError::InvalidIdea(DomainError::EmptyTitle)));
        assert!(database.ideas().expect("ideas list").is_empty());
    }
}
