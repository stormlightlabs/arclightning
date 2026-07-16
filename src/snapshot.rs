use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CONFIG_FORMAT_VERSION: u32 = 1;

/// Snapshot and configuration validation failures.
#[derive(Debug, Error)]
pub enum SnapshotError {
    #[error("configuration TOML is invalid: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("configuration TOML could not be rendered: {0}")]
    Render(#[from] toml::ser::Error),
    #[error("unsupported configuration format version {found}; expected {expected}")]
    UnsupportedVersion { found: u32, expected: u32 },
    #[error("snapshot path `{path}` must be a non-empty worktree-relative path")]
    InvalidPath { path: PathBuf },
}

type Result<T> = std::result::Result<T, SnapshotError>;

/// Project configuration persisted in `.arcl/config.toml`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    #[serde(rename = "format-version")]
    pub format_version: u32,
    pub snapshot: SnapshotConfig,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self { format_version: CONFIG_FORMAT_VERSION, snapshot: SnapshotConfig::default() }
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
            return Err(SnapshotError::UnsupportedVersion {
                found: self.format_version,
                expected: CONFIG_FORMAT_VERSION,
            });
        }
        validate_relative_path(&self.snapshot.path)
    }

    /// Render the canonical v1 configuration representation.
    pub fn render(&self) -> Result<String> {
        toml::to_string_pretty(self).map_err(|e| e.into())
    }
}

/// Snapshot settings in project configuration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotConfig {
    pub enabled: bool,
    pub path: PathBuf,
}

impl Default for SnapshotConfig {
    fn default() -> Self {
        Self { enabled: false, path: PathBuf::from(".arcl/snapshot") }
    }
}

fn validate_relative_path(path: &Path) -> Result<()> {
    let invalid = path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir));
    if invalid { Err(SnapshotError::InvalidPath { path: path.to_owned() }) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        assert!(ProjectConfig::default().validate().is_ok());
    }

    #[test]
    fn config_rejects_absolute_snapshot_paths() {
        let input = "format-version = 1\n[snapshot]\nenabled = true\npath = \"/tmp/snapshot\"\n";
        assert!(matches!(
            ProjectConfig::parse(input),
            Err(SnapshotError::InvalidPath { .. })
        ));
    }
}
