//! Reading and saving one record's workspace file, as the web app's raw
//! record editor does.
//!
//! The file is the record's canonical projection, the same bytes a workspace
//! export writes. A save is checked as a workspace import checks a file: it
//! must parse, match the record's kind and ID, and keep every relationship
//! and blocker rule of the complete graph. Its revision must also match the
//! file it was edited from.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use thiserror::Error;

use arcl_store::{ConnectedGraph, Database, StorageError};

use super::import::validate_candidate;
use super::{RecordKind, WorkspaceExportError, WorkspaceFile, WorkspaceImportError, decode_record, project_graph};

/// A failure to read or save one record's file.
#[derive(Debug, Error)]
pub enum RecordEditError {
    /// No record has this ID.
    #[error("record `{id}` was not found")]
    NotFound { id: String },
    /// The record's file changed since the revision the edit started from.
    #[error(
        "workspace file `{path}` changed since it was opened; reload it and make the edit again",
        path = .path.display()
    )]
    Stale { path: PathBuf },
    /// The edited file failed the import's checks.
    #[error(transparent)]
    Invalid(Box<WorkspaceImportError>),
    /// The graph could not be rendered as workspace files.
    #[error(transparent)]
    Export(Box<WorkspaceExportError>),
    /// The database could not be read or written.
    #[error(transparent)]
    Storage(#[from] StorageError),
}

impl RecordEditError {
    /// Return the CLI exit category for this failure.
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::NotFound { .. } => 5,
            Self::Stale { .. } => 4,
            Self::Invalid(error) => error.exit_code(),
            Self::Export(_) => 1,
            Self::Storage(error) => error.exit_code(),
        }
    }
}

impl From<WorkspaceImportError> for RecordEditError {
    fn from(error: WorkspaceImportError) -> Self {
        Self::Invalid(Box::new(error))
    }
}

impl From<WorkspaceExportError> for RecordEditError {
    fn from(error: WorkspaceExportError) -> Self {
        Self::Export(Box::new(error))
    }
}

/// One record's workspace file and the revision that names its content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordSource {
    /// The record ID.
    pub id: String,
    /// The record kind, such as `task`.
    pub kind: RecordKind,
    /// The path relative to the workspace root, such as `tasks/<id>.md`.
    pub path: PathBuf,
    /// The canonical file: TOML front matter between `+++` lines, then the
    /// Markdown body.
    pub content: String,
    /// See [`source_revision`].
    pub revision: String,
}

impl RecordSource {
    fn from_file(id: &str, file: &WorkspaceFile) -> Option<Self> {
        let kind = file
            .path
            .parent()
            .and_then(Path::to_str)
            .and_then(RecordKind::from_directory)?;
        Some(Self {
            id: id.to_owned(),
            kind,
            path: file.path.clone(),
            content: String::from_utf8_lossy(&file.content).into_owned(),
            revision: source_revision(&file.content),
        })
    }
}

/// What a saved edit wrote.
#[derive(Debug)]
pub struct RecordEdit {
    /// The record's canonical file after the save.
    pub source: RecordSource,
    /// One message for each blocker rule violation the graph holds that the
    /// edit did not introduce, as [`import_graph`](super::import_graph)
    /// reports them.
    pub warnings: Vec<String>,
}

