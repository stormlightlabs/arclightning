//! Automatic workspace sync around ordinary commands.
//!
//! When the workspace is enabled, reads and writes sync first so they see
//! changes made on either side. A write that cannot sync refuses before it
//! changes anything; a read warns and shows the database as it is. After a
//! write commits, a second sync exports it. If that export fails, the change
//! is already saved and the next command finishes the export.
//!
//! Every sync runs under the project's sync lock, so arcl commands in one
//! project with the workspace on run one at a time.

use super::support::*;
use super::workspace::{as_dyn, discover_vcs};
use super::*;

/// The lock file, under `.arcl/`, that serializes syncs in one project. The
/// `/arcl.db-*` entry in `.arcl/.gitignore` keeps it out of Git.
const SYNC_LOCK_FILE: &str = "arcl.db-sync.lock";

/// How a command touches project records.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Access {
    /// The command does not read or write records, or manages the workspace
    /// itself.
    Untracked,
    /// The command only reads records.
    Read,
    /// The command may change records.
    Write,
}

/// The project root and workspace root of a project with the workspace on.
pub(super) struct WorkspaceRoots {
    worktree_root: PathBuf,
    workspace_root: PathBuf,
}

impl WorkspaceRoots {
    /// Take the project's sync lock. See [`lock_project`].
    pub(super) fn lock(&self) -> CResult<File> {
        lock_project(&self.worktree_root)
    }
}

/// Classify a parsed command by how it touches records.
pub(super) fn command_access(command: Option<&Command>) -> Access {
    let read_when = |read: bool| if read { Access::Read } else { Access::Write };
    match command {
        None | Some(Command::Init { .. } | Command::Workspace { .. } | Command::Skill { .. }) => Access::Untracked,
        Some(
            Command::Show { .. }
            | Command::List { .. }
            | Command::Tree { .. }
            | Command::Explain { .. }
            | Command::Context { .. }
            | Command::Check
            | Command::Ready { .. }
            | Command::Next { .. },
        ) => Access::Read,
        Some(Command::Capture { command }) => {
            read_when(matches!(command, CaptureCommand::Show { .. } | CaptureCommand::List))
        }
        Some(Command::Release { command }) => read_when(match command {
            ReleaseCommand::Show { .. } | ReleaseCommand::List => true,
            ReleaseCommand::Member { command } => matches!(command, MembershipCommand::List { .. }),
            _ => false,
        }),
        Some(Command::Spec { command }) => read_when(matches!(command, SpecCommand::Show { .. } | SpecCommand::List)),
        Some(Command::Plan { command }) => read_when(match command {
            PlanCommand::Show { .. } | PlanCommand::List | PlanCommand::Check { .. } | PlanCommand::Diff { .. } => true,
            PlanCommand::Phase { command } => matches!(command, PhaseCommand::Show { .. } | PhaseCommand::List),
            _ => false,
        }),
        Some(Command::Task { command }) => read_when(matches!(
            command,
            TaskCommand::Show { .. } | TaskCommand::List | TaskCommand::Explain { .. } | TaskCommand::Context { .. }
        )),
        Some(Command::Dependency { command }) => read_when(matches!(command, DependencyCommand::List)),
        Some(Command::Note { command }) => read_when(match command {
            NoteCommand::Show { .. } | NoteCommand::List => true,
            NoteCommand::Link { command } => matches!(command, NoteLinkCommand::List { .. }),
            _ => false,
        }),
    }
}

/// Return the roots to sync, or `None` when there is no project or its
/// workspace is off. Any other configuration error, such as a symlinked
/// workspace, is returned so the caller treats it as a failed sync.
pub(super) fn workspace_roots() -> CResult<Option<WorkspaceRoots>> {
    match workspace_paths() {
        Ok((worktree_root, workspace_root)) => Ok(Some(WorkspaceRoots { worktree_root, workspace_root })),
        Err(CommandError::WorkspaceDisabled | CommandError::NotInitialized { .. }) => Ok(None),
        Err(error) => Err(error),
    }
}

/// Sync the database and the workspace once, and return the sync's warnings.
pub(super) fn auto_sync(roots: &WorkspaceRoots) -> CResult<Vec<String>> {
    let mut database = open_database()?;
    let vcs = discover_vcs(&roots.worktree_root)?;
    let outcome = sync_workspace(&roots.workspace_root, &roots.worktree_root, &mut database, as_dyn(&vcs))?;
    Ok(outcome.warnings)
}

/// Take an exclusive lock on `.arcl/arcl.db-sync.lock` under `project_root`,
/// creating the file if it is missing. Blocks until no other arcl process
/// holds it. The lock is held until the returned file drops, so a caller
/// keeps it across a sync, the command and the sync after it.
pub(super) fn lock_project(project_root: &Path) -> CResult<File> {
    let path = project_root.join(ARCL_DIRECTORY).join(SYNC_LOCK_FILE);
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|source| CommandError::SyncLock { path: path.clone(), source })?;
    file.lock().map_err(|source| CommandError::SyncLock { path, source })?;
    Ok(file)
}
