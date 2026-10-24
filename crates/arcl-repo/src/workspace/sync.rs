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
    path::{Component, Path, PathBuf},
};

use thiserror::Error;

use arcl_store::{Database, StorageError, WorkspaceBaseFile};

use super::{
    Change, RecordState, WorkspaceExportError, WorkspaceImportError, compare, decode_record,
    export::export_observed,
    import::{comparison_graph, read_candidate, reject_unmerged, validate_candidate},
    list_paths, project_graph,
};
use crate::vcs::Vcs;

/// The workspace manifest. It has one legal value, which the database
/// projection always writes, so the workspace's copy is never imported.
const MANIFEST: &str = "manifest.toml";

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
/// `Unmerged`, `Divergent`, `NothingToResolve`, `Removed` and `Import` write
/// nothing. `Export` and `Storage`
/// may leave workspace files written without the database commit; the next
/// sync reconciles them.
#[derive(Debug, Error)]
pub enum WorkspaceSyncError {
    /// Git reports these workspace files as unmerged.
    #[error(
        "workspace files are unmerged in Git: {}; finish the merge, then sync again",
        list_paths(paths)
    )]
    Unmerged { paths: Vec<PathBuf> },
    /// The database and the workspace changed these paths differently since
    /// the last sync.
    #[error(
        "the database and the workspace changed these files differently since the last sync: {}",
        list_paths(paths)
    )]
    Divergent { paths: Vec<PathBuf> },
    /// A path named for resolution has nothing to resolve.
    #[error(
        "workspace file `{path}` has nothing to resolve; resolve takes files the database and the workspace changed differently, or a record file removed from the workspace; paths are workspace-relative, as `arcl workspace status` prints them"
    )]
    NothingToResolve { path: PathBuf },
    /// One side removed a file the stored base holds.
    #[error(
        "workspace file `{path}` was removed from the {side}; the workspace does not support hard deletion{}",
        removal_hint(path, *side)
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
        match error {
            WorkspaceImportError::Unmerged { paths } => Self::Unmerged { paths },
            error => Self::Import(Box::new(error)),
        }
    }
}

impl WorkspaceSyncError {
    /// Return the CLI exit category for this sync failure.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Unmerged { .. }
            | Self::Divergent { .. }
            | Self::Export(WorkspaceExportError::Conflict { .. })
            | Self::Storage(StorageError::ChangedSinceRead) => 4,
            Self::Removed { .. } | Self::NothingToResolve { .. } => 3,
            Self::Import(error) => error.exit_code(),
            Self::Export(_) | Self::Storage(_) => 1,
        }
    }

    /// The hint to add when this failure stops a change: `arcl workspace
    /// resolve` only for files the database and the workspace changed
    /// differently, which it settles, and empty otherwise. A file removed from
    /// the workspace carries its own `resolve --keep database` hint.
    pub const fn resolve_hint(&self) -> &'static str {
        match self {
            Self::Divergent { .. } => "; `arcl workspace resolve` settles files both sides changed",
            _ => "",
        }
    }
}

/// The paths a sync carried from one side to the other.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SyncOutcome {
    /// Paths changed only in the workspace, or resolved in its favour, and
    /// imported into the database.
    pub imported: Vec<PathBuf>,
    /// Paths changed only in the database, or resolved in its favour, and
    /// exported to the workspace.
    pub exported: Vec<PathBuf>,
    /// Paths both sides changed the same way; only the base moved. A changed
    /// `manifest.toml` is listed here too, after its canonical form is
    /// written back over the workspace copy.
    pub converged: Vec<PathBuf>,
    /// One message for each blocker rule violation the merged graph holds
    /// that the workspace changes did not introduce. Sync loads these and
    /// refuses only the violations a workspace change causes.
    pub warnings: Vec<String>,
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
/// The sync refuses when Git reports a workspace file as unmerged, a path
/// diverged or a record file was removed, and validates the complete merged graph before writing anything. A commit from
/// another connection after the sync reads the database refuses the import
/// with [`StorageError::ChangedSinceRead`] instead of overwriting it. Pass
/// `vcs` when the project has a Git worktree; without it there is no Git
/// state to check.
pub fn sync_workspace(
    root: &Path, worktree_root: &Path, database: &mut Database, vcs: Option<&dyn Vcs>,
) -> Result<SyncOutcome> {
    run_sync(root, worktree_root, database, vcs, &BTreeMap::new())
}

