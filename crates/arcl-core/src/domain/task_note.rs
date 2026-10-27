use serde::{Deserialize, Serialize};

use super::{DomainError, TaskId, error::is_format_character};

/// The longest task note, in characters.
pub const MAX_TASK_NOTE_CHARS: usize = 2000;

/// What a task note records. A note may also have no kind.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum TaskNoteKind {
    /// A choice made while working, and why.
    Decision,
    /// Something found while working.
    Observation,
    /// Something that stops the work.
    Blocker,
    /// The outcome of an attempt.
    Result,
}

impl TaskNoteKind {
    /// Return the stable value stored in SQLite, workspace files and JSON.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Decision => "decision",
            Self::Observation => "observation",
            Self::Blocker => "blocker",
            Self::Result => "result",
        }
    }

    /// Parse a persisted or command-line note kind.
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "decision" => Ok(Self::Decision),
            "observation" => Ok(Self::Observation),
            "blocker" => Ok(Self::Blocker),
            "result" => Ok(Self::Result),
            _ => Err(DomainError::InvalidTaskNoteKind { value: value.to_owned() }),
        }
    }
}

/// A short timestamped note appended to a task while working on it.
///
/// Notes are append-only. They stay on the task whatever its status, and
/// never affect readiness.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TaskNote {
    /// The task the note belongs to.
    pub task_id: TaskId,
    /// When the note was added, as an RFC 3339 UTC timestamp.
    pub created_at: String,
    /// What the note records, when given.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<TaskNoteKind>,
    /// The plain-text note, which may span lines.
    pub text: String,
}

/// Return note `text` with line endings normalized to LF and surrounding
/// whitespace trimmed, or refuse text that is then empty, longer than
/// [`MAX_TASK_NOTE_CHARS`] characters, holds a control character other than
/// a newline or tab, or a format character, or has a line that is exactly
/// `+++`. That line delimits a workspace record's front matter, so a note
/// holding it could not be read back from its task file.
pub fn validate_task_note_text(text: &str) -> Result<String, DomainError> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = normalized.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > MAX_TASK_NOTE_CHARS
        || trimmed
            .chars()
            .any(|c| (c.is_control() && c != '\n' && c != '\t') || is_format_character(c))
        || trimmed.lines().any(|line| line == "+++")
    {
        Err(DomainError::InvalidTaskNote)
    } else {
        Ok(trimmed.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_text_is_trimmed_and_may_span_lines() {
        assert_eq!(
            validate_task_note_text("  first\r\n\tsecond\n"),
            Ok("first\n\tsecond".to_owned())
        );
    }

    #[test]
    fn note_text_refuses_empty_long_and_hidden_characters() {
        let longest = "é".repeat(MAX_TASK_NOTE_CHARS);
        assert_eq!(validate_task_note_text(&longest), Ok(longest.clone()));
        for text in [
            " \n ".to_owned(),
            format!("{longest}e"),
            "bell\u{7}".to_owned(),
            "zero\u{200B}width".to_owned(),
            "\u{202E}reversed".to_owned(),
            "before\n+++\nafter".to_owned(),
        ] {
            assert_eq!(
                validate_task_note_text(&text),
                Err(DomainError::InvalidTaskNote),
                "{}",
                text.escape_debug()
            );
        }
    }

    #[test]
    fn kinds_round_trip_through_their_names() {
        for kind in [
            TaskNoteKind::Decision,
            TaskNoteKind::Observation,
            TaskNoteKind::Blocker,
            TaskNoteKind::Result,
        ] {
            assert_eq!(TaskNoteKind::parse(kind.as_str()), Ok(kind));
        }
        assert!(matches!(
            TaskNoteKind::parse("idea"),
            Err(DomainError::InvalidTaskNoteKind { .. })
        ));
    }
}
