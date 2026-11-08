use serde::{Deserialize, Serialize};

use ulid::Ulid;

use super::{DomainError, NoteId, PhaseId, ProjectId, validate_title};

/// The bits of a ULID after its 48-bit timestamp.
const ULID_RANDOM_MASK: u128 = (1 << 80) - 1;

/// A Markdown research, decision, reference, or debugging record.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Note {
    pub id: NoteId,
    pub project_id: ProjectId,
    pub title: String,
    /// The editable Markdown note body.
    pub body: String,
}

impl Note {
    /// Create a note owned by a project.
    pub fn new(project_id: ProjectId, title: String, body: String) -> Result<Self, DomainError> {
        validate_title(&title)?;
        Ok(Self { id: NoteId::new(), project_id, title, body })
    }
}

/// The ID of the note `arcl compact` writes for `phase`.
///
/// The ID is derived from the phase's, so every clone that compacts the
/// phase writes the same note file. It keeps the phase ULID's timestamp and
/// inverts its 80 random bits. Distinct phases give distinct note IDs, and
/// the note's short ID differs from the phase's in every random character.
/// A generated note ID matches it only if it has the same millisecond and
/// the same 80 random bits.
pub fn compaction_note_id(phase: PhaseId) -> NoteId {
    let ulid = phase.ulid();
    NoteId::from_ulid(Ulid::from_parts(ulid.timestamp_ms(), !ulid.random() & ULID_RANDOM_MASK))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_compaction_note_id_is_stable_and_differs_from_the_phase_id() {
        let phase = PhaseId::parse("arcl-ph-01M6XY7S3HQ7SHW33WEY35QTAP").expect("ID parses");
        let note = compaction_note_id(phase);
        assert_eq!(note, compaction_note_id(phase));
        assert_eq!(NoteId::parse(&note.to_string()), Ok(note));
        assert_eq!(note.ulid().timestamp_ms(), phase.ulid().timestamp_ms());
        let (note, phase) = (note.ulid().to_string(), phase.ulid().to_string());
        assert!(
            note[10..]
                .chars()
                .zip(phase[10..].chars())
                .all(|(left, right)| left != right)
        );
        let other = PhaseId::parse("arcl-ph-01M6XY7S3HQ7SHW33WEY35QTAQ").expect("ID parses");
        assert_ne!(compaction_note_id(other).ulid().to_string(), note);
    }
}
