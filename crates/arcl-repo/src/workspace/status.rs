//! Read-only report of how the database and the workspace stand against the
//! last sync, with Git state where the project has a Git worktree.
//!
//! Status reads workspace files as raw bytes, so a file with conflict markers
//! or invalid front matter is reported instead of stopping the command.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::Serialize;
use thiserror::Error;

use arcl_store::{Database, StorageError};

use super::{
    Change, RecordState, WorkspaceExportError, WorkspaceFile, compare, export::format_floor, has_conflict_markers,
    import::RECORD_DIRECTORIES, project_graph_at,
};
use crate::vcs::{HeadState, PathState, Vcs, VcsError};

/// How a path differs between the database, the workspace and the last sync.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecordStatusKind {
    /// Only the database changed; sync exports it.
    DatabaseChanged,
    /// Only the workspace changed; sync imports it.
    WorkspaceChanged,
    /// Both sides made the same change; sync only records it.
    Converged,
    /// Both sides changed it differently; sync stops on it.
    Divergent,
}

/// A status failure. Status writes nothing.
#[derive(Debug, Error)]
pub enum WorkspaceStatusError {
    /// A workspace directory or file could not be read.
    #[error("could not read workspace path `{path}`: {source}")]
    Read { path: PathBuf, source: io::Error },
    /// The database projection could not be rendered.
    #[error(transparent)]
    Export(#[from] WorkspaceExportError),
    /// The database could not be read.
    #[error(transparent)]
    Storage(#[from] StorageError),
    /// Git state could not be read.
    #[error(transparent)]
    Vcs(#[from] VcsError),
}

/// One path that differs from the last sync.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RecordStatus {
    /// The path relative to the workspace root.
    pub path: PathBuf,
    /// How the two sides differ from the last sync.
    pub state: RecordStatusKind,
    /// The database's change, when it changed.
    pub database: Option<Change>,
    /// The workspace's change, when it changed.
    pub workspace: Option<Change>,
    /// The path's Git state, when the project has a Git worktree and Git
    /// reports a change.
    pub git: Option<PathState>,
}

/// A workspace path Git reports as changed although it matches the last sync,
/// such as a file sync wrote that is not yet staged.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GitChange {
    /// The path relative to the workspace root.
    pub path: PathBuf,
    /// The path's Git state.
    pub git: PathState,
}

/// Everything status reports about one workspace.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct WorkspaceStatus {
    /// Where HEAD points, when the project has a Git worktree.
    pub head: Option<HeadState>,
    /// Paths that differ from the last sync, sorted by path.
    pub records: Vec<RecordStatus>,
    /// Paths that match the last sync but differ from the Git index, sorted
    /// by path. Unmerged paths are listed under `unmerged` instead.
    pub git_changes: Vec<GitChange>,
    /// Workspace paths Git reports as unmerged, which sync and import refuse.
    pub unmerged: Vec<PathBuf>,
    /// Workspace files with lines that look like merge conflict markers.
    /// This is a warning: sync accepts them as content, since a record body
    /// can show a conflict as an example.
    pub conflict_markers: Vec<PathBuf>,
    /// Entries outside the workspace layout, which sync and import refuse.
    pub unexpected: Vec<PathBuf>,
}

impl WorkspaceStatus {
    /// Return whether the database and the workspace match the last sync and
    /// Git: sync has nothing to do, nothing blocks it and Git reports no
    /// workspace changes. The `conflict_markers` warning does not count.
    pub fn is_clean(&self) -> bool {
        self.records.is_empty() && self.git_changes.is_empty() && self.unmerged.is_empty() && self.unexpected.is_empty()
    }

    /// Return whether `arcl workspace sync` would stop without writing.
    /// Conflict marker lines do not count; only Git's unmerged state does.
    pub fn sync_refuses(&self) -> bool {
        !self.unmerged.is_empty()
            || !self.unexpected.is_empty()
            || self.records.iter().any(|record| {
                record.state == RecordStatusKind::Divergent
                    || record.workspace == Some(Change::Removed)
                    || record.database == Some(Change::Removed)
            })
    }
}

