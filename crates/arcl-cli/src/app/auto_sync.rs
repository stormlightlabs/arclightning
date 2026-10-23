//! Automatic workspace sync around ordinary commands.
//!
//! When the workspace is enabled, reads and writes sync first so they see
//! changes made on either side. A write that cannot sync refuses before it
//! changes anything; a read warns and shows the database as it is. After a
//! write commits, a second sync exports it. If that export fails, the change
//! is already saved and the next command finishes the export.

use super::support::*;
use super::workspace::{as_dyn, discover_vcs};
use super::*;

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

/// Classify a parsed command by how it touches records.
pub(super) fn command_access(command: Option<&Command>) -> Access {
    let read_when = |read: bool| if read { Access::Read } else { Access::Write };
    match command {
        None | Some(Command::Init { .. } | Command::Workspace { .. }) => Access::Untracked,
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
/// workspace is off. Configuration errors are left for the command to report.
pub(super) fn workspace_roots() -> Option<WorkspaceRoots> {
    workspace_paths()
        .ok()
        .map(|(worktree_root, workspace_root)| WorkspaceRoots { worktree_root, workspace_root })
}

/// Sync the database and the workspace once.
pub(super) fn auto_sync(roots: &WorkspaceRoots) -> CResult<()> {
    let mut database = open_database()?;
    let vcs = discover_vcs(&roots.worktree_root)?;
    sync_workspace(&roots.workspace_root, &roots.worktree_root, &mut database, as_dyn(&vcs))?;
    Ok(())
}
