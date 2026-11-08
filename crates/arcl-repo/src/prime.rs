//! The session primer `arcl prime` prints and the MCP `prime` tool returns:
//! the working loop, open work and ready work, short enough for a
//! session-start hook to inject into an agent's context.
//!
//! The loop is the block of `skills/arclightning/SKILL.md` between
//! [`LOOP_BEGIN`] and [`LOOP_END`], so the primer and the skill print the same
//! steps. Each list keeps [`PRIME_LIST_LIMIT`] tasks, so `arcl prime` can
//! print the primer within [`PRIME_MAX_LINES`] and [`PRIME_MAX_BYTES`].
//!
//! JSON keeps more of each task than text does, within per-field caps:
//! [`PRIME_TITLE_BYTES`] of a title and [`PRIME_HANDOFF_BYTES`] of a handoff,
//! each cut field ending with [`PRIME_TRUNCATED`]. Assignees are already
//! limited to [`MAX_ASSIGNEE_CHARS`]. A full primer stays near 10 KiB however
//! long its tasks' titles and handoffs are. Text output clips each field shorter still, so the caps
//! never change it.
//!
//! Outside an initialized project there is no primer: both adapters return a
//! [`PrimeSetup`] that says to run `arcl init`, and succeed.
//!
//! Building the primer only reads the database, in one read transaction. Both adapters wait at most
//! [`PRIME_LOCK_WAIT`] for the sync lock, then read the database as it is
//! with a warning, so a stuck command cannot stall a session start.

use std::collections::HashMap;
use std::time::Duration;

use serde::Serialize;

use arcl_core::prelude::*;
use arcl_store::prelude::*;

use crate::project::ProjectError;

/// The most tasks each primer list shows. The rest are counted in
/// [`PrimeList::more`].
pub const PRIME_LIST_LIMIT: usize = 5;
/// The most lines human `arcl prime` output prints. The size budget the
/// manual documents.
pub const PRIME_MAX_LINES: usize = 40;
/// The most bytes human `arcl prime` output prints.
pub const PRIME_MAX_BYTES: usize = 4096;
/// The most bytes of a task title the primer keeps before [`PRIME_TRUNCATED`].
pub const PRIME_TITLE_BYTES: usize = 200;
/// The most bytes of a handoff note the primer keeps before
/// [`PRIME_TRUNCATED`]. `arcl context` shows the whole note.
pub const PRIME_HANDOFF_BYTES: usize = 400;
/// What a field the primer cuts ends with.
pub const PRIME_TRUNCATED: &str = "… [truncated]";
/// How long `arcl prime` and the MCP `prime` tool wait for the sync lock
/// before they read the database without a sync.
pub const PRIME_LOCK_WAIT: Duration = Duration::from_secs(2);
/// `skills/arclightning/SKILL.md`, the Arc Lightning skill. The primer prints
/// the loop it marks.
pub const SKILL_MARKDOWN: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../skills/arclightning/SKILL.md"
));
/// The line that opens the loop block in `SKILL.md`.
const LOOP_BEGIN: &str = "<!-- arcl-prime:begin -->";
/// The line that closes the loop block in `SKILL.md`.
const LOOP_END: &str = "<!-- arcl-prime:end -->";

/// What `arcl --json prime` and the MCP `prime` tool return: the primer, or
/// the setup step outside an initialized project. Serializes as the inner
/// object; `initialized` tells them apart.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum Prime {
    /// The primer of an initialized project.
    Primer(Primer),
    /// The step to take where there is no initialized project.
    Setup(PrimeSetup),
}

/// The primer: the skill's working loop, open work and ready work.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Primer {
    /// Always true. Outside an initialized project the primer is a
    /// [`PrimeSetup`], with `initialized` false.
    pub initialized: bool,
    /// The loop block of the Arc Lightning skill, as Markdown.
    #[serde(rename = "loop")]
    pub work_loop: &'static str,
    /// In-progress tasks, then parked tasks.
    pub open: PrimeList,
    /// Ready tasks, in `arcl ready` order.
    pub ready: PrimeList,
}

