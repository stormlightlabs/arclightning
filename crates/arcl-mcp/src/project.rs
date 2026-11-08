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
//! the CLI gives a lock failure, whether it reads or writes. The exception is
//! [`Access::Prime`], which waits at most [`PRIME_LOCK_WAIT`] and then reads
//! the database as it is with a warning, as `arcl prime` does.
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
    /// The operation only reads records. It waits at most [`PRIME_LOCK_WAIT`]
    /// for the lock, then reads the database as it is with a
    /// `workspace_sync` warning instead of failing. `prime` reads this way, as `arcl prime` does, so an agent
    /// starting a session still gets its loop and its work.
    Prime,
    /// The operation may change records.
    Write,
}

/// Run `operation` against the project that holds `start`, syncing the
/// workspace around it as the CLI does. Returns the operation's value and the
/// warnings the call collected.
///
/// The call waits at most `lock_wait` for the sync lock, or at most
/// [`PRIME_LOCK_WAIT`] for [`Access::Prime`]. Once `cancelled`
/// fires, the call stops before `operation` starts; after that it runs to the
/// end.
pub fn run<T>(
    start: &Path, access: Access, lock_wait: Duration, cancelled: &CancellationToken,
    operation: impl FnOnce(&mut Database) -> Result<T, ToolError>,
) -> Result<(T, Vec<Warning>), ToolError> {
    let project = ProjectDir::discover(start)?;
    project.require_database()?;

    let lock_wait = match access {
        Access::Prime => lock_wait.min(PRIME_LOCK_WAIT),
        Access::Read | Access::Write => lock_wait,
    };
    let is_cancelled = || cancelled.is_cancelled();
    let wait = LockWait::Within { timeout: lock_wait, cancelled: &is_cancelled };
    let SyncBefore { locked, result } = sync_before(project.workspace(), wait);
    let mut warnings = Vec::new();
    match result {
        Ok(found) => warnings.extend(Warning::blocker_edges(found)),
        Err(error) if access == Access::Write || stops_reads(&error, access) => {
            return Err(error.before_write().into());
        }
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

/// Whether a failure before the call stops it even as a read. A cancelled
/// call always stops. A held lock stops an [`Access::Read`] call; an
/// [`Access::Prime`] call warns and reads, as the CLI does for any read.
const fn stops_reads(error: &ProjectError, access: Access) -> bool {
    match error {
        ProjectError::Cancelled => true,
        ProjectError::SyncLockBusy { .. } => !matches!(access, Access::Prime),
        _ => false,
    }
}
