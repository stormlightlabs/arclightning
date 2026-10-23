//! Two-way synchronization between the database and the workspace files.
//!
//! A sync compares the stored base, the database projection and the workspace
//! with [`compare`]. One-sided changes are carried to the other side; a path
//! both sides changed differently stops the sync before anything is written.
//!
//! Workspace files are written before the database commits the new graph and
//! base. A sync cut off between the two steps leaves states the next sync
//! classifies correctly: files that already hold the new bytes converge with
//! the database, and workspace edits that did not reach the database import
//! again. The same holds for an export cut off after a database commit, since
//! the base only moves once the files are written.

use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
};

use thiserror::Error;

use arcl_store::{Database, StorageError, WorkspaceBaseFile};

use super::{
    Change, RecordState, WorkspaceExportError, WorkspaceImportError, compare, decode_record,
    export::export_observed,
    import::{read_candidate, validate_candidate},
    project_graph,
};

/// The side of a sync that made a change.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Side {
    /// The operational database.
    Database,
    /// The workspace files.
    Workspace,
}

impl fmt::Display for Side {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Database => "database",
            Self::Workspace => "workspace",
        })
    }
}

/// A sync that could not be applied.
///
/// `Divergent`, `Removed` and `Import` write nothing. `Export` and `Storage`
/// may leave workspace files written without the database commit; the next
/// sync reconciles them.
#[derive(Debug, Error)]
pub enum WorkspaceSyncError {
    /// The database and the workspace changed these paths differently since
    /// the last sync.
    #[error(
        "the database and the workspace changed these files differently since the last sync: {}",
        list_paths(paths)
    )]
    Divergent { paths: Vec<PathBuf> },
    /// One side removed a file the stored base holds.
    #[error(
        "workspace file `{path}` was removed from the {side}; the workspace does not support hard deletion{}",
        removal_hint(*side)
    )]
    Removed { path: PathBuf, side: Side },
    /// The workspace could not be read, or the merged graph is invalid.
    #[error(transparent)]
    Import(Box<WorkspaceImportError>),
    /// A workspace file could not be rendered or written.
    #[error(transparent)]
    Export(#[from] WorkspaceExportError),
    /// The database could not be read or the result could not be committed.
    #[error(transparent)]
    Storage(#[from] StorageError),
}

impl From<WorkspaceImportError> for WorkspaceSyncError {
    fn from(error: WorkspaceImportError) -> Self {
        Self::Import(Box::new(error))
    }
}

impl WorkspaceSyncError {
    /// Return the CLI exit category for this sync failure.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Divergent { .. }
            | Self::Export(WorkspaceExportError::Conflict { .. })
            | Self::Storage(StorageError::ChangedSinceRead) => 4,
            Self::Removed { .. } => 3,
            Self::Import(error) => error.exit_code(),
            Self::Export(_) | Self::Storage(_) => 1,
        }
    }
}

/// The paths a sync carried from one side to the other.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SyncOutcome {
    /// Paths changed only in the workspace and imported into the database.
    pub imported: Vec<PathBuf>,
    /// Paths changed only in the database and exported to the workspace.
    pub exported: Vec<PathBuf>,
    /// Paths both sides changed the same way; only the base moved.
    pub converged: Vec<PathBuf>,
}

impl SyncOutcome {
    /// Return whether the sync found nothing to carry or record.
    pub fn is_empty(&self) -> bool {
        self.imported.is_empty() && self.exported.is_empty() && self.converged.is_empty()
    }
}

type Result<T> = std::result::Result<T, WorkspaceSyncError>;