/// Report how the database and the workspace at `root` stand against the
/// last sync. Pass `vcs` when the project has a Git worktree.
pub fn workspace_status(
    root: &Path, database: &Database, vcs: Option<&dyn Vcs>,
) -> Result<WorkspaceStatus, WorkspaceStatusError> {
    let base = database.workspace_base()?;
    let (workspace_files, unexpected) = read_workspace_bytes(root)?;
    let database_files = project_graph_at(&database.connected_graph()?, format_floor(&base, &workspace_files))?;

    let (head, git_paths) = match vcs {
        Some(vcs) => (Some(vcs.head_state()?), vcs.changed_paths(root)?),
        None => (None, Vec::new()),
    };
    let git_state = |path: &Path| {
        git_paths
            .iter()
            .find(|(candidate, _)| candidate == path)
            .map(|(_, state)| *state)
    };

    let records: Vec<RecordStatus> = compare(&base, &database_files, &workspace_files)
        .records
        .into_iter()
        .filter_map(|record| {
            let (state, database, workspace) = match record.state {
                RecordState::Unchanged => return None,
                RecordState::DatabaseChanged(change) => (RecordStatusKind::DatabaseChanged, Some(change), None),
                RecordState::WorkspaceChanged(change) => (RecordStatusKind::WorkspaceChanged, None, Some(change)),
                RecordState::Converged(change) => (RecordStatusKind::Converged, Some(change), Some(change)),
                RecordState::Divergent { database, workspace } => {
                    (RecordStatusKind::Divergent, Some(database), Some(workspace))
                }
            };
            let git = git_state(&record.path);
            Some(RecordStatus { path: record.path, state, database, workspace, git })
        })
        .collect();

    let mut git_changes: Vec<GitChange> = git_paths
        .iter()
        .filter(|(path, state)| *state != PathState::Conflicted && !records.iter().any(|record| &record.path == path))
        .map(|(path, state)| GitChange { path: path.clone(), git: *state })
        .collect();
    git_changes.sort_by(|left, right| left.path.cmp(&right.path));
    let unmerged = git_paths
        .iter()
        .filter(|(_, state)| *state == PathState::Conflicted)
        .map(|(path, _)| path.clone())
        .collect();
    let conflict_markers = workspace_files
        .iter()
        .filter(|file| has_conflict_markers(&file.content))
        .map(|file| file.path.clone())
        .collect();

    Ok(WorkspaceStatus { head, records, git_changes, unmerged, conflict_markers, unexpected })
}

