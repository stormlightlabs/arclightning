use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

mod codec;
pub mod edit;
pub mod export;
pub mod import;
pub mod reconcile;
pub mod status;
pub mod sync;

pub use codec::*;
pub use edit::{
    RecordEdit, RecordEditError, RecordSource, edit_record, record_source, record_sources, source_revision,
};
pub use export::{
    WorkspaceExportError, WorkspaceFile, export_graph, export_graph_with_base, project_graph, project_graph_at,
    required_format_version,
};
pub use import::{ImportOutcome, WorkspaceImportError, import_graph, import_workspace};
pub use reconcile::{Change, Comparison, RecordComparison, RecordState, compare};
pub use status::{GitChange, RecordStatus, RecordStatusKind, WorkspaceStatus, WorkspaceStatusError, workspace_status};
pub use sync::{Side, SyncOutcome, WorkspaceSyncError, resolve_workspace, sync_workspace, workspace_relative};

pub const CONFIG_FORMAT_VERSION: u32 = 1;
/// The newest on-disk workspace format version this crate reads and writes.
///
/// Version 2 adds the task `assignee` field, version 3 the task `notes`
/// array, version 4 the task `gates` array and version 5 the capture
/// `source-task` field; see [`ASSIGNEE_FORMAT_VERSION`],
/// [`TASK_NOTES_FORMAT_VERSION`], [`GATES_FORMAT_VERSION`] and
/// [`CAPTURE_SOURCE_FORMAT_VERSION`] for when a projection writes each. Reading also
/// accepts [`OLDEST_WORKSPACE_FORMAT_VERSION`] and later versions up to this
/// one; an arcl that predates a version refuses its manifest before it reads
/// any record.
pub const WORKSPACE_FORMAT_VERSION: u32 = 5;
/// The oldest workspace format version this crate still reads.
pub const OLDEST_WORKSPACE_FORMAT_VERSION: u32 = 1;
/// The workspace format version a task `assignee` needs.
///
/// A projection writes this version once a task carries an assignee, and
/// keeps a manifest at a higher version than its records need, so a workspace
/// is never moved back to an older version. Otherwise it keeps
/// [`OLDEST_WORKSPACE_FORMAT_VERSION`], which an older arcl still reads.
pub const ASSIGNEE_FORMAT_VERSION: u32 = 2;
/// The workspace format version task `notes` need.
///
/// A projection writes this version once a task carries a note, under the
/// same rule as [`ASSIGNEE_FORMAT_VERSION`]: it never lowers the version a
/// workspace already has.
pub const TASK_NOTES_FORMAT_VERSION: u32 = 3;
/// The workspace format version task `gates` need.
///
/// A projection writes this version once a task carries a gate, open or
/// met, under the same rule as [`ASSIGNEE_FORMAT_VERSION`]: it never lowers
/// the version a workspace already has.
pub const GATES_FORMAT_VERSION: u32 = 4;
/// The workspace format version a capture `source-task` needs.
///
/// A projection writes this version once a capture names the task it came
/// from, under the same rule as [`ASSIGNEE_FORMAT_VERSION`]: it never lowers
/// the version a workspace already has.
pub const CAPTURE_SOURCE_FORMAT_VERSION: u32 = 5;