/// Resolve `paths` by keeping one side's version, then sync everything else.
///
/// Each path must be one the database and the workspace changed differently,
/// or a record file removed from the workspace. `manifest.toml` is never
/// resolvable, since sync always writes its one legal value. Keeping the database writes
/// its version to the workspace; keeping the workspace imports the file. The
/// workspace cannot win a removal, since records are never hard deleted.
/// Other divergent paths still stop the sync, and nothing is written then.
///
/// A relative path is first read against `current_dir`, following symbolic
/// links; workspace-relative paths, as `arcl workspace status` prints them,
/// work from any directory. See [`workspace_relative`].
pub fn resolve_workspace(
    root: &Path, worktree_root: &Path, current_dir: &Path, database: &mut Database, vcs: Option<&dyn Vcs>,
    paths: &[PathBuf], keep: Side,
) -> Result<SyncOutcome> {
    let choices = paths
        .iter()
        .map(|path| (workspace_relative(path, current_dir, root, worktree_root), keep))
        .collect();
    run_sync(root, worktree_root, database, vcs, &choices)
}

/// Return `path` relative to the workspace at `root`.
///
/// A relative `path` is joined onto `current_dir`. When that file, or its
/// parent directory for a removed file, exists and lies inside `root` after
/// following symbolic links and `..` segments, the result is its path under
/// `root`.
///
/// Otherwise the path is read as named: leading `./` components are dropped,
/// an absolute path under `root` loses that prefix, and a relative path that
/// starts with the workspace's path relative to `worktree_root` (such as
/// `.arcl/workspace/captures/x.md`) loses that prefix. Any other path is
/// returned without its leading `./`, and matches no workspace file.
pub fn workspace_relative(path: &Path, current_dir: &Path, root: &Path, worktree_root: &Path) -> PathBuf {
    if let Some(relative) = relative_on_disk(&current_dir.join(path), root) {
        return relative;
    }
    if path.is_absolute() {
        return path.strip_prefix(root).unwrap_or(path).to_owned();
    }
    let path = path
        .components()
        .skip_while(|component| matches!(component, Component::CurDir))
        .collect::<PathBuf>();
    match root.strip_prefix(worktree_root) {
        Ok(prefix) if !prefix.as_os_str().is_empty() => path.strip_prefix(prefix).map(Path::to_owned).unwrap_or(path),
        _ => path,
    }
}

/// Return the path of `named` under `root` after following symbolic links,
/// when `named` or its parent exists and lies inside `root`.
fn relative_on_disk(named: &Path, root: &Path) -> Option<PathBuf> {
    let root = root.canonicalize().ok()?;
    let canonical = named.canonicalize().ok().or_else(|| {
        let name = named.file_name()?;
        Some(named.parent()?.canonicalize().ok()?.join(name))
    })?;
    let relative = canonical.strip_prefix(&root).ok()?;
    (!relative.as_os_str().is_empty()).then(|| relative.to_owned())
}