/// The revision of a record file: the lowercase hex SHA-256 of its bytes.
/// An edit names the revision it started from, and a save refuses it when
/// the file no longer has that revision.
pub fn source_revision(content: &[u8]) -> String {
    Sha256::digest(content)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Return the canonical file of the record `id` in `graph`, or `None` when
/// the graph has no such record.
pub fn record_source(graph: &ConnectedGraph, id: &str) -> Result<Option<RecordSource>, RecordEditError> {
    let files = project_graph(graph)?;
    Ok(find(&files, id).and_then(|file| RecordSource::from_file(id, file)))
}

/// Replace the record `id` with `content`, a complete record file, when the
/// record's file still has `revision`.
///
/// The file is decoded at the record's own path, so its kind and ID cannot
/// change, and the graph with the edit in place passes the same checks as a
/// workspace import before the database changes. The edit is compared with
/// the database graph whether or not a workspace base exists, so blocker rule
/// violations it introduces are refused as `add_planning_dependency` refuses
/// them; ones already in the database load with a warning. The workspace base
/// is left as it was, so the next sync exports the change to the workspace. A
/// failure changes nothing.
pub fn edit_record(
    database: &mut Database, id: &str, content: &str, revision: &str,
) -> Result<RecordEdit, RecordEditError> {
    // Taken before any read, so a commit from another connection after this
    // point refuses the replacement instead of being overwritten.
    let read = database.data_version()?;
    let project = database.project()?;
    let base = database.workspace_base()?;
    let before = database.connected_graph()?;
    let files = project_graph(&before)?;
    let Some(current) = find(&files, id) else {
        return Err(RecordEditError::NotFound { id: id.to_owned() });
    };
    let path = current.path.clone();
    if source_revision(&current.content) != revision {
        return Err(RecordEditError::Stale { path });
    }

    let mut records = BTreeMap::new();
    for file in files.iter().filter(|file| file.path.parent() != Some(Path::new(""))) {
        let input = if file.path == path {
            content.to_owned()
        } else {
            String::from_utf8_lossy(&file.content).into_owned()
        };
        let record = decode_record(&file.path, &input)
            .map_err(|source| WorkspaceImportError::Record { path: file.path.clone(), source })?;
        records.insert(file.path.clone(), record);
    }
    let (graph, warnings) = validate_candidate(records, project, Path::new(""), Some(&before))?;
    let source = record_source(&graph, id)?.ok_or_else(|| RecordEditError::NotFound { id: id.to_owned() })?;
    database.replace_graph_and_workspace_base(&graph, &base, read)?;
    Ok(RecordEdit { source, warnings })
}

/// The record file whose name is `id`, leaving out `manifest.toml`.
fn find<'a>(files: &'a [WorkspaceFile], id: &str) -> Option<&'a WorkspaceFile> {
    files.iter().find(|file| {
        file.path.parent() != Some(Path::new("")) && file.path.file_stem().and_then(|stem| stem.to_str()) == Some(id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcl_core::domain::{TaskId, TaskPriority};
    use arcl_store::PlanningTaskCreate;
    use arcl_store::WorkspaceBaseFile;

    fn task(database: &mut Database, title: &str) -> String {
        let project = database.project().expect("project exists");
        database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: None,
                plan_id: None,
                phase_id: None,
                parent_id: None,
                title: title.to_owned(),
                body: "Body".to_owned(),
                priority: TaskPriority::Normal,
                position: 0,
            })
            .expect("task creates")
            .id
            .to_string()
    }

    fn source(database: &Database, id: &str) -> RecordSource {
        record_source(&database.connected_graph().expect("graph reads"), id)
            .expect("graph renders")
            .expect("record exists")
    }

    #[test]
    fn source_is_the_canonical_file_with_its_revision() {
        let mut database = Database::open_in_memory().expect("database opens");
        let id = task(&mut database, "First");
        let source = source(&database, &id);
        assert_eq!(source.kind, RecordKind::Task);
        assert_eq!(source.path, PathBuf::from(format!("tasks/{id}.md")));
        assert!(source.content.starts_with(&format!("+++\nid = \"{id}\"\n")));
        assert!(source.content.ends_with("+++\n\nBody\n"));
        assert_eq!(source.revision, source_revision(source.content.as_bytes()));
        assert_eq!(source.revision.len(), 64);
    }

    #[test]
    fn an_edit_replaces_the_record_and_keeps_the_base() {
        let mut database = Database::open_in_memory().expect("database opens");
        let id = task(&mut database, "First");
        let before = source(&database, &id);
        let content = before
            .content
            .replace("title = \"First\"", "title = \"Renamed\"")
            .replace("Body", "New body");
        let edit = edit_record(&mut database, &id, &content, &before.revision).expect("edit saves");
        assert_eq!(edit.source.content, content);
        let task = database
            .planning_task(TaskId::parse(&id).expect("ID parses"))
            .expect("task reads")
            .expect("task exists");
        assert_eq!(task.title, "Renamed");
        assert_eq!(task.body, "New body");
        assert!(database.workspace_base().expect("base reads").is_empty());
    }

    #[test]
    fn a_stale_revision_is_refused() {
        let mut database = Database::open_in_memory().expect("database opens");
        let id = task(&mut database, "First");
        let before = source(&database, &id);
        let error = edit_record(&mut database, &id, &before.content, &source_revision(b"other")).unwrap_err();
        assert!(matches!(error, RecordEditError::Stale { .. }));
        assert_eq!(error.exit_code(), 4);
    }

    #[test]
    fn invalid_files_are_refused_with_the_import_error_and_change_nothing() {
        let mut database = Database::open_in_memory().expect("database opens");
        let id = task(&mut database, "First");
        let other = task(&mut database, "Second");
        let before = source(&database, &id);
        let cases = [
            ("+++\ntitle = \n+++\n".to_owned(), "front matter TOML is invalid"),
            (before.content.replace(&id, &other), "does not match filename"),
            (
                format!("+++\nid = \"{id}\"\ntitle = \"Spec\"\nstatus = \"open\"\nacceptance-criteria = \"x\"\n+++\n"),
                "unknown field",
            ),
            (
                before
                    .content
                    .replace("priority", "parent = \"arcl-t-01M5STHSRRT2ADFN3Y9YAC2N7A\"\npriority"),
                "relationship `parent`",
            ),
            (before.content.replace("+++\n\n", ""), "closing `+++`"),
        ];
        for (content, message) in cases {
            let error = edit_record(&mut database, &id, &content, &before.revision).unwrap_err();
            assert_eq!(error.exit_code(), 3, "{content}");
            assert!(error.to_string().contains(message), "{error}");
        }
        assert_eq!(source(&database, &id), before);
    }

    fn base(database: &Database) -> Vec<WorkspaceBaseFile> {
        project_graph(&database.connected_graph().expect("graph reads"))
            .expect("graph renders")
            .into_iter()
            .map(|file| WorkspaceBaseFile { path: file.path.to_string_lossy().into_owned(), content: file.content })
            .collect()
    }

    /// Add `blocked-by = [blocker]` to `id` through a raw edit.
    fn block(database: &mut Database, id: &str, blocker: &str) -> Result<RecordEdit, RecordEditError> {
        let before = source(database, id);
        let content = before
            .content
            .replace("position = 0", &format!("position = 0\nblocked-by = [\"{blocker}\"]"));
        edit_record(database, id, &content, &before.revision)
    }

    #[test]
    fn a_blocker_cycle_the_edit_adds_is_refused_with_or_without_a_base() {
        for synced in [false, true] {
            let mut database = Database::open_in_memory().expect("database opens");
            let first = task(&mut database, "First");
            let second = task(&mut database, "Second");
            database
                .add_planning_dependency(
                    TaskId::parse(&second).expect("ID parses"),
                    TaskId::parse(&first).expect("ID parses"),
                )
                .expect("dependency adds");
            // Without a base, as after `arcl init` without `--workspace`, the
            // edit is still compared with the database graph.
            if synced {
                let base = base(&database);
                database.replace_workspace_base(&base).expect("base saves");
            }
            let before = source(&database, &first);
            let error = block(&mut database, &first, &second).unwrap_err();
            assert_eq!(error.exit_code(), 3, "{error}");
            assert!(error.to_string().contains("blocked-by"), "{error}");
            assert_eq!(source(&database, &first), before);
        }
    }

    #[test]
    fn a_blocker_on_an_ancestor_is_refused_without_a_base() {
        let mut database = Database::open_in_memory().expect("database opens");
        let parent = task(&mut database, "Parent");
        let project = database.project().expect("project exists");
        let child = database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: None,
                plan_id: None,
                phase_id: None,
                parent_id: Some(TaskId::parse(&parent).expect("ID parses")),
                title: "Child".to_owned(),
                body: "Body".to_owned(),
                priority: TaskPriority::Normal,
                position: 0,
            })
            .expect("child creates")
            .id
            .to_string();
        assert!(database.workspace_base().expect("base reads").is_empty());
        let before = source(&database, &child);
        let error = block(&mut database, &child, &parent).unwrap_err();
        assert_eq!(error.exit_code(), 3, "{error}");
        assert!(error.to_string().contains("blocked-by"), "{error}");
        assert_eq!(source(&database, &child), before);
    }

    #[test]
    fn a_blocker_without_a_violation_saves_without_a_base() {
        let mut database = Database::open_in_memory().expect("database opens");
        let first = task(&mut database, "First");
        let second = task(&mut database, "Second");
        let edit = block(&mut database, &first, &second).expect("edit saves");
        assert!(edit.warnings.is_empty(), "{:?}", edit.warnings);
        assert!(edit.source.content.contains(&second));
    }

    #[test]
    fn an_unknown_record_is_not_found() {
        let mut database = Database::open_in_memory().expect("database opens");
        let error = edit_record(&mut database, "arcl-t-01M5STHSRRT2ADFN3Y9YAC2N7A", "", "").unwrap_err();
        assert_eq!(error.exit_code(), 5);
    }
}
