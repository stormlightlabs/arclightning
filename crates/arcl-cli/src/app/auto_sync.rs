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
//! lock as long as it takes; `arcl serve` waits a bounded time instead, so a
//! long command cannot hold up the server.

use std::fs::TryLockError;
use std::thread;
use std::time::Instant;

use super::support::*;
use super::workspace::{as_dyn, discover_vcs};
use super::*;

/// The lock file, under `.arcl/`, that serializes syncs in one project. The
/// `/arcl.db-*` entry in `.arcl/.gitignore` keeps it out of Git.
const SYNC_LOCK_FILE: &str = "arcl.db-sync.lock";

/// How often [`lock_project_within`] tries the lock again while another
/// process holds it.
const LOCK_POLL: Duration = Duration::from_millis(25);

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

    /// Take the project's sync lock, waiting at most `wait`. See
    /// [`lock_project_within`].
    fn lock_within(&self, wait: Duration) -> CResult<File> {
        lock_project_within(&self.worktree_root, wait)
    }
}

/// Classify a parsed command by how it touches records.
pub(super) fn command_access(command: Option<&Command>) -> Access {
    let read_when = |read: bool| if read { Access::Read } else { Access::Write };
    match command {
        // `serve` runs until interrupted, so it must not hold the sync lock.
        None
        | Some(Command::Init { .. } | Command::Workspace { .. } | Command::Serve { .. } | Command::Skill { .. }) => {
            Access::Untracked
        }
        Some(
            Command::Show { .. }
            | Command::List { .. }
            | Command::Tree { .. }
            | Command::Explain { .. }
            | Command::Context { .. }
            | Command::Prime
            | Command::Check
            | Command::Ready { .. }
            | Command::Next { start: false, .. },
        ) => Access::Read,
        Some(Command::Next { start: true, .. }) => Access::Write,
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
    let roots = match workspace_roots() {
        Ok(Some(roots)) => roots,
        // A missing database is the write's own error to report.
        Ok(None) | Err(CommandError::NotInitialized { .. }) => return write().map(|value| (value, Vec::new())),
        Err(error) => return Err(refuse_write(error)),
    };
    let _lock = roots.lock_within(lock_wait).map_err(refuse_write)?;
    let mut warnings = match auto_sync(&roots) {
        Ok(found) => Warning::blocker_edges(found).collect(),
        Err(CommandError::NotInitialized { .. }) => Vec::new(),
        Err(error) => return Err(refuse_write(error)),
    };
    let value = write()?;
    export_after_write(&roots, &mut warnings);
    Ok((value, warnings))
}

/// The error a write returns when the sync before it fails. A sync that
/// stopped says the write did not run, except when the database changed
/// while it read, which keeps its own error.
pub(super) fn refuse_write(error: CommandError) -> ApplicationError {
    let error = match error {
        CommandError::WorkspaceSync(error)
            if !matches!(*error, WorkspaceSyncError::Storage(StorageError::ChangedSinceRead)) =>
        {
            CommandError::AutoSync(error)
        }
        other => other,
    };
    ApplicationError::from_command(error)
}

/// Sync after a committed write to export it, adding a warning when the
/// export fails, since the change is saved either way.
pub(super) fn export_after_write(roots: &WorkspaceRoots, warnings: &mut Vec<Warning>) {
    match auto_sync(roots) {
        Ok(found) => warnings.extend(Warning::blocker_edges(found)),
        Err(error) => warnings.push(Warning::workspace_sync(format!(
            "the change is saved, but the workspace was not updated: {error}; the next command updates it"
        ))),
    }
}

/// Take an exclusive lock on `.arcl/arcl.db-sync.lock` under `project_root`,
/// creating the file if it is missing. Blocks until no other arcl process
/// holds it. The lock is held until the returned file drops, so a caller
/// keeps it across a sync, the command and the sync after it.
pub(super) fn lock_project(project_root: &Path) -> CResult<File> {
    let (path, file) = open_lock_file(project_root)?;
    file.lock().map_err(|source| CommandError::SyncLock { path, source })?;
    Ok(file)
}

/// Take the lock [`lock_project`] takes, trying again every [`LOCK_POLL`]
/// while another process holds it, and fail with
/// [`CommandError::SyncLockBusy`] once `wait` passes.
fn lock_project_within(project_root: &Path, wait: Duration) -> CResult<File> {
    let (path, file) = open_lock_file(project_root)?;
    let deadline = Instant::now() + wait;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(TryLockError::Error(source)) => return Err(CommandError::SyncLock { path, source }),
            Err(TryLockError::WouldBlock) if Instant::now() >= deadline => {
                return Err(CommandError::SyncLockBusy { path, waited: wait });
            }
            Err(TryLockError::WouldBlock) => thread::sleep(LOCK_POLL),
        }
    }
}

/// Open, creating it if missing, the sync lock file under `project_root`.
fn open_lock_file(project_root: &Path) -> CResult<(PathBuf, File)> {
    let path = project_root.join(ARCL_DIRECTORY).join(SYNC_LOCK_FILE);
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|source| CommandError::SyncLock { path: path.clone(), source })?;
    Ok((path, file))
}