/// Synchronize the database and the workspace at `root`.
///
/// Changes made on one side since the last sync are carried to the other.
/// The sync refuses when a path diverged or a record file was removed, and
/// validates the complete merged graph before writing anything. A commit from
/// another connection after the sync reads the database refuses the import
/// with [`StorageError::ChangedSinceRead`] instead of overwriting it.
pub fn sync_workspace(root: &Path, worktree_root: &Path, database: &mut Database) -> Result<SyncOutcome> {
    let read = database.data_version()?;
    let base = database.workspace_base()?;
    let database_files = project_graph(&database.connected_graph()?)?;
    let candidate = read_candidate(root)?;
    let comparison = compare(&base, &database_files, &candidate.files);

    let divergent = comparison
        .divergent()
        .map(|record| record.path.clone())
        .collect::<Vec<_>>();
    if !divergent.is_empty() {
        return Err(WorkspaceSyncError::Divergent { paths: divergent });
    }

    let workspace = candidate
        .files
        .iter()
        .map(|file| (&file.path, &file.content))
        .collect::<BTreeMap<_, _>>();
    let mut merged = database_files
        .iter()
        .map(|file| (file.path.clone(), file.content.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut outcome = SyncOutcome::default();
    for record in comparison.records {
        match record.state {
            RecordState::Unchanged => {}
            RecordState::DatabaseChanged(Change::Removed) => {
                return Err(WorkspaceSyncError::Removed { path: record.path, side: Side::Database });
            }
            RecordState::WorkspaceChanged(Change::Removed) => {
                return Err(WorkspaceSyncError::Removed { path: record.path, side: Side::Workspace });
            }
            RecordState::DatabaseChanged(_) => outcome.exported.push(record.path),
            RecordState::WorkspaceChanged(_) => {
                merged.insert(record.path.clone(), workspace[&record.path].clone());
                outcome.imported.push(record.path);
            }
            RecordState::Converged(_) => outcome.converged.push(record.path),
            RecordState::Divergent { .. } => unreachable!("divergent paths are refused above"),
        }
    }
    if outcome.is_empty() {
        return Ok(outcome);
    }

    let graph = if outcome.imported.is_empty() {
        None
    } else {
        let mut records = BTreeMap::new();
        for (path, content) in merged.iter().filter(|(path, _)| *path != Path::new("manifest.toml")) {
            // Both sides hold UTF-8: the projection renders it and the
            // workspace reader rejects anything else.
            let input = String::from_utf8_lossy(content);
            let record = decode_record(path, &input)
                .map_err(|source| WorkspaceImportError::Record { path: path.clone(), source })?;
            records.insert(path.clone(), record);
        }
        Some(validate_candidate(records, database.project()?, worktree_root)?)
    };
    let files = match &graph {
        Some(graph) => project_graph(graph)?,
        None => database_files,
    };

    export_observed(root, &files, &candidate.files)?;
    let base = files
        .iter()
        .map(|file| WorkspaceBaseFile { path: file.path.to_string_lossy().into_owned(), content: file.content.clone() })
        .collect::<Vec<_>>();
    match &graph {
        Some(graph) => database.replace_graph_and_workspace_base(graph, &base, read)?,
        None => database.replace_workspace_base(&base)?,
    }
    Ok(outcome)
}

fn removal_hint(side: Side) -> &'static str {
    match side {
        Side::Workspace => "; if a branch switch removed it, `arcl workspace export` writes it back",
        Side::Database => "",
    }
}

fn list_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| format!("`{}`", path.display()))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::workspace::{WorkspaceFile, export::export_graph_with_base};

    struct Project {
        root: tempfile::TempDir,
        database: Database,
    }

    impl Project {
        /// A project whose workspace and base hold one spec and one capture.
        fn synced() -> Self {
            let mut database = Database::open_in_memory().expect("database opens");
            database
                .create_spec("Spec".to_owned(), "Spec body".to_owned(), "criterion".to_owned())
                .expect("spec creates");
            database
                .create_capture("Capture".to_owned(), "Capture body".to_owned())
                .expect("capture creates");
            let root = tempfile::tempdir().expect("workspace creates");
            let mut project = Self { root, database };
            fs::write(project.root.path().join("manifest.toml"), "format-version = 1\n").expect("manifest writes");
            let first = project.sync().expect("first sync succeeds");
            assert!(!first.exported.is_empty());
            project
        }

        fn sync(&mut self) -> Result<SyncOutcome> {
            sync_workspace(self.root.path(), self.root.path(), &mut self.database)
        }

        fn path_of(&self, directory: &str) -> PathBuf {
            let entry = fs::read_dir(self.root.path().join(directory))
                .expect("directory reads")
                .next()
                .expect("directory has a record")
                .expect("entry reads");
            PathBuf::from(directory).join(entry.file_name())
        }

        fn edit(&self, relative: &Path, from: &str, to: &str) {
            let path = self.root.path().join(relative);
            let content = fs::read_to_string(&path).expect("record reads");
            assert!(content.contains(from), "`{from}` is not in {}", relative.display());
            fs::write(&path, content.replace(from, to)).expect("record writes");
        }

        fn read(&self, relative: &Path) -> String {
            fs::read_to_string(self.root.path().join(relative)).expect("record reads")
        }
    }

    #[test]
    fn a_clean_project_syncs_without_changes() {
        let mut project = Project::synced();
        let before = project.database.workspace_base().expect("base loads");
        let outcome = project.sync().expect("sync succeeds");
        assert!(outcome.is_empty());
        assert_eq!(project.database.workspace_base().expect("base loads"), before);
    }

    #[test]
    fn workspace_edits_import_into_the_database() {
        let mut project = Project::synced();
        let spec = project.path_of("specs");
        project.edit(&spec, "Spec body", "Edited in the workspace");

        let outcome = project.sync().expect("sync succeeds");
        assert_eq!(outcome.imported, std::slice::from_ref(&spec));
        let graph = project.database.connected_graph().expect("graph loads");
        assert_eq!(graph.specs[0].body, "Edited in the workspace");
        assert!(project.sync().expect("second sync succeeds").is_empty());
    }

    #[test]
    fn database_changes_export_to_the_workspace() {
        let mut project = Project::synced();
        let note = project
            .database
            .create_note("Note".to_owned(), "Written in the database".to_owned())
            .expect("note creates");

        let outcome = project.sync().expect("sync succeeds");
        let path = PathBuf::from(format!("notes/{}.md", note.id));
        assert_eq!(outcome.exported, std::slice::from_ref(&path));
        assert!(project.read(&path).contains("Written in the database"));
        assert!(project.sync().expect("second sync succeeds").is_empty());
    }

    #[test]
    fn changes_on_both_sides_to_different_records_merge() {
        let mut project = Project::synced();
        let spec = project.path_of("specs");
        project.edit(&spec, "Spec body", "Edited in the workspace");
        let note = project
            .database
            .create_note("Note".to_owned(), "Written in the database".to_owned())
            .expect("note creates");

        let outcome = project.sync().expect("sync succeeds");
        let note_path = PathBuf::from(format!("notes/{}.md", note.id));
        assert_eq!(outcome.imported, [spec]);
        assert_eq!(outcome.exported, std::slice::from_ref(&note_path));
        let graph = project.database.connected_graph().expect("graph loads");
        assert_eq!(graph.specs[0].body, "Edited in the workspace");
        assert_eq!(graph.notes[0].id, note.id);
        assert!(project.read(&note_path).contains("Written in the database"));
    }

    #[test]
    fn divergent_changes_stop_before_anything_is_written() {
        let mut project = Project::synced();
        let capture = project.path_of("captures");
        project.edit(&capture, "Capture body", "Edited in the workspace");
        let id = project.database.connected_graph().expect("graph loads").captures[0].id;
        project
            .database
            .update_capture(
                id,
                arcl_store::CaptureUpdate { title: None, body: Some("Edited in the database".to_owned()) },
            )
            .expect("capture updates");
        let base = project.database.workspace_base().expect("base loads");

        let error = project.sync().expect_err("divergence refuses");
        assert!(
            matches!(&error, WorkspaceSyncError::Divergent { paths } if paths.as_slice() == std::slice::from_ref(&capture))
        );
        assert_eq!(error.exit_code(), 4);
        assert!(project.read(&capture).contains("Edited in the workspace"));
        assert_eq!(
            project.database.connected_graph().expect("graph loads").captures[0].body,
            "Edited in the database"
        );
        assert_eq!(project.database.workspace_base().expect("base loads"), base);
    }

    #[test]
    fn a_removed_record_file_is_refused() {
        let mut project = Project::synced();
        let capture = project.path_of("captures");
        fs::remove_file(project.root.path().join(&capture)).expect("record removes");

        let error = project.sync().expect_err("removal refuses");
        assert!(matches!(
            &error,
            WorkspaceSyncError::Removed { path, side: Side::Workspace } if path == &capture
        ));
        assert_eq!(error.exit_code(), 3);
    }

    #[test]
    fn an_invalid_merged_graph_writes_nothing() {
        let mut project = Project::synced();
        let spec = project.path_of("specs");
        // Decodes, but links to a note that does not exist.
        let missing = arcl_core::domain::NoteId::new();
        project.edit(
            &spec,
            "status = \"open\"",
            &format!("status = \"open\"\nlinks = [{{ kind = \"note\", id = \"{missing}\" }}]"),
        );
        let note = project
            .database
            .create_note("Note".to_owned(), "Not exported yet".to_owned())
            .expect("note creates");
        let base = project.database.workspace_base().expect("base loads");

        let error = project.sync().expect_err("invalid graph refuses");
        assert!(
            matches!(&error, WorkspaceSyncError::Import(import) if matches!(**import, WorkspaceImportError::Relationship { .. })),
            "{error:?}"
        );
        assert_eq!(error.exit_code(), 3);
        assert!(!project.root.path().join(format!("notes/{}.md", note.id)).exists());
        assert_eq!(project.database.workspace_base().expect("base loads"), base);
    }

    #[test]
    fn an_import_cut_off_after_the_file_writes_imports_on_the_next_sync() {
        let mut project = Project::synced();
        let spec = project.path_of("specs");
        project.edit(&spec, "Spec body", "Written, never committed");
        let base = project.database.workspace_base().expect("base loads");

        // Run a sync's steps up to its file writes, then stop before the
        // database commits the merged graph and the new base.
        let candidate = read_candidate(project.root.path()).expect("workspace reads");
        let graph = validate_candidate(
            candidate.records,
            project.database.project().expect("project loads"),
            project.root.path(),
        )
        .expect("merged graph validates");
        let files = project_graph(&graph).expect("projection succeeds");
        export_observed(project.root.path(), &files, &candidate.files).expect("files write");
        assert_eq!(
            project.database.connected_graph().expect("graph loads").specs[0].body,
            "Spec body"
        );
        assert_eq!(project.database.workspace_base().expect("base loads"), base);

        let outcome = project.sync().expect("sync succeeds");
        assert_eq!(outcome.imported, std::slice::from_ref(&spec));
        assert_eq!(
            project.database.connected_graph().expect("graph loads").specs[0].body,
            "Written, never committed"
        );
        assert!(project.sync().expect("second sync succeeds").is_empty());
    }

    #[test]
    fn files_written_before_the_base_moved_converge_on_the_next_sync() {
        let mut project = Project::synced();
        let note = project
            .database
            .create_note("Note".to_owned(), "Written, base not stored".to_owned())
            .expect("note creates");
        let base = project.database.workspace_base().expect("base loads");
        let written: Vec<WorkspaceFile> = export_graph_with_base(
            project.root.path(),
            &project.database.connected_graph().expect("graph loads"),
            &base,
        )
        .expect("export writes files");
        assert!(!written.is_empty());

        let outcome = project.sync().expect("sync succeeds");
        let path = PathBuf::from(format!("notes/{}.md", note.id));
        assert!(outcome.imported.is_empty());
        assert!(outcome.exported.is_empty());
        assert_eq!(outcome.converged, [path]);
        assert!(project.sync().expect("second sync succeeds").is_empty());
    }
}