fn run_sync(
    root: &Path, worktree_root: &Path, database: &mut Database, vcs: Option<&dyn Vcs>,
    choices: &BTreeMap<PathBuf, Side>,
) -> Result<SyncOutcome> {
    reject_unmerged(root, vcs)?;
    let read = database.data_version()?;
    let base = database.workspace_base()?;
    let before = database.connected_graph()?;
    let database_files = project_graph(&before)?;
    let candidate = read_candidate(root)?;
    let comparison = compare(&base, &database_files, &candidate.files);

    for path in choices.keys() {
        let resolvable = path != Path::new(MANIFEST)
            && comparison.records.iter().any(|record| {
                &record.path == path
                    && matches!(
                        record.state,
                        RecordState::Divergent { .. } | RecordState::WorkspaceChanged(Change::Removed)
                    )
            });
        if !resolvable {
            return Err(WorkspaceSyncError::NothingToResolve { path: path.clone() });
        }
    }
    let divergent = comparison
        .divergent()
        .filter(|record| record.path != Path::new(MANIFEST) && !choices.contains_key(&record.path))
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
        if record.path == Path::new(MANIFEST) {
            // The workspace reader requires the manifest, so it is never
            // removed there. Any other change gives way to the canonical copy.
            if record.state != RecordState::Unchanged {
                outcome.converged.push(record.path);
            }
            continue;
        }
        if let Some(keep) = choices.get(&record.path) {
            match (keep, workspace.get(&record.path)) {
                (Side::Database, _) => outcome.exported.push(record.path),
                (Side::Workspace, Some(content)) => {
                    merged.insert(record.path.clone(), (*content).clone());
                    outcome.imported.push(record.path);
                }
                (Side::Workspace, None) => {
                    return Err(WorkspaceSyncError::Removed { path: record.path, side: Side::Workspace });
                }
            }
            continue;
        }
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
        for (path, content) in merged.iter().filter(|(path, _)| *path != Path::new(MANIFEST)) {
            // Both sides hold UTF-8: the projection renders it and the
            // workspace reader rejects anything else.
            let input = String::from_utf8_lossy(content);
            let record = decode_record(path, &input)
                .map_err(|source| WorkspaceImportError::Record { path: path.clone(), source })?;
            records.insert(path.clone(), record);
        }
        let (graph, warnings) = validate_candidate(
            records,
            database.project()?,
            worktree_root,
            comparison_graph(&before, &base),
        )?;
        outcome.warnings = warnings;
        Some(graph)
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

fn removal_hint(path: &Path, side: Side) -> String {
    match side {
        Side::Workspace => {
            format!(
                "; `arcl workspace resolve --keep database {}` writes it back",
                path.display()
            )
        }
        Side::Database => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::vcs::{FakeVcs, PathState};
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
            self.sync_with(None)
        }

        fn sync_with(&mut self, vcs: Option<&dyn Vcs>) -> Result<SyncOutcome> {
            sync_workspace(self.root.path(), self.root.path(), &mut self.database, vcs)
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

        /// Edit the synced capture differently in the workspace and the database.
        fn diverge_capture(&mut self) -> PathBuf {
            let capture = self.path_of("captures");
            self.edit(&capture, "Capture body", "Edited in the workspace");
            let id = self.database.connected_graph().expect("graph loads").captures[0].id;
            self.database
                .update_capture(
                    id,
                    arcl_store::CaptureUpdate { title: None, body: Some("Edited in the database".to_owned()) },
                )
                .expect("capture updates");
            capture
        }

        fn resolve(&mut self, paths: &[PathBuf], keep: Side) -> Result<SyncOutcome> {
            let root = self.root.path().to_owned();
            self.resolve_from(&root, paths, keep)
        }

        fn resolve_from(&mut self, current_dir: &Path, paths: &[PathBuf], keep: Side) -> Result<SyncOutcome> {
            resolve_workspace(
                self.root.path(),
                self.root.path(),
                current_dir,
                &mut self.database,
                None,
                paths,
                keep,
            )
        }

        fn capture_body(&self) -> String {
            self.database.connected_graph().expect("graph loads").captures[0]
                .body
                .clone()
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
    fn resolving_with_the_database_writes_its_version_to_the_workspace() {
        let mut project = Project::synced();
        let capture = project.diverge_capture();

        let outcome = project
            .resolve(std::slice::from_ref(&capture), Side::Database)
            .expect("resolve succeeds");
        assert_eq!(outcome.exported, std::slice::from_ref(&capture));
        assert!(project.read(&capture).contains("Edited in the database"));
        assert_eq!(project.capture_body(), "Edited in the database");
        assert!(project.sync().expect("next sync succeeds").is_empty());
    }

    #[test]
    fn resolving_with_the_workspace_imports_its_version() {
        let mut project = Project::synced();
        let capture = project.diverge_capture();

        let outcome = project
            .resolve(std::slice::from_ref(&capture), Side::Workspace)
            .expect("resolve succeeds");
        assert_eq!(outcome.imported, std::slice::from_ref(&capture));
        assert_eq!(project.capture_body(), "Edited in the workspace");
        assert!(project.read(&capture).contains("Edited in the workspace"));
        assert!(project.sync().expect("next sync succeeds").is_empty());
    }

    #[test]
    fn resolving_a_removed_file_with_the_database_writes_it_back() {
        let mut project = Project::synced();
        let capture = project.path_of("captures");
        fs::remove_file(project.root.path().join(&capture)).expect("record removes");

        let error = project
            .resolve(std::slice::from_ref(&capture), Side::Workspace)
            .expect_err("the workspace cannot win a removal");
        assert!(matches!(
            error,
            WorkspaceSyncError::Removed { side: Side::Workspace, .. }
        ));
        assert!(!project.root.path().join(&capture).exists());

        project
            .resolve(std::slice::from_ref(&capture), Side::Database)
            .expect("resolve succeeds");
        assert!(project.read(&capture).contains("Capture body"));
        assert!(project.sync().expect("next sync succeeds").is_empty());
    }

    #[test]
    fn resolve_refuses_a_path_with_nothing_to_resolve() {
        let mut project = Project::synced();
        let spec = project.path_of("specs");
        project.edit(&spec, "Spec body", "Edited in the workspace");
        let base = project.database.workspace_base().expect("base loads");

        let error = project
            .resolve(std::slice::from_ref(&spec), Side::Database)
            .expect_err("a one-sided change has nothing to resolve");
        assert!(matches!(&error, WorkspaceSyncError::NothingToResolve { path } if path == &spec));
        assert_eq!(error.exit_code(), 3);
        assert!(project.read(&spec).contains("Edited in the workspace"));
        assert_eq!(project.database.workspace_base().expect("base loads"), base);
    }

    #[test]
    fn resolve_accepts_a_path_with_a_leading_dot() {
        let mut project = Project::synced();
        let capture = project.diverge_capture();

        let outcome = project
            .resolve(&[Path::new(".").join(&capture)], Side::Database)
            .expect("resolve succeeds");
        assert_eq!(outcome.exported, std::slice::from_ref(&capture));
        assert!(project.read(&capture).contains("Edited in the database"));
    }

    #[test]
    fn named_paths_become_workspace_relative() {
        let worktree = Path::new("/project");
        let root = Path::new("/project/.arcl/workspace");
        let expected = Path::new("captures/x.md");
        for named in [
            "captures/x.md",
            "./captures/x.md",
            ".arcl/workspace/captures/x.md",
            "./.arcl/workspace/captures/x.md",
            "/project/.arcl/workspace/captures/x.md",
        ] {
            assert_eq!(
                workspace_relative(Path::new(named), worktree, root, worktree),
                expected,
                "{named}"
            );
        }
        assert_eq!(
            workspace_relative(Path::new("/elsewhere/captures/x.md"), worktree, root, worktree),
            Path::new("/elsewhere/captures/x.md")
        );
        assert_eq!(
            workspace_relative(Path::new("./captures/x.md"), worktree, worktree, worktree),
            expected
        );
    }

    #[test]
    fn named_paths_resolve_against_the_current_directory() {
        let worktree = tempfile::tempdir().expect("worktree creates");
        let root = worktree.path().join(".arcl/workspace");
        fs::create_dir_all(root.join("captures")).expect("workspace creates");
        fs::write(root.join("captures/x.md"), "x").expect("record writes");
        let subdirectory = worktree.path().join("src");
        fs::create_dir(&subdirectory).expect("subdirectory creates");
        fs::write(worktree.path().join("outside.md"), "o").expect("file writes");
        let expected = Path::new("captures/x.md");

        for (current, named) in [
            (subdirectory.as_path(), "../.arcl/workspace/captures/x.md"),
            (subdirectory.as_path(), ".arcl/workspace/captures/x.md"),
            (subdirectory.as_path(), "captures/x.md"),
            (root.join("captures").as_path(), "x.md"),
            (root.join("captures").as_path(), "../captures/x.md"),
            // Removed from disk: the parent directory still resolves.
            (subdirectory.as_path(), "../.arcl/workspace/captures/gone.md"),
        ] {
            let relative = workspace_relative(Path::new(named), current, &root, worktree.path());
            if named.ends_with("gone.md") {
                assert_eq!(relative, Path::new("captures/gone.md"), "{named}");
            } else {
                assert_eq!(relative, expected, "{named}");
            }
        }
        // A path that resolves outside the workspace matches no workspace file.
        let outside = workspace_relative(Path::new("../outside.md"), &root, &root, worktree.path());
        assert_eq!(outside, Path::new("../outside.md"));

        #[cfg(unix)]
        {
            let link = worktree.path().join("link.md");
            std::os::unix::fs::symlink(root.join("captures/x.md"), &link).expect("link creates");
            assert_eq!(
                workspace_relative(Path::new("link.md"), worktree.path(), &root, worktree.path()),
                expected
            );
        }
    }

    #[test]
    fn resolve_reads_a_path_from_a_workspace_subdirectory() {
        let mut project = Project::synced();
        let capture = project.diverge_capture();
        let captures = project.root.path().join("captures");
        let name = PathBuf::from(capture.file_name().expect("capture has a name"));

        let outcome = project
            .resolve_from(&captures, &[name], Side::Database)
            .expect("resolve succeeds");
        assert_eq!(outcome.exported, std::slice::from_ref(&capture));
        assert!(project.read(&capture).contains("Edited in the database"));
    }

    #[test]
    fn a_changed_manifest_is_rewritten_without_an_import() {
        let mut project = Project::synced();
        let manifest = PathBuf::from(MANIFEST);
        fs::write(project.root.path().join(MANIFEST), "# note\nformat-version = 1\n").expect("manifest writes");

        let error = project
            .resolve(std::slice::from_ref(&manifest), Side::Workspace)
            .expect_err("the manifest has nothing to resolve");
        assert!(matches!(&error, WorkspaceSyncError::NothingToResolve { path } if path == &manifest));
        assert!(project.read(&manifest).starts_with("# note"));

        let outcome = project.sync().expect("sync succeeds");
        assert!(outcome.imported.is_empty());
        assert_eq!(outcome.converged, std::slice::from_ref(&manifest));
        assert_eq!(project.read(&manifest), "format-version = 1\n");
        assert!(project.sync().expect("second sync succeeds").is_empty());
    }

    #[test]
    fn a_manifest_that_differs_on_the_first_sync_does_not_diverge() {
        let mut database = Database::open_in_memory().expect("database opens");
        database
            .create_capture("Capture".to_owned(), "Capture body".to_owned())
            .expect("capture creates");
        let root = tempfile::tempdir().expect("workspace creates");
        fs::write(root.path().join(MANIFEST), "# note\nformat-version = 1\n").expect("manifest writes");

        let outcome = sync_workspace(root.path(), root.path(), &mut database, None).expect("sync succeeds");
        assert!(outcome.imported.is_empty());
        assert_eq!(outcome.converged, [PathBuf::from(MANIFEST)]);
        assert_eq!(
            fs::read_to_string(root.path().join(MANIFEST)).expect("manifest reads"),
            "format-version = 1\n"
        );
    }

    #[test]
    fn a_removed_file_names_resolve_as_the_fix() {
        let error = WorkspaceSyncError::Removed { path: PathBuf::from("captures/x.md"), side: Side::Workspace };
        assert!(
            error
                .to_string()
                .ends_with("; `arcl workspace resolve --keep database captures/x.md` writes it back")
        );
    }

    #[test]
    fn other_divergent_paths_still_stop_a_resolve() {
        let mut project = Project::synced();
        let capture = project.diverge_capture();
        let spec = project.path_of("specs");
        project.edit(&spec, "Spec body", "Spec in the workspace");
        let id = project.database.connected_graph().expect("graph loads").specs[0].id;
        project
            .database
            .update_spec(
                id,
                arcl_store::SpecUpdate { body: Some("Spec in the database".to_owned()), ..Default::default() },
            )
            .expect("spec updates");

        let error = project
            .resolve(std::slice::from_ref(&capture), Side::Database)
            .expect_err("the spec is still divergent");
        assert!(
            matches!(&error, WorkspaceSyncError::Divergent { paths } if paths.as_slice() == std::slice::from_ref(&spec))
        );
        assert!(project.read(&capture).contains("Edited in the workspace"));
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
    fn unmerged_files_stop_before_anything_is_written() {
        let mut project = Project::synced();
        let capture = project.path_of("captures");
        project.edit(&capture, "Capture body", "Edited during a merge");
        let edited = project.read(&capture);
        let note = project
            .database
            .create_note("Note".to_owned(), "Not exported yet".to_owned())
            .expect("note creates");
        let base = project.database.workspace_base().expect("base loads");
        let vcs = FakeVcs::unmerged(&[&capture]);

        let error = project.sync_with(Some(&vcs)).expect_err("unmerged files refuse");
        assert!(
            matches!(&error, WorkspaceSyncError::Unmerged { paths } if paths.as_slice() == std::slice::from_ref(&capture)),
            "{error:?}"
        );
        assert_eq!(error.exit_code(), 4);
        assert!(error.to_string().contains("unmerged in Git"));
        assert_eq!(project.read(&capture), edited);
        assert!(!project.root.path().join(format!("notes/{}.md", note.id)).exists());
        assert_eq!(
            project.database.connected_graph().expect("graph loads").captures[0].body,
            "Capture body"
        );
        assert_eq!(project.database.workspace_base().expect("base loads"), base);
    }

    #[test]
    fn a_conflict_example_in_a_body_syncs_repeatedly() {
        let mut project = Project::synced();
        let capture = project.path_of("captures");
        let example = "```text\n<<<<<<< HEAD\nOurs\n=======\nTheirs\n>>>>>>> other\n```";
        project.edit(&capture, "Capture body", example);
        let vcs = FakeVcs { changed: vec![(capture.clone(), PathState::Modified)] };

        let outcome = project.sync_with(Some(&vcs)).expect("first sync succeeds");
        assert_eq!(outcome.imported, std::slice::from_ref(&capture));
        assert_eq!(
            project.database.connected_graph().expect("graph loads").captures[0].body,
            example
        );
        for _ in 0..2 {
            assert!(project.sync_with(Some(&vcs)).expect("later sync succeeds").is_empty());
        }
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
    fn a_synced_project_without_tasks_refuses_a_blocker_cycle_a_pull_adds() {
        let mut database = Database::open_in_memory().expect("database opens");
        database
            .create_capture("Capture".to_owned(), "Capture body".to_owned())
            .expect("capture creates");
        let root = tempfile::tempdir().expect("workspace creates");
        fs::write(root.path().join("manifest.toml"), "format-version = 1\n").expect("manifest writes");
        let mut project = Project { root, database };
        project.sync().expect("first sync succeeds");
        assert!(!project.database.workspace_base().expect("base loads").is_empty());

        // A pull brings tasks A and B, each blocked by the other.
        let mut other = Database::open_in_memory().expect("database opens");
        let project_id = other.project().expect("project exists").id;
        let create = |title: &str| arcl_store::PlanningTaskCreate {
            project_id,
            spec_id: None,
            plan_id: None,
            phase_id: None,
            parent_id: None,
            title: title.to_owned(),
            body: String::new(),
            priority: arcl_core::domain::TaskPriority::Normal,
            position: 0,
        };
        let a = other.create_planning_task(create("A")).expect("task creates").id;
        let b = other.create_planning_task(create("B")).expect("task creates").id;
        let mut graph = other.connected_graph().expect("graph loads");
        graph.dependencies = vec![
            arcl_core::domain::TaskDependency::new(a, b).expect("edge"),
            arcl_core::domain::TaskDependency::new(b, a).expect("edge"),
        ];
        fs::create_dir_all(project.root.path().join("tasks")).expect("tasks directory creates");
        for file in project_graph(&graph).expect("projection succeeds") {
            if file.path.starts_with("tasks") {
                fs::write(project.root.path().join(&file.path), &file.content).expect("task writes");
            }
        }

        let error = project.sync().expect_err("a new cycle refuses the sync");
        assert!(error.to_string().contains("would form a cycle"), "{error}");
        assert_eq!(error.exit_code(), 3);
        assert!(
            project
                .database
                .connected_graph()
                .expect("graph loads")
                .tasks
                .is_empty()
        );
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
        let before = project.database.connected_graph().expect("graph loads");
        let (graph, _) = validate_candidate(
            candidate.records,
            project.database.project().expect("project loads"),
            project.root.path(),
            Some(&before),
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
