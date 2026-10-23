use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

mod codec;
pub mod export;
pub mod import;

pub use codec::*;
pub use export::{WorkspaceExportError, WorkspaceFile, export_graph, export_graph_with_base, project_graph};
pub use import::{WorkspaceImportError, import_graph, import_workspace};

pub const CONFIG_FORMAT_VERSION: u32 = 1;
/// The version of the on-disk workspace format implemented by this crate.
pub const WORKSPACE_FORMAT_VERSION: u32 = 1;

/// Workspace and configuration validation failures.
#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error("configuration TOML is invalid: {0}")]
    Parse(#[from] toml::de::Error),
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
    #[error("workspace capture creation time cannot be empty")]
    EmptyRecordCreatedAt,
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
    pub fn parse(input: &str) -> Result<Self> {
        let config = toml::from_str::<Self>(input)?;
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
}
