//! `arcl prime`: the working loop and the state of work, short enough for a
//! session-start hook to inject into an agent's context.
//!
//! The loop is the block of `skills/arclightning/SKILL.md` between
//! [`LOOP_BEGIN`] and [`LOOP_END`], so the primer and the skill print the same
//! steps.

use super::skill::SKILL_MARKDOWN;
use super::support::{display_ids, open_database};
use super::*;

/// The line that opens the loop block in `SKILL.md`.
const LOOP_BEGIN: &str = "<!-- arcl-prime:begin -->";
/// The line that closes the loop block in `SKILL.md`.
const LOOP_END: &str = "<!-- arcl-prime:end -->";

/// `arcl prime`. Outside an initialized project it says to run `arcl init`
/// and succeeds, so a session-start hook does not fail in a fresh clone.
pub(super) fn read_prime(renderer: &Renderer) -> anyhow::Result<Option<String>> {
    let database = match open_database() {
        Ok(database) => database,
        Err(CommandError::NotInitialized { root }) => {
            let display = root.display().to_string();
            let message = CommandError::NotInitialized { root }.to_string();
            let setup = PrimeSetup { initialized: false, root: display, message };
            return Ok(renderer.render_prime_not_initialized(&setup)?);
        }
        Err(error) => return Err(ApplicationError::from_command(error).into()),
    };
    let primer = primer(&database, renderer).map_err(ApplicationError::from_command)?;
    Ok(renderer.render_primer(&primer)?)
}

/// The loop block of `SKILL.md`, without its marker lines. The
/// `skill_marks_the_primer_loop` test fails when the markers go missing.
fn primer_loop() -> &'static str {
    SKILL_MARKDOWN
        .split_once(LOOP_BEGIN)
        .and_then(|(_, rest)| rest.split_once(LOOP_END))
        .map_or("", |(block, _)| block.trim())
}

/// In-progress tasks, then parked tasks, then ready tasks, each list capped
/// at [`PRIME_LIST_LIMIT`].
fn primer(database: &Database, renderer: &Renderer) -> CResult<Primer> {
    let ids = display_ids(database, renderer)?;
    let tasks = database.planning_tasks()?;
    let open = [TaskStatus::InProgress, TaskStatus::Parked]
        .into_iter()
        .flat_map(|status| tasks.iter().filter(move |task| task.status == status));
    let ready = database.ready_planning_tasks()?;
    Ok(Primer {
        initialized: true,
        work_loop: primer_loop(),
        open: PrimeList::new(open, &ids),
        ready: PrimeList::new(ready.iter(), &ids),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_marks_the_primer_loop() {
        let work_loop = primer_loop();
        assert!(work_loop.starts_with("1. "), "loop block: {work_loop:?}");
        assert!(work_loop.contains("arcl next --start --claim"));
        assert!(work_loop.contains("arcl task handoff"));
        assert!(work_loop.len() < 700, "loop block is {} bytes", work_loop.len());
    }
}
