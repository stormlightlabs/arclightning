use std::path::{Path, PathBuf};

use gix::status::index_worktree::iter::Summary;
use thiserror::Error;

/// Read-only VCS operations required by the application boundary.
pub trait Vcs {
    fn worktree_root(&self) -> Result<&Path, VcsError>;
    fn head_id(&self) -> Result<Option<String>, VcsError>;
    fn branch_name(&self) -> Result<Option<String>, VcsError>;
    fn path_state(&self, path: &Path) -> Result<PathState, VcsError>;
}

/// The state of a worktree path relevant to snapshot synchronization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathState {
    Tracked,
    Untracked,
    Modified,
    Deleted,
    Conflicted,
    Absent,
}

/// Git discovery and inspection failures.
#[derive(Debug, Error)]
pub enum VcsError {
    #[error("could not discover a Git worktree from `{path}`: {message}")]
    Discovery { path: PathBuf, message: String },
    #[error("Git repository `{path}` is bare; Arc Lightning requires a worktree")]
    BareRepository { path: PathBuf },
    #[error("discovered Git repository `{path}` has no worktree")]
    MissingWorktree { path: PathBuf },
    #[error("path `{path}` is outside the Git worktree `{root}`")]
    PathOutsideWorktree { path: PathBuf, root: PathBuf },
    #[error("Git {operation} failed: {message}")]
    Operation { operation: &'static str, message: String },
}

/// The gix-backed VCS implementation used by v1.
#[derive(Debug)]
pub struct GixVcs {
    repository: gix::Repository,
    root: PathBuf,
}

impl GixVcs {
    pub fn discover(start: impl AsRef<Path>) -> Result<Self, VcsError> {
        let start = start.as_ref();
        let repository = gix::discover(start)
            .map_err(|error| VcsError::Discovery { path: start.to_owned(), message: error.to_string() })?;
        if repository.is_bare() {
            return Err(VcsError::BareRepository { path: repository.git_dir().to_owned() });
        }
        let root = repository
            .workdir()
            .ok_or_else(|| VcsError::MissingWorktree { path: repository.git_dir().to_owned() })?
            .to_owned();
        Ok(Self { repository, root })
    }
}

impl Vcs for GixVcs {
    fn worktree_root(&self) -> Result<&Path, VcsError> {
        Ok(&self.root)
    }

    fn head_id(&self) -> Result<Option<String>, VcsError> {
        let head = self
            .repository
            .head()
            .map_err(|error| VcsError::Operation { operation: "read HEAD", message: error.to_string() })?;
        Ok(head.id().map(|id| id.to_string()))
    }

    fn branch_name(&self) -> Result<Option<String>, VcsError> {
        let head = self
            .repository
            .head()
            .map_err(|error| VcsError::Operation { operation: "read branch", message: error.to_string() })?;
        Ok(head.referent_name().map(|name| name.shorten().to_string()))
    }

    fn path_state(&self, path: &Path) -> Result<PathState, VcsError> {
        let candidate = if path.is_absolute() { path.to_owned() } else { self.root.join(path) };
        let relative = candidate
            .strip_prefix(&self.root)
            .map_err(|_| VcsError::PathOutsideWorktree { path: candidate.clone(), root: self.root.clone() })?;
        let relative = relative.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/");

        let mut statuses = self
            .repository
            .status(gix::progress::Discard)
            .map_err(|error| VcsError::Operation { operation: "create status iterator", message: error.to_string() })?
            .untracked_files(gix::status::UntrackedFiles::Files)
            .into_index_worktree_iter(Vec::new())
            .map_err(|error| VcsError::Operation {
                operation: "inspect worktree status",
                message: error.to_string(),
            })?;

        for item in &mut statuses {
            let item = item.map_err(|error| VcsError::Operation {
                operation: "read worktree status",
                message: error.to_string(),
            })?;
            if item.rela_path() == relative.as_bytes() {
                return Ok(match item.summary() {
                    Some(Summary::Added) => PathState::Untracked,
                    Some(Summary::Removed) => PathState::Deleted,
                    Some(Summary::Conflict) => PathState::Conflicted,
                    Some(_) => PathState::Modified,
                    None => PathState::Tracked,
                });
            }
        }

        if candidate.exists() { Ok(PathState::Tracked) } else { Ok(PathState::Absent) }
    }
}
