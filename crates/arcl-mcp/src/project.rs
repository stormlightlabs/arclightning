//! Find the project and run one operation against its database, with the
//! workspace sync the CLI runs around a command.
//!
//! With the workspace on, every call takes the project's sync lock and syncs
//! first. A write that cannot sync fails before it changes anything; a read
//! warns and reads the database as it is. After a write, a second sync
//! exports it, and a failure there is a warning, since the change is saved.
//! The lock file is the CLI's, so the server and `arcl` commands in one
//! project run one at a time. A call waits at most [`LOCK_WAIT`] for it, then
//! fails with `application_error`, the code the CLI gives a lock failure.
//!
//! A cancelled call stops while it waits for the lock, and checks once more
//! before its operation starts. Once the operation starts it runs to the end:
//! a write is not cancellable after that point.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use schemars::JsonSchema;
use serde::Serialize;
use tokio_util::sync::CancellationToken;

use arcl_repo::vcs::{GixVcs, Vcs, VcsError};
use arcl_repo::workspace::{ProjectConfig, WorkspaceSyncError, resolve_workspace_root, sync_workspace};
use arcl_store::{Database, StorageError};

use crate::error::ToolError;

/// How long a call waits for the sync lock an `arcl` command holds before it
/// fails with `application_error`.
pub const LOCK_WAIT: Duration = Duration::from_secs(10);

/// How often a waiting call tries the lock again and checks for cancellation.
const LOCK_POLL: Duration = Duration::from_millis(25);
const ARCL_DIRECTORY: &str = ".arcl";
const CONFIG_FILE: &str = "config.toml";
const DATABASE_FILE: &str = "arcl.db";
/// The CLI's lock file under `.arcl/`, which serializes syncs in one project.
const SYNC_LOCK_FILE: &str = "arcl.db-sync.lock";

/// Whether an operation may change records.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Access {
    /// The operation only reads records.
    Read,
    /// The operation may change records.
    Write,
}

/// A condition the call went on despite. `code` is `workspace_sync` for a
/// sync that did not run, or `blocker_edge` for a blocker rule violation a
/// workspace import loaded, as in the CLI's warnings envelope.
#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize)]
pub struct Warning {
    /// The kind of warning.
    pub code: String,
    /// What happened.
    pub message: String,
}

impl Warning {
    fn workspace_sync(message: String) -> Self {
        Self { code: "workspace_sync".to_owned(), message }
    }

    fn blocker_edge(message: String) -> Self {
        Self { code: "blocker_edge".to_owned(), message }
    }
}

/// A sync, or the setup before it, that did not run.
enum SyncFailure {
    /// The setup before the sync failed, such as an invalid workspace path. A
    /// write returns it as it is; a read warns and goes on.
    Setup(ToolError),
    /// The call cannot go on, read or write: the lock wait ran out or the call
    /// was cancelled.
    Stopped(ToolError),
    /// The sync itself stopped.
    Sync(WorkspaceSyncError),
}

impl SyncFailure {
    /// The error for a write that cannot run. Only a sync that stopped gets
    /// the `arcl workspace status` wrapper, as in the CLI, and a database
    /// written by another connection since the sync read it is reported as it
    /// is, since running again settles it.
    fn before_write(self) -> ToolError {
        match self {
            Self::Setup(error) | Self::Stopped(error) => error,
            Self::Sync(error @ WorkspaceSyncError::Storage(StorageError::ChangedSinceRead)) => error.into(),
            Self::Sync(error) => ToolError::new(
                error.exit_code(),
                format!(
                    "the workspace must sync before this change, and the sync stopped: {error}; run `arcl workspace status` to see why{}",
                    error.resolve_hint()
                ),
            ),
        }
    }

    fn message(&self) -> String {
        match self {
            Self::Setup(error) | Self::Stopped(error) => error.to_string(),
            Self::Sync(error) => error.to_string(),
        }
    }
}

impl From<ToolError> for SyncFailure {
    fn from(error: ToolError) -> Self {
        Self::Setup(error)
    }
}

impl From<WorkspaceSyncError> for SyncFailure {
    fn from(error: WorkspaceSyncError) -> Self {
        Self::Sync(error)
    }
}

