use rusqlite::{OptionalExtension, Transaction, params};

use crate::domain::{ContainerStatus, Epic, EpicId, IdeaId, IdeaStatus, ReleaseId};

use super::{Result, StorageError, epics, ideas};

/// The epic and source idea returned by an idempotent promotion.
#[derive(Clone, Debug)]
pub struct Promotion {
    /// The source inbox idea.
    pub idea: crate::domain::Idea,
    /// The linked spec-backed epic.
    pub epic: Epic,
}

pub fn promote(
    connection: &mut rusqlite::Connection, idea_id: IdeaId, title: String, description: String, spec_path: String,
    release_id: Option<ReleaseId>,
) -> Result<Promotion> {
    let transaction = connection.transaction()?;
    let idea =
        ideas::find(&transaction, &idea_id)?.ok_or_else(|| StorageError::IdeaNotFound { id: idea_id.to_string() })?;
    let relationship = transaction
        .query_row(
            "SELECT epic_id FROM idea_promotions WHERE idea_id = ?1",
            [idea_id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()?;

    match (idea.status, relationship) {
        (IdeaStatus::Promoted, Some(epic_id)) => {
            let epic_id = EpicId::parse(&epic_id)
                .map_err(crate::domain::DomainError::from)
                .map_err(StorageError::InvalidEpic)?;
            let epic = epics::find(&transaction, &epic_id)?
                .ok_or_else(|| StorageError::InconsistentPromotion { id: idea_id.to_string() })?;
            transaction.commit()?;
            Ok(Promotion { idea, epic })
        }
        (IdeaStatus::Promoted, None) | (IdeaStatus::Captured, Some(_)) => {
            Err(StorageError::InconsistentPromotion { id: idea_id.to_string() })
        }
        (IdeaStatus::Discarded, _) => {
            Err(StorageError::IdeaNotPromotable { id: idea_id.to_string(), status: idea.status.as_str().to_owned() })
        }
        (IdeaStatus::Captured, None) => {
            let epic = crate::domain::Epic::new(title, description, spec_path, release_id)
                .map_err(StorageError::InvalidEpic)?;
            ensure_release_exists(&transaction, release_id)?;
            ensure_spec_available(&transaction, &epic.spec_path)?;
            transaction.execute(
                "INSERT INTO epics (id, release_id, title, description, spec_path, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    epic.id.to_string(),
                    release_id.map(|id| id.to_string()),
                    epic.title,
                    epic.description,
                    epic.spec_path,
                    ContainerStatus::Open.as_str(),
                ],
            )?;
            transaction.execute(
                "INSERT INTO idea_promotions (idea_id, epic_id) VALUES (?1, ?2)",
                params![idea_id.to_string(), epic.id.to_string()],
            )?;
            transaction.execute(
                "UPDATE ideas SET status = 'promoted' WHERE id = ?1",
                [idea_id.to_string()],
            )?;
            let mut idea = idea;
            idea.status = IdeaStatus::Promoted;
            idea.promoted_to = Some(epic.id);
            let mut epic = epic;
            epic.source_idea = Some(idea_id);
            transaction.commit()?;
            Ok(Promotion { idea, epic })
        }
    }
}

fn ensure_release_exists(transaction: &Transaction<'_>, release_id: Option<ReleaseId>) -> Result<()> {
    let Some(release_id) = release_id else { return Ok(()) };
    let exists: Option<String> = transaction
        .query_row(
            "SELECT id FROM releases WHERE id = ?1",
            [release_id.to_string()],
            |row| row.get(0),
        )
        .optional()?;
    if exists.is_none() {
        return Err(StorageError::ReleaseNotFound { id: release_id.to_string() });
    }
    Ok(())
}

fn ensure_spec_available(transaction: &Transaction<'_>, spec_path: &str) -> Result<()> {
    let existing: Option<String> = transaction
        .query_row("SELECT id FROM epics WHERE spec_path = ?1", [spec_path], |row| {
            row.get(0)
        })
        .optional()?;
    if existing.is_some() {
        return Err(StorageError::DuplicateSpec { path: spec_path.to_owned() });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{Database, StorageError};
    use crate::domain::IdeaStatus;

    #[test]
    fn promotion_is_atomic_and_idempotent() {
        let mut database = Database::open_in_memory().expect("database opens");
        let idea = database
            .create_idea("Idea".to_owned(), "Details".to_owned())
            .expect("idea creates");
        let first = database
            .promote_idea(
                idea.id,
                idea.title.clone(),
                idea.description.clone(),
                "spec.md".to_owned(),
                None,
            )
            .expect("promotion succeeds");
        let second = database
            .promote_idea(
                idea.id,
                "Changed".to_owned(),
                "Changed".to_owned(),
                "other.md".to_owned(),
                None,
            )
            .expect("repeated promotion succeeds");
        assert_eq!(first.epic.id, second.epic.id);
        assert_eq!(database.epics().expect("epics list").len(), 1);
        assert_eq!(
            database.ideas().expect("ideas list")[0].promoted_to,
            Some(first.epic.id)
        );
        assert_eq!(database.epics().expect("epics list")[0].source_idea, Some(idea.id));
    }

    #[test]
    fn discarded_and_invalid_release_promotions_leave_the_idea_unchanged() {
        let mut database = Database::open_in_memory().expect("database opens");
        let discarded = database
            .create_idea("Discarded".to_owned(), String::new())
            .expect("idea creates");
        database.discard_idea(discarded.id).expect("idea discards");
        assert!(matches!(
            database.promote_idea(discarded.id, "x".to_owned(), String::new(), "x.md".to_owned(), None),
            Err(StorageError::IdeaNotPromotable { .. })
        ));
        assert_eq!(
            database
                .idea(discarded.id)
                .expect("idea reads")
                .expect("idea exists")
                .status,
            IdeaStatus::Discarded
        );

        let captured = database
            .create_idea("Captured".to_owned(), String::new())
            .expect("idea creates");
        let missing = crate::domain::ReleaseId::new();
        assert!(matches!(
            database.promote_idea(
                captured.id,
                "x".to_owned(),
                String::new(),
                "x.md".to_owned(),
                Some(missing)
            ),
            Err(StorageError::ReleaseNotFound { .. })
        ));
        assert_eq!(
            database
                .idea(captured.id)
                .expect("idea reads")
                .expect("idea exists")
                .status,
            IdeaStatus::Captured
        );
        assert!(database.epics().expect("epics list").is_empty());
    }

    #[test]
    fn promotion_relationships_use_database_uniqueness_constraints() {
        let database = Database::open_in_memory().expect("database opens");
        let error = database.connection().execute(
            "INSERT INTO idea_promotions (idea_id, epic_id) VALUES ('bad', 'bad')",
            [],
        );
        assert!(error.is_err());
    }
}