/// Read the manifest and every record file under `root` as raw bytes, and
/// list entries outside the workspace layout instead of failing on them.
fn read_workspace_bytes(root: &Path) -> Result<(Vec<WorkspaceFile>, Vec<PathBuf>), WorkspaceStatusError> {
    let mut files = Vec::new();
    let mut unexpected = Vec::new();
    for entry in read_dir(root)? {
        let name = entry.file_name();
        let relative = PathBuf::from(&name);
        let path = root.join(&name);
        let is_dir = path.is_dir() && !is_symlink(&path);
        match name.to_str() {
            Some("manifest.toml") if path.is_file() && !is_symlink(&path) => {
                files.push(WorkspaceFile { path: relative, content: read(&path)? })
            }
            Some(directory) if is_dir && RECORD_DIRECTORIES.contains(&directory) => {
                for record in read_dir(&path)? {
                    let record_name = record.file_name();
                    let record_relative = relative.join(&record_name);
                    let record_path = path.join(&record_name);
                    let is_record = record_path.is_file()
                        && !is_symlink(&record_path)
                        && record_name.to_str().is_some_and(|name| name.ends_with(".md"));
                    if is_record {
                        files.push(WorkspaceFile { path: record_relative, content: read(&record_path)? });
                    } else {
                        unexpected.push(record_relative);
                    }
                }
            }
            _ => unexpected.push(relative),
        }
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    unexpected.sort();
    Ok((files, unexpected))
}

fn read_dir(path: &Path) -> Result<Vec<fs::DirEntry>, WorkspaceStatusError> {
    match fs::read_dir(path) {
        Ok(entries) => entries
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| WorkspaceStatusError::Read { path: path.to_owned(), source }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(source) => Err(WorkspaceStatusError::Read { path: path.to_owned(), source }),
    }
}

fn read(path: &Path) -> Result<Vec<u8>, WorkspaceStatusError> {
    fs::read(path).map_err(|source| WorkspaceStatusError::Read { path: path.to_owned(), source })
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vcs::FakeVcs;
    use crate::workspace::sync_workspace;

    fn synced() -> (tempfile::TempDir, Database) {
        let mut database = Database::open_in_memory().expect("database opens");
        database
            .create_capture("Capture".to_owned(), "Capture body".to_owned())
            .expect("capture creates");
        let root = tempfile::tempdir().expect("workspace creates");
        fs::write(root.path().join("manifest.toml"), "format-version = 1\n").expect("manifest writes");
        sync_workspace(root.path(), root.path(), &mut database, None).expect("sync succeeds");
        (root, database)
    }

    fn capture_path(root: &Path) -> PathBuf {
        let entry = fs::read_dir(root.join("captures"))
            .expect("captures read")
            .next()
            .expect("one capture")
            .expect("entry reads");
        PathBuf::from("captures").join(entry.file_name())
    }

    #[test]
    fn a_synced_workspace_is_clean() {
        let (root, database) = synced();
        let status = workspace_status(root.path(), &database, None).expect("status reads");
        assert!(status.is_clean());
        assert!(!status.sync_refuses());
        assert_eq!(status.head, None);
    }

    #[test]
    fn one_sided_changes_are_listed_with_their_side() {
        let (root, mut database) = synced();
        let capture = capture_path(root.path());
        let content = fs::read_to_string(root.path().join(&capture)).expect("capture reads");
        fs::write(root.path().join(&capture), content.replace("Capture body", "Edited")).expect("capture writes");
        let note = database
            .create_note("Note".to_owned(), "Body".to_owned())
            .expect("note creates");

        let status = workspace_status(root.path(), &database, None).expect("status reads");
        assert_eq!(status.records.len(), 2);
        assert_eq!(status.records[0].path, capture);
        assert_eq!(status.records[0].state, RecordStatusKind::WorkspaceChanged);
        assert_eq!(status.records[0].workspace, Some(Change::Modified));
        assert_eq!(status.records[1].path, PathBuf::from(format!("notes/{}.md", note.id)));
        assert_eq!(status.records[1].state, RecordStatusKind::DatabaseChanged);
        assert_eq!(status.records[1].database, Some(Change::Added));
        assert!(!status.sync_refuses());
    }

    #[test]
    fn conflict_markers_warn_and_unexpected_entries_refuse() {
        let (root, database) = synced();
        let capture = capture_path(root.path());
        fs::write(
            root.path().join(&capture),
            "<<<<<<< ours\n+++\n=======\n+++\n>>>>>>> theirs\n",
        )
        .expect("conflicted capture writes");
        fs::write(root.path().join("notes.txt"), "stray").expect("stray file writes");

        let status = workspace_status(root.path(), &database, None).expect("status reads");
        assert_eq!(status.conflict_markers, std::slice::from_ref(&capture));
        assert_eq!(status.unexpected, [PathBuf::from("notes.txt")]);
        assert_eq!(status.records[0].path, capture);
        assert!(status.sync_refuses());

        fs::remove_file(root.path().join("notes.txt")).expect("stray file removes");
        let status = workspace_status(root.path(), &database, None).expect("status reads");
        assert_eq!(status.conflict_markers, std::slice::from_ref(&capture));
        assert!(!status.sync_refuses());
    }

    #[test]
    fn a_synced_conflict_example_is_clean_with_a_warning() {
        let (root, mut database) = synced();
        let capture = capture_path(root.path());
        let content = fs::read_to_string(root.path().join(&capture)).expect("capture reads");
        let example = "```text\n<<<<<<< HEAD\nOurs\n=======\nTheirs\n>>>>>>> other\n```";
        fs::write(root.path().join(&capture), content.replace("Capture body", example)).expect("capture writes");
        sync_workspace(root.path(), root.path(), &mut database, None).expect("sync succeeds");

        let status = workspace_status(root.path(), &database, None).expect("status reads");
        assert_eq!(status.conflict_markers, std::slice::from_ref(&capture));
        assert!(status.is_clean());
        assert!(!status.sync_refuses());
    }

    #[test]
    fn a_removed_record_blocks_sync() {
        let (root, database) = synced();
        let capture = capture_path(root.path());
        fs::remove_file(root.path().join(&capture)).expect("capture removes");

        let status = workspace_status(root.path(), &database, None).expect("status reads");
        assert_eq!(status.records[0].workspace, Some(Change::Removed));
        assert!(status.sync_refuses());
    }

    #[test]
    fn git_state_head_and_unmerged_paths_come_from_vcs() {
        let vcs = FakeVcs {
            changed: vec![
                (PathBuf::from("notes/a.md"), PathState::Conflicted),
                (PathBuf::from("manifest.toml"), PathState::Untracked),
            ],
        };

        let (root, database) = synced();
        let status = workspace_status(root.path(), &database, Some(&vcs)).expect("status reads");
        assert_eq!(status.head, Some(HeadState::Unborn { branch: "main".to_owned() }));
        assert_eq!(status.unmerged, [PathBuf::from("notes/a.md")]);
        assert_eq!(
            status.git_changes,
            [GitChange { path: PathBuf::from("manifest.toml"), git: PathState::Untracked }]
        );
        assert!(!status.is_clean());
        assert!(status.sync_refuses());
    }
}