/// Run `operation` against the project that holds `start`, syncing the
/// workspace around it as the CLI does. Returns the operation's value and the
/// warnings the call collected.
///
/// The call waits at most `lock_wait` for the sync lock. Once `cancelled`
/// fires, the call stops before `operation` starts; after that it runs to the
/// end.
pub fn run<T>(
    start: &Path, access: Access, lock_wait: Duration, cancelled: &CancellationToken,
    operation: impl FnOnce(&mut Database) -> Result<T, ToolError>,
) -> Result<(T, Vec<Warning>), ToolError> {
    let root = nearest_project_root(start).ok_or_else(|| not_initialized(start))?;
    let config = read_config(&root)?;
    let database_path = root.join(ARCL_DIRECTORY).join(DATABASE_FILE);
    if !database_path.is_file() {
        return Err(not_initialized(&root));
    }

    let mut warnings = Vec::new();
    let mut workspace = None;
    let synced = workspace_root(&root, &config).and_then(|workspace_root| {
        let Some(workspace_root) = workspace_root else { return Ok(Vec::new()) };
        let lock = lock_project(&root, lock_wait, cancelled)?;
        let found = sync(&root, &workspace_root, &database_path);
        workspace = Some((workspace_root, lock));
        found
    });
    match synced {
        Ok(found) => warnings.extend(found.into_iter().map(Warning::blocker_edge)),
        Err(failure @ SyncFailure::Stopped(_)) => return Err(failure.before_write()),
        Err(failure) if access == Access::Write => return Err(failure.before_write()),
        Err(failure) => warnings.push(Warning::workspace_sync(format!(
            "the workspace did not sync, so this shows the database as it is: {}",
            failure.message()
        ))),
    }

    if cancelled.is_cancelled() {
        return Err(cancelled_error());
    }
    let mut database = open_database(&database_path)?;
    let value = operation(&mut database)?;
    drop(database);

    if access == Access::Write
        && let Some((workspace_root, _lock)) = &workspace
    {
        match sync(&root, workspace_root, &database_path) {
            Ok(found) => warnings.extend(found.into_iter().map(Warning::blocker_edge)),
            Err(failure) => warnings.push(Warning::workspace_sync(format!(
                "the change is saved, but the workspace was not updated: {}; the next command updates it",
                failure.message()
            ))),
        }
    }
    Ok((value, warnings))
}

/// The nearest ancestor of `start`, or `start` itself, that holds
/// `.arcl/config.toml`.
fn nearest_project_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|candidate| candidate.join(ARCL_DIRECTORY).join(CONFIG_FILE).is_file())
        .map(Path::to_owned)
}

fn not_initialized(root: &Path) -> ToolError {
    ToolError::new(
        3,
        format!(
            "Arc Lightning is not initialized in project directory `{}`; run `arcl init` first",
            root.display()
        ),
    )
}

fn read_config(root: &Path) -> Result<ProjectConfig, ToolError> {
    let path = root.join(ARCL_DIRECTORY).join(CONFIG_FILE);
    let input = fs::read_to_string(&path).map_err(|source| {
        ToolError::new(
            1,
            format!("could not read project configuration `{}`: {source}", path.display()),
        )
    })?;
    ProjectConfig::parse(&input).map_err(|source| {
        ToolError::new(
            3,
            format!("project configuration `{}` is invalid: {source}", path.display()),
        )
    })
}

/// The workspace root, or `None` when the workspace is off.
fn workspace_root(root: &Path, config: &ProjectConfig) -> Result<Option<PathBuf>, SyncFailure> {
    if !config.workspace.enabled {
        return Ok(None);
    }
    resolve_workspace_root(root, &config.workspace.path)
        .map(Some)
        .map_err(|source| {
            let path = root.join(ARCL_DIRECTORY).join(CONFIG_FILE);
            ToolError::new(
                3,
                format!("project configuration `{}` is invalid: {source}", path.display()),
            )
            .into()
        })
}

/// Take an exclusive lock on the project's sync lock file. Tries again every
/// [`LOCK_POLL`] while another process holds it, and fails with `application_error`
/// once `wait` runs out, or stops once `cancelled` fires. The lock lasts until
/// the file drops.
fn lock_project(root: &Path, wait: Duration, cancelled: &CancellationToken) -> Result<File, SyncFailure> {
    let path = root.join(ARCL_DIRECTORY).join(SYNC_LOCK_FILE);
    let lock_error =
        |source: std::io::Error| ToolError::new(1, format!("could not lock `{}`: {source}", path.display()));
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(lock_error)?;
    let deadline = Instant::now() + wait;
    loop {
        if cancelled.is_cancelled() {
            return Err(SyncFailure::Stopped(cancelled_error()));
        }
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(TryLockError::Error(source)) => return Err(lock_error(source).into()),
            Err(TryLockError::WouldBlock) if Instant::now() >= deadline => {
                return Err(SyncFailure::Stopped(ToolError::new(
                    1,
                    format!(
                        "another `arcl` command held the sync lock `{}` for more than {} s; try again when it finishes",
                        path.display(),
                        wait.as_secs_f64()
                    ),
                )));
            }
            Err(TryLockError::WouldBlock) => thread::sleep(LOCK_POLL),
        }
    }
}

/// The error for a call cancelled before its operation started.
fn cancelled_error() -> ToolError {
    ToolError::new(1, "the call was cancelled before it changed anything")
}

/// Sync the database and the workspace once and return the sync's blocker
/// rule warnings.
fn sync(root: &Path, workspace_root: &Path, database_path: &Path) -> Result<Vec<String>, SyncFailure> {
    let mut database = open_database(database_path)?;
    let vcs = match GixVcs::discover(root) {
        Ok(vcs) => Some(vcs),
        Err(VcsError::Discovery { .. }) => None,
        Err(error) => return Err(ToolError::from(error).into()),
    };
    let outcome = sync_workspace(
        workspace_root,
        root,
        &mut database,
        vcs.as_ref().map(|vcs| vcs as &dyn Vcs),
    )?;
    Ok(outcome.warnings)
}

fn open_database(path: &Path) -> Result<Database, ToolError> {
    Database::open(path).map_err(|source| {
        ToolError::new(
            source.exit_code(),
            format!("could not open Arc Lightning database `{}`: {source}", path.display()),
        )
    })
}
