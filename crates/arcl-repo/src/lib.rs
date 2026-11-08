//! Repository files, workspace interchange, optional Git inspection, project
//! discovery and workspace sync around an operation, and gate checks through
//! the GitHub CLI.

pub mod gate;
pub mod project;
pub mod vcs;
pub mod workspace;

/// The project items the CLI and the MCP server share: finding a project,
/// opening its database, the sync lock, the sync around an operation and its
/// warnings.
///
/// `use arcl_repo::prelude::*;` brings them in. Workspace interchange stays
/// in [`workspace`] and Git inspection in [`vcs`].
pub mod prelude {
    pub use crate::project::{
        ARCL_DIRECTORY, CONFIG_FILE, DATABASE_FILE, LockWait, LockedWorkspace, ProjectDir, ProjectError, SyncBefore,
        SyncLock, Warning, Workspace, as_dyn, discover_vcs, nearest_project_root, sync_before,
    };
}