/// Workspace and configuration validation failures.
#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error("configuration TOML is invalid: {0}")]
    Parse(#[from] toml::de::Error),
    /// The configuration still uses the `[snapshot]` table that `[workspace]` replaced.
    #[error("the `[snapshot]` table is now `[workspace]`; rename it in `.arcl/config.toml`")]
    LegacySnapshotTable,
    #[error("TOML could not be rendered: {0}")]
    Render(#[from] toml::ser::Error),
    #[error("unsupported configuration format version {found}; expected {expected}")]
    UnsupportedVersion { found: u32, expected: u32 },
    #[error("workspace path `{path}` must be a non-empty worktree-relative path")]
    InvalidPath { path: PathBuf },
    #[error("workspace path component `{path}` must not be a symbolic link")]
    SymlinkPath { path: PathBuf },
    #[error("workspace path component `{path}` could not be inspected: {source}")]
    InspectPath { path: PathBuf, source: io::Error },
    #[error("workspace manifest TOML is invalid: {0}")]
    ManifestParse(#[source] toml::de::Error),
    #[error("workspace front matter TOML is invalid: {0}")]
    FrontMatterParse(#[source] toml::de::Error),
    #[error("workspace record format is invalid: {0}")]
    InvalidRecordFormat(String),
    #[error("workspace record path `{path}` is invalid")]
    InvalidRecordPath { path: PathBuf },
    #[error("workspace record ID `{id}` does not match filename `{filename}`")]
    FilenameIdMismatch { id: String, filename: String },
    #[error("workspace record kind `{kind}` does not match path `{path}`")]
    KindPathMismatch { kind: String, path: PathBuf },
    #[error("workspace record title cannot be empty")]
    EmptyRecordTitle,
    #[error("workspace record spec path cannot be empty")]
    EmptyRecordSpecPath,
    #[error("workspace record position {position} is invalid; positions must be non-negative")]
    InvalidRecordPosition { position: i64 },
    /// A task `assignee` that is empty, longer than 64 characters, or holds a
    /// control or format character.
    #[error(
        "workspace task assignee `{}` must be a non-empty name of at most 64 characters without control or format characters",
        value.escape_debug()
    )]
    InvalidRecordAssignee { value: String },
    /// A task `assignee` on a task that is not in progress.
    #[error("workspace task assignee is set on a `{status}` task; only an in-progress task holds a claim")]
    AssigneeNotInProgress { status: String },
    /// A task note whose text is empty, longer than 2000 characters, or
    /// holds a control character other than a newline or tab, a format
    /// character, or a line that is exactly `+++`.
    #[error(
        "workspace task note text `{}` must be non-empty, at most 2000 characters, and without control characters \
         other than newline and tab, format characters, or a line that is exactly `+++`",
        value.escape_debug()
    )]
    InvalidRecordNote { value: String },
    /// A task note with an empty `created-at`.
    #[error("workspace task note creation time cannot be empty")]
    EmptyRecordNoteCreatedAt,
    /// A task note whose `created-at` is not an RFC 3339 timestamp.
    #[error(
        "workspace task note creation time `{}` must be an RFC 3339 timestamp, such as `2026-10-27T10:00:00.000Z`",
        value.escape_debug()
    )]
    InvalidRecordNoteCreatedAt { value: String },
    #[error("workspace capture creation time cannot be empty")]
    EmptyRecordCreatedAt,
    /// A task gate with an invalid target, an empty `created-at`, or a
    /// `met-at` that does not match its status.
    #[error("workspace task gate `{id}` is invalid: {reason}")]
    InvalidRecordGate { id: String, reason: String },
    #[error("workspace record relationship is invalid: {0}")]
    InvalidRecordRelationship(String),
}

type Result<T> = std::result::Result<T, WorkspaceError>;

/// Project configuration persisted in `.arcl/config.toml`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    #[serde(rename = "format-version")]
    pub format_version: u32,
    pub workspace: WorkspaceConfig,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self { format_version: CONFIG_FORMAT_VERSION, workspace: WorkspaceConfig::default() }
    }
}

impl ProjectConfig {
    /// Parse and validate configuration TOML.
    ///
    /// A config that still has a `[snapshot]` table fails with
    /// [`WorkspaceError::LegacySnapshotTable`] so the user knows to rename it.
    pub fn parse(input: &str) -> Result<Self> {
        let config = toml::from_str::<Self>(input).map_err(|error| {
            let legacy = toml::from_str::<toml::Table>(input).is_ok_and(|table| table.contains_key("snapshot"));
            if legacy { WorkspaceError::LegacySnapshotTable } else { error.into() }
        })?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.format_version != CONFIG_FORMAT_VERSION {
            return Err(WorkspaceError::UnsupportedVersion {
                found: self.format_version,
                expected: CONFIG_FORMAT_VERSION,
            });
        }
        validate_relative_path(&self.workspace.path)
    }

    /// Render the canonical configuration representation.
    pub fn render(&self) -> Result<String> {
        toml::to_string_pretty(self).map_err(|e| e.into())
    }
}

/// Workspace settings in project configuration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceConfig {
    pub enabled: bool,
    pub path: PathBuf,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self { enabled: false, path: PathBuf::from(".arcl/workspace") }
    }
}

fn validate_relative_path(path: &Path) -> Result<()> {
    let invalid = path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir));
    if invalid { Err(WorkspaceError::InvalidPath { path: path.to_owned() }) } else { Ok(()) }
}

