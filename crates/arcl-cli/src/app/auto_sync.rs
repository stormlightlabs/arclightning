//! Automatic workspace sync around ordinary commands.
//!
//! When the workspace is enabled, reads and writes sync first so they see
//! changes made on either side. A write that cannot sync refuses before it
//! changes anything; a read warns and shows the database as it is. After a
//! write commits, a second sync exports it. If that export fails, the change
//! is already saved and the next command finishes the export.
//!
//! Every sync runs under the project's sync lock, so arcl commands in one
//! project with the workspace on run one at a time. A command waits for the
//! lock as long as it takes. `arcl serve` waits a bounded time instead, so a
//! long command cannot hold up the server, and so does `arcl prime`, which
//! then shows the database without a sync. The lock and the syncs come from
//! [`arcl_repo::project`], which the MCP server uses too.

use super::*;

/// How a command touches project records.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Access {
    /// The command does not read or write records, manages the workspace
    /// itself, or, like `serve`, runs too long to hold the sync lock.
    Untracked,
    /// The command only reads records.
    Read,
    /// The command may change records.
    Write,
}

/// Classify a parsed command by how it touches records.
pub(super) fn command_access(command: Option<&Command>) -> Access {
    let read_when = |read: bool| if read { Access::Read } else { Access::Write };
    match command {
        // `serve` runs until interrupted, so it must not hold the sync lock.
        None
        | Some(
            Command::Init { .. }
            | Command::Workspace { .. }
            | Command::Serve { .. }
            | Command::Skill { .. }
            | Command::Setup { .. }
            | Command::Query { schema: true, .. },
        ) => Access::Untracked,
        Some(
            Command::Show { .. }
            | Command::List { .. }
            | Command::Tree { .. }
            | Command::Roadmap { .. }
            | Command::Explain { .. }
            | Command::Context { .. }
            | Command::Prime
            | Command::Check
            | Command::Ready { .. }
            | Command::Next { start: false, .. }
            | Command::Query { schema: false, .. },
        ) => Access::Read,
        Some(Command::Next { start: true, .. }) => Access::Write,
        Some(Command::Compact { dry_run, .. }) => read_when(*dry_run),
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
            TaskCommand::Show { .. }
                | TaskCommand::List { .. }
                | TaskCommand::Explain { .. }
                | TaskCommand::Context { .. }
        )),
        Some(Command::Dependency { command }) => read_when(matches!(command, DependencyCommand::List)),
        // `gate check` takes the lock to read and again to write, and not
        // while `gh` runs.
        Some(Command::Gate { command: GateCommand::Check { .. } }) => Access::Untracked,
        Some(Command::Gate { command }) => read_when(matches!(command, GateCommand::List { .. })),
        // Each applied edit takes the lock for itself, as `serve` does.
        Some(Command::Web { command: WebCommand::Apply { .. } }) => Access::Untracked,
        Some(Command::Web { command: WebCommand::Export { .. } }) => Access::Read,
        Some(Command::Note { command }) => read_when(match command {
            NoteCommand::Show { .. } | NoteCommand::List => true,
            NoteCommand::Link { command } => matches!(command, NoteLinkCommand::List { .. }),
            _ => false,
        }),
    }
}

/// The workspace of the project that holds the current directory, or `None`
/// when there is no project or its workspace is off. Any other configuration
/// error, such as a symlinked workspace, is returned so the caller treats it
/// as a failed sync.
pub(super) fn current_workspace() -> Result<Option<Workspace>, ProjectError> {
    match ProjectDir::discover_current() {
        Ok(project) => project.workspace(),
        Err(ProjectError::NotInitialized { .. }) => Ok(None),
        Err(error) => Err(error),
    }
}

/// Run `write` as `arcl` runs a write command: under the sync lock, after a
/// sync that must succeed, and followed by a sync that exports the change.
/// Returns what `write` returned with the warnings both syncs gave.
///
/// Unlike a command, this waits at most `lock_wait` for the lock, then fails
/// with an error whose [`ApplicationError::lock_busy`] is set, before it
/// changes anything.
pub(super) fn synced_write<T>(
    lock_wait: Duration, write: impl FnOnce() -> Result<T, ApplicationError>,
) -> Result<(T, Vec<Warning>), ApplicationError> {
    synced(Some(lock_wait), true, write)
}

/// Run `operation` under the sync lock, after a sync that must succeed, and
/// release the lock when it returns. With `export`, a second sync exports
/// what `operation` changed. Returns what `operation` returned with the
/// warnings the syncs gave.
///
/// `lock_wait` bounds the wait for the lock as [`synced_write`] does; `None`
/// waits as long as it takes, as a command does. A write command that does
/// slow work between a read and a write, such as `gate check` asking `gh`,
/// runs each in its own call, so the lock is not held while it waits.
pub(super) fn synced<T>(
    lock_wait: Option<Duration>, export: bool, operation: impl FnOnce() -> Result<T, ApplicationError>,
) -> Result<(T, Vec<Warning>), ApplicationError> {
    let wait = lock_wait.map_or(LockWait::Unbounded, LockWait::Within);
    let SyncBefore { locked, result } = sync_before(current_workspace(), wait, &|| false);
    let mut warnings = match result {
        Ok(found) => Warning::blocker_edges(found).collect(),
        // A missing database is the operation's own error to report.
        Err(ProjectError::NotInitialized { .. }) => Vec::new(),
        Err(error) => return Err(ApplicationError::from_command(error.before_write())),
    };
    let value = operation()?;
    if export && let Some(locked) = &locked {
        locked.export_after_write(&mut warnings);
    }
    Ok((value, warnings))
}
