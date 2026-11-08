//! `arcl prime`: the working loop and the state of work, short enough for a
//! session-start hook to inject into an agent's context.
//!
//! [`Primer`] builds the primer, which the MCP `prime` tool returns too.

use super::support::{display_ids, open_database};
use super::*;

/// How long `arcl prime` waits for the sync lock before it shows the database
/// without a sync, so a stuck command cannot stall a session start.
pub(super) const PRIME_LOCK_WAIT: Duration = Duration::from_secs(2);

/// `arcl prime`. Outside an initialized project it says to run `arcl init`
/// and succeeds, so a session-start hook does not fail in a fresh clone.
pub(super) fn read_prime(renderer: &Renderer) -> anyhow::Result<Option<String>> {
    let database = match open_database() {
        Ok(database) => database,
        Err(CommandError::Project(ProjectError::NotInitialized { root, workspace })) => {
            let display = root.display().to_string();
            let message = ProjectError::NotInitialized { root, workspace }.to_string();
            let setup = PrimeSetup { initialized: false, root: display, message };
            return Ok(renderer.render_prime_not_initialized(&setup)?);
        }
        Err(error) => return Err(ApplicationError::from_command(error).into()),
    };
    let primer = primer(&database, renderer).map_err(ApplicationError::from_command)?;
    Ok(renderer.render_primer(&primer)?)
}

/// The primer, with short IDs in human output and full IDs otherwise.
fn primer(database: &Database, renderer: &Renderer) -> CResult<Primer> {
    let ids = display_ids(database, renderer)?;
    Ok(Primer::read(database, &ids)?)
}