impl Primer {
    /// Read the primer from `database`: in-progress tasks, then parked tasks,
    /// then ready tasks, each list capped at [`PRIME_LIST_LIMIT`]. Tasks show
    /// the IDs `ids` gives, so an empty [`ShortIds`] keeps full IDs. An open
    /// task's line counts its open gates; a ready task has none. Archived
    /// tasks are completed, so neither list holds one.
    ///
    /// The queries run in one read transaction, so the lists agree with each
    /// other. Titles and handoffs are cut to their caps.
    pub fn read(database: &Database, ids: &ShortIds) -> Result<Self, StorageError> {
        let (tasks, ready, gates) = database.read_snapshot(|database| {
            Ok((
                database.planning_tasks()?,
                database.ready_planning_tasks()?,
                database.gates(None)?,
            ))
        })?;
        let open = [TaskStatus::InProgress, TaskStatus::Parked]
            .into_iter()
            .flat_map(|status| tasks.iter().filter(move |task| task.status == status));
        let mut open_gates = HashMap::new();
        for gate in gates {
            if gate.status == GateStatus::Open {
                *open_gates.entry(gate.task_id).or_default() += 1;
            }
        }
        Ok(Self {
            initialized: true,
            work_loop: primer_loop(),
            open: PrimeList::new(open, ids, &open_gates),
            ready: PrimeList::new(ready.iter(), ids, &open_gates),
        })
    }
}

/// What `arcl prime` and the MCP `prime` tool return where the directory is
/// not in an initialized project, so a session-start hook succeeds in a fresh
/// clone.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PrimeSetup {
    /// Always false; [`Primer`] carries true.
    pub initialized: bool,
    /// The project directory, or the starting directory outside any project.
    pub root: String,
    /// The line human output prints, which says to run `arcl init`.
    pub message: String,
}

impl PrimeSetup {
    /// The setup step for [`ProjectError::NotInitialized`], or any other
    /// `error` back unchanged.
    pub fn from_error(error: ProjectError) -> Result<Self, ProjectError> {
        match error {
            ProjectError::NotInitialized { root, workspace } => {
                let display = root.display().to_string();
                let message = ProjectError::NotInitialized { root, workspace }.to_string();
                Ok(Self { initialized: false, root: display, message })
            }
            error => Err(error),
        }
    }
}

/// One primer list: at most [`PRIME_LIST_LIMIT`] tasks, and how many more
/// there are.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PrimeList {
    /// The first tasks, in order.
    pub tasks: Vec<PrimeTask>,
    /// How many tasks the list leaves out.
    pub more: usize,
}

impl PrimeList {
    /// Keep the first [`PRIME_LIST_LIMIT`] of `tasks`, with IDs from `ids`
    /// and each task's open gate count from `open_gates`, and count the
    /// rest.
    pub fn new<'a>(
        tasks: impl IntoIterator<Item = &'a PlanningTask>, ids: &ShortIds, open_gates: &HashMap<TaskId, usize>,
    ) -> Self {
        let mut list = Self { tasks: Vec::new(), more: 0 };
        for task in tasks {
            if list.tasks.len() < PRIME_LIST_LIMIT {
                let gates = open_gates.get(&task.id).copied().unwrap_or_default();
                list.tasks.push(PrimeTask::new(task, ids, gates));
            } else {
                list.more += 1;
            }
        }
        list
    }
}

/// One task in a primer list.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PrimeTask {
    /// The full ID, or the short ID in human output.
    pub id: String,
    /// The title, cut to [`PRIME_TITLE_BYTES`].
    pub title: String,
    /// The task's status.
    pub status: TaskStatus,
    /// The task's priority.
    pub priority: TaskPriority,
    /// Who claimed the task, if anyone did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    /// The latest handoff note, cut to [`PRIME_HANDOFF_BYTES`], or empty.
    pub handoff: String,
    /// How many open gates the task has; JSON omits it at zero.
    #[serde(skip_serializing_if = "is_zero")]
    #[cfg_attr(feature = "schema", schemars(default))]
    pub open_gates: usize,
}

impl PrimeTask {
    fn new(task: &PlanningTask, ids: &ShortIds, open_gates: usize) -> Self {
        let id = task.id.to_string();
        Self {
            id: ids.short(&id).to_owned(),
            title: truncate(&task.title, PRIME_TITLE_BYTES),
            status: task.status,
            priority: task.priority,
            assignee: task.assignee.clone(),
            handoff: truncate(&task.handoff, PRIME_HANDOFF_BYTES),
            open_gates,
        }
    }
}