/// Resolve a configured workspace path while rejecting symbolic-link components.
pub fn resolve_workspace_root(worktree_root: &Path, relative: &Path) -> Result<PathBuf> {
    validate_relative_path(relative)?;

    let mut resolved = worktree_root.to_owned();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            continue;
        };
        resolved.push(component);
        match fs::symlink_metadata(&resolved) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(WorkspaceError::SymlinkPath { path: resolved });
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(source) => return Err(WorkspaceError::InspectPath { path: resolved, source }),
        }
    }
    Ok(resolved)
}

/// The workspace directory of a project without a database, when `config`
/// enables one and its `manifest.toml` exists. Running `arcl init` there
/// lets the next command import it.
pub fn importable_workspace(worktree_root: &Path, config: &ProjectConfig) -> Option<PathBuf> {
    if !config.workspace.enabled {
        return None;
    }
    let workspace_root = resolve_workspace_root(worktree_root, &config.workspace.path).ok()?;
    workspace_root.join("manifest.toml").is_file().then_some(workspace_root)
}

/// The message for a project without a database, shared by the CLI and the
/// MCP server. With `workspace`, from [`importable_workspace`], it says the
/// command after `arcl init` imports it.
pub fn not_initialized_message(root: &Path, workspace: Option<&Path>) -> String {
    match workspace {
        Some(workspace) => format!(
            "Arc Lightning has no database in project directory `{}`; run `arcl init`, and the next command imports \
             the workspace in `{}`",
            root.display(),
            workspace.display()
        ),
        None => format!(
            "Arc Lightning is not initialized in project directory `{}`; run `arcl init` first",
            root.display()
        ),
    }
}

/// Return whether `content` holds a line that looks like a Git merge conflict
/// marker: seven or more `<` or `>` followed by a space or the end of the line.
///
/// Status reports such files as a warning only. A record body can show a
/// conflict as an example, so sync and import read Git's unmerged state
/// instead of matching text.
pub(crate) fn has_conflict_markers(content: &[u8]) -> bool {
    String::from_utf8_lossy(content).lines().any(|line| {
        let Some(marker) = line.chars().next().filter(|first| matches!(first, '<' | '>')) else {
            return false;
        };
        let rest = line.trim_start_matches(marker);
        line.len() - rest.len() >= 7 && (rest.is_empty() || rest.starts_with(' '))
    })
}

/// Render paths as a comma-separated list of backquoted names for messages.
pub(crate) fn list_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| format!("`{}`", path.display()))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        assert!(ProjectConfig::default().validate().is_ok());
    }

    #[test]
    fn config_rejects_absolute_workspace_paths() {
        let input = "format-version = 1\n[workspace]\nenabled = true\npath = \"/tmp/workspace\"\n";
        assert!(matches!(
            ProjectConfig::parse(input),
            Err(WorkspaceError::InvalidPath { .. })
        ));
    }

    #[test]
    fn config_names_the_snapshot_table_rename() {
        let input = "format-version = 1\n[snapshot]\nenabled = true\npath = \".arcl/snapshot\"\n";
        let error = ProjectConfig::parse(input).unwrap_err();
        assert!(matches!(error, WorkspaceError::LegacySnapshotTable));
        assert!(error.to_string().contains("rename it"));
    }

    #[cfg(unix)]
    #[test]
    fn workspace_root_rejects_symlink_components() {
        use std::os::unix::fs::symlink;

        let worktree = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), worktree.path().join("linked")).unwrap();

        assert!(matches!(
            resolve_workspace_root(worktree.path(), Path::new("linked/workspace")),
            Err(WorkspaceError::SymlinkPath { .. })
        ));
    }

    #[test]
    fn conflict_marker_lines_need_seven_or_more_and_a_space_or_line_end() {
        assert!(has_conflict_markers(b"<<<<<<< HEAD\n"));
        assert!(has_conflict_markers(b"body\n>>>>>>>\n"));
        assert!(has_conflict_markers(b"<<<<<<<<<< ours\n"));
        assert!(has_conflict_markers(b">>>>>>>>>>\n"));
        assert!(!has_conflict_markers(b"<<<<<< six\n"));
        assert!(!has_conflict_markers(b"<<<<<<<<x\n"));
        assert!(!has_conflict_markers(b"text <<<<<<< inline\n"));
    }
}
