//! Find the project and run one operation against its database, with the
//! workspace sync the CLI runs around a command.
//!
//! The project lookup, the sync lock and the syncs come from
//! [`arcl_repo::project`], which the CLI uses too. With the workspace on,
//! every call takes the project's sync lock and syncs first. A write that
//! cannot sync fails before it changes anything; a read warns and reads the
//! database as it is. After a write, a second sync exports it, and a failure
//! there is a warning, since the change is saved. A call waits at most
//! [`LOCK_WAIT`] for the lock, then fails with `application_error`, the code
//! the CLI gives a lock failure, whether it reads or writes.
//!
//! A cancelled call stops while it waits for the lock, and checks once more
//! before its operation starts. Once the operation starts it runs to the end:
//! a write is not cancellable after that point.

use std::path::Path;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use arcl_repo::prelude::*;
use arcl_store::prelude::*;

use crate::error::ToolError;

/// How long a call waits for the sync lock an `arcl` command holds before it
/// fails with `application_error`.
pub const LOCK_WAIT: Duration = Duration::from_secs(10);

/// Whether an operation may change records.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Access {
    /// The operation only reads records.
    Read,
    /// The operation may change records.
    Write,
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
    let project = ProjectDir::discover(start)?;
    project.require_database()?;

    let is_cancelled = || cancelled.is_cancelled();
    let wait = LockWait::Within { timeout: lock_wait, cancelled: &is_cancelled };
    let SyncBefore { locked, result } = sync_before(project.workspace(), wait);
    let mut warnings = Vec::new();
    match result {
        Ok(found) => warnings.extend(Warning::blocker_edges(found)),
        Err(error) if error.stops_reads() || access == Access::Write => return Err(error.before_write().into()),
        Err(error) => warnings.push(Warning::unsynced_read(&error)),
    }

    if cancelled.is_cancelled() {
        return Err(ProjectError::Cancelled.into());
    }
    let mut database = project.open_database()?;
    let value = operation(&mut database)?;
    drop(database);

    if access == Access::Write
        && let Some(locked) = &locked
    {
        locked.export_after_write(&mut warnings);
    }
    Ok((value, warnings))
}