/// The loop block of `SKILL.md`, without its marker lines. The
/// `skill_marks_the_primer_loop` test fails when the markers go missing.
fn primer_loop() -> &'static str {
    SKILL_MARKDOWN
        .split_once(LOOP_BEGIN)
        .and_then(|(_, rest)| rest.split_once(LOOP_END))
        .map_or("", |(block, _)| block.trim())
}

/// `text`, or its first `limit` bytes cut at a character boundary and
/// followed by [`PRIME_TRUNCATED`] when it is longer.
fn truncate(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_owned();
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{PRIME_TRUNCATED}", &text[..end])
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a pending task titled `title` in `phase`, if any.
    fn create_task(database: &mut Database, title: &str, phase_id: Option<PhaseId>) -> TaskId {
        let project_id = database.project().expect("project exists").id;
        database
            .create_planning_task(PlanningTaskCreate {
                project_id,
                spec_id: None,
                plan_id: None,
                phase_id,
                parent_id: None,
                title: title.to_owned(),
                body: String::new(),
                priority: TaskPriority::Normal,
                position: 0,
            })
            .expect("task creates")
            .id
    }

    fn ids(list: &PrimeList) -> Vec<String> {
        list.tasks.iter().map(|task| task.id.clone()).collect()
    }

    #[test]
    fn skill_marks_the_primer_loop() {
        let work_loop = primer_loop();
        assert!(work_loop.starts_with("1. "), "loop block: {work_loop:?}");
        assert!(work_loop.contains("arcl next --start --claim"));
        assert!(work_loop.contains("arcl task handoff"));
        assert!(work_loop.len() < 700, "loop block is {} bytes", work_loop.len());
    }

    #[test]
    fn a_primer_lists_in_progress_then_parked_then_ready_with_gate_counts() {
        let mut database = Database::open_in_memory().expect("database opens");
        let parked = create_task(&mut database, "Parked", None);
        let started = create_task(&mut database, "Started", None);
        let ready = create_task(&mut database, "Ready", None);
        for task in [parked, started] {
            database.start_planning_task(task, false).expect("task starts");
        }
        database
            .handoff_planning_task(parked, "Waiting on review.\nMore detail.".to_owned())
            .expect("task parks");
        database
            .add_gate(started, GateKind::Date, "2999-01-01")
            .expect("gate adds");
        database
            .add_gate(started, GateKind::Date, "2999-01-02")
            .expect("gate adds");

        let primer = Primer::read(&database, &ShortIds::default()).expect("primer reads");
        assert!(primer.initialized);
        assert_eq!(primer.work_loop, primer_loop());
        assert_eq!(ids(&primer.open), [started.to_string(), parked.to_string()]);
        assert_eq!(ids(&primer.ready), [ready.to_string()]);
        assert_eq!(primer.open.tasks[0].open_gates, 2);
        assert_eq!(primer.open.tasks[1].handoff, "Waiting on review.\nMore detail.");

        let json = serde_json::to_value(&primer).expect("primer serializes");
        assert_eq!(json["loop"], primer_loop());
        assert_eq!(json["open"]["tasks"][0]["open_gates"], 2);
        assert!(json["open"]["tasks"][1].get("open_gates").is_none(), "{json}");
        assert!(json["open"]["tasks"][1].get("assignee").is_none(), "{json}");
    }

    #[test]
    fn a_primer_shows_the_ids_it_is_given() {
        let mut database = Database::open_in_memory().expect("database opens");
        let task = create_task(&mut database, "Ready", None);
        let short_ids = database.short_ids().expect("short IDs read");
        let primer = Primer::read(&database, &short_ids).expect("primer reads");
        let id = task.to_string();
        assert_eq!(ids(&primer.ready), [short_ids.short(&id).to_owned()]);
        assert!(primer.ready.tasks[0].id.len() < id.len());
    }

    #[test]
    fn a_full_primer_caps_each_list() {
        let mut database = Database::open_in_memory().expect("database opens");
        let long = format!("{}\nsecond line", "\u{1b}x".repeat(200));
        for index in 0..8 {
            let task = create_task(&mut database, &format!("Open {index} {long}"), None);
            database
                .claim_planning_task(task, false, &"agént".repeat(12))
                .expect("task claims");
            database.handoff_planning_task(task, long.clone()).expect("task parks");
            create_task(&mut database, &format!("Ready {index} {long}"), None);
        }

        let primer = Primer::read(&database, &ShortIds::default()).expect("primer reads");
        assert_eq!((primer.open.tasks.len(), primer.open.more), (PRIME_LIST_LIMIT, 3));
        assert_eq!((primer.ready.tasks.len(), primer.ready.more), (PRIME_LIST_LIMIT, 3));
        // Each field is cut to its cap; text output clips them shorter.
        let title = format!("Ready 0 {long}");
        assert_eq!(primer.ready.tasks[0].title, truncate(&title, PRIME_TITLE_BYTES));
        assert_eq!(primer.open.tasks[0].handoff, truncate(&long, PRIME_HANDOFF_BYTES));
    }

    #[test]
    fn a_primer_cuts_huge_fields_to_their_caps() {
        let mut database = Database::open_in_memory().expect("database opens");
        let huge = "é".repeat(25_000);
        for index in 0..10 {
            let task = create_task(&mut database, &format!("Task {index} {huge}"), None);
            // Five parked tasks with huge handoffs, then five ready ones.
            if index < 5 {
                database
                    .claim_planning_task(task, false, &"é".repeat(MAX_ASSIGNEE_CHARS))
                    .expect("task claims");
                database.handoff_planning_task(task, huge.clone()).expect("task parks");
            }
        }

        let primer = Primer::read(&database, &ShortIds::default()).expect("primer reads");
        let tasks = primer.open.tasks.iter().chain(&primer.ready.tasks);
        assert_eq!(tasks.clone().count(), 10);
        for task in tasks {
            assert!(task.title.ends_with(PRIME_TRUNCATED), "{}", task.title);
            assert!(task.title.len() <= PRIME_TITLE_BYTES + PRIME_TRUNCATED.len());
        }
        for task in &primer.open.tasks {
            assert!(task.handoff.ends_with(PRIME_TRUNCATED));
            assert!(task.handoff.len() <= PRIME_HANDOFF_BYTES + PRIME_TRUNCATED.len());
        }
        let json = serde_json::to_string(&primer).expect("primer serializes");
        assert!(json.len() < 10 * 1024, "primer is {} bytes", json.len());
    }

    #[test]
    fn truncate_keeps_short_text_and_cuts_at_a_character_boundary() {
        assert_eq!(truncate("short", 80), "short");
        assert_eq!(truncate(&"a".repeat(80), 80), "a".repeat(80));
        assert_eq!(truncate("aé", 2), format!("a{PRIME_TRUNCATED}"));
    }

    #[test]
    fn a_setup_step_stands_in_for_an_uninitialized_project() {
        let root = std::path::PathBuf::from("/tmp/fresh");
        let setup = PrimeSetup::from_error(ProjectError::NotInitialized { root: root.clone(), workspace: None })
            .expect("setup builds");
        assert!(!setup.initialized);
        assert_eq!(setup.root, root.display().to_string());
        assert!(setup.message.contains("arcl init"), "{}", setup.message);
        assert!(PrimeSetup::from_error(ProjectError::Cancelled).is_err());

        let json = serde_json::to_value(Prime::Setup(setup)).expect("setup serializes");
        assert_eq!(json["initialized"], false);
    }

    #[test]
    fn a_primer_leaves_out_archived_tasks() {
        let mut database = Database::open_in_memory().expect("database opens");
        let spec = database
            .create_spec("Feature".to_owned(), String::new(), String::new())
            .expect("spec creates");
        let plan = database
            .create_plan(spec.id, "Roadmap".to_owned(), String::new())
            .expect("plan creates");
        let phase = database
            .create_phase(plan.id, "First".to_owned(), String::new(), 1)
            .expect("phase creates");
        let shipped = create_task(&mut database, "Shipped", Some(phase.id));
        database
            .complete_planning_task(shipped, OpenSubtasks::Refuse, None)
            .expect("task completes");
        let next = create_task(&mut database, "Next", None);
        let compaction = database
            .compact(&CompactionRequest {
                before: "2999-01-01".to_owned(),
                phase_id: Some(phase.id),
                include_undated: true,
                dry_run: false,
            })
            .expect("compaction runs");
        assert_eq!(compaction.phases.len(), 1);

        let primer = Primer::read(&database, &ShortIds::default()).expect("primer reads");
        assert_eq!(ids(&primer.ready), [next.to_string()]);
        assert!(primer.open.tasks.is_empty());
    }
}
