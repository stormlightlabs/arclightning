//! The mutations and the JSON options each takes.

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::answer::Reply;
use crate::routes::Write;

/// The operations a queued change can name, as `arcl web apply` reads them.
/// Each row holds the name, whether it names a target record, and the
/// [`Write`] it runs. This table is the only map from a name to a mutation,
/// so a name outside it is refused, never run as another mutation.
const OPERATIONS: &[Operation] = &[
    ("create_task", false, |_| Write::Create),
    ("update_task", true, Write::Update),
    ("start_task", true, Write::Start),
    ("park_task", true, Write::Park),
    ("unpark_task", true, Write::Unpark),
    ("hand_off_task", true, Write::Handoff),
    ("complete_task", true, Write::Complete),
    ("create_capture", false, |_| Write::CreateCapture),
    ("promote_capture", true, Write::PromoteCapture),
    ("discard_capture", true, Write::DiscardCapture),
    ("create_spec", false, |_| Write::CreateSpec),
    ("create_plan", false, |_| Write::CreatePlan),
    ("apply_plan", true, Write::ApplyPlan),
    ("update_source", true, Write::EditRecord),
];

/// One row of [`OPERATIONS`]: the name, whether it names a target record,
/// and the [`Write`] it runs given the target's ID.
type Operation = (&'static str, bool, fn(String) -> Write);

/// The options of `arcl task create`, as the body of `POST /api/tasks`.
/// `priority` defaults to `normal`.
#[derive(Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NewTask {
    /// The task title.
    pub title: String,
    /// The Markdown body.
    #[serde(default)]
    pub body: Option<String>,
    /// The task priority.
    #[serde(default)]
    pub priority: Option<String>,
    /// The parent task's ID.
    #[serde(default)]
    pub parent: Option<String>,
    /// The spec's ID.
    #[serde(default)]
    pub spec: Option<String>,
    /// The plan's ID.
    #[serde(default)]
    pub plan: Option<String>,
    /// The phase's ID.
    #[serde(default)]
    pub phase: Option<String>,
    /// The IDs of the tasks that must complete first.
    #[serde(default)]
    pub blocked_by: Vec<String>,
}

/// The options of `arcl task update <id>`. A field left out keeps its value.
#[derive(Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TaskEdit {
    /// The new title.
    #[serde(default)]
    pub title: Option<String>,
    /// The new Markdown body.
    #[serde(default)]
    pub body: Option<String>,
    /// The new priority.
    #[serde(default)]
    pub priority: Option<String>,
}

/// The body of `POST /api/records/{id}/edit`, and the input of the queued
/// `update_source` operation: a complete record file and the revision of the
/// file it was edited from.
#[derive(Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecordSourceEdit {
    /// The whole file: TOML front matter between `+++` lines, then the
    /// Markdown body.
    pub source: String,
    /// The `revision` that `GET /api/records/{id}/source` gave for the file
    /// the edit started from.
    pub revision: String,
}

/// The options of `arcl task start <id>`.
#[derive(Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TaskStart {
    /// Start the task even when it is not ready.
    #[serde(default)]
    pub force: bool,
}

/// The options of `arcl task handoff <id>`.
#[derive(Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Handoff {
    /// The Markdown resume note. The CLI refuses a handoff without one.
    #[serde(default)]
    pub note: Option<String>,
}

/// The options of `arcl task complete <id>`.
#[derive(Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Completion {
    /// Markdown completion evidence.
    #[serde(default)]
    pub evidence: Option<String>,
    /// Complete only this task even when descendants are open.
    #[serde(default)]
    pub allow_open_children: bool,
    /// Also complete every pending and in-progress descendant.
    #[serde(default)]
    pub with_subtasks: bool,
}

/// The options of `arcl capture create`, as the body of `POST /api/captures`.
#[derive(Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NewCapture {
    /// The capture title.
    pub title: String,
    /// The Markdown body.
    #[serde(default)]
    pub body: Option<String>,
}

/// The options of `arcl capture promote <id>`, as the body of
/// `POST /api/captures/{id}/promote`. A title or body left out is the
/// capture's. `acceptance_criteria` applies to a spec; `spec`, `plan`,
/// `phase`, `parent` and `priority` apply to a task, whose priority defaults
/// to `normal`.
#[derive(Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CapturePromotion {
    /// What the capture becomes: `task`, `spec` or `note`.
    pub to: String,
    /// The new record's title.
    #[serde(default)]
    pub title: Option<String>,
    /// The new record's Markdown body.
    #[serde(default)]
    pub body: Option<String>,
    /// A spec's Markdown acceptance criteria.
    #[serde(default)]
    pub acceptance_criteria: Option<String>,
    /// A task's spec ID.
    #[serde(default)]
    pub spec: Option<String>,
    /// A task's plan ID.
    #[serde(default)]
    pub plan: Option<String>,
    /// A task's phase ID.
    #[serde(default)]
    pub phase: Option<String>,
    /// A task's parent task ID.
    #[serde(default)]
    pub parent: Option<String>,
    /// A task's priority.
    #[serde(default)]
    pub priority: Option<String>,
}

/// The options of `arcl spec create`, as the body of `POST /api/specs`.
#[derive(Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NewSpec {
    /// The spec title.
    pub title: String,
    /// The Markdown body.
    #[serde(default)]
    pub body: Option<String>,
    /// The Markdown acceptance criteria.
    #[serde(default)]
    pub acceptance_criteria: Option<String>,
}

/// The options of `arcl plan create`, as the body of `POST /api/plans`.
#[derive(Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NewPlan {
    /// The plan title.
    pub title: String,
    /// The ID of the spec that owns the plan.
    pub spec: String,
    /// The Markdown body.
    #[serde(default)]
    pub body: Option<String>,
}

/// The input of `arcl plan apply <id>`, as the body of
/// `POST /api/plans/{id}/apply`.
#[derive(Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PlanInput {
    /// The structured plan as TOML, as `--file` holds it.
    pub document: String,
}

/// The application's mutations, each answering with the CLI's JSON
/// envelope for the same command. Each runs as the CLI runs a write, with the
/// same validation and errors.
pub trait Mutations {
    /// `arcl task create`.
    fn create_task(&self, input: NewTask) -> Reply;
    /// `arcl task update <id>`.
    fn update_task(&self, id: &str, input: TaskEdit) -> Reply;
    /// `arcl task start <id>`.
    fn start_task(&self, id: &str, input: TaskStart) -> Reply;
    /// `arcl task park <id>`.
    fn park_task(&self, id: &str) -> Reply;
    /// `arcl task unpark <id>`.
    fn unpark_task(&self, id: &str) -> Reply;
    /// `arcl task handoff <id>`.
    fn hand_off_task(&self, id: &str, input: Handoff) -> Reply;
    /// `arcl task complete <id>`.
    fn complete_task(&self, id: &str, input: Completion) -> Reply;
    /// `arcl capture create`.
    fn create_capture(&self, input: NewCapture) -> Reply;
    /// `arcl capture promote <id>`.
    fn promote_capture(&self, id: &str, input: CapturePromotion) -> Reply;
    /// `arcl capture discard <id>`.
    fn discard_capture(&self, id: &str) -> Reply;
    /// `arcl spec create`.
    fn create_spec(&self, input: NewSpec) -> Reply;
    /// `arcl plan create`, without structured input.
    fn create_plan(&self, input: NewPlan) -> Reply;
    /// `arcl plan apply <id>`.
    fn apply_plan(&self, id: &str, input: PlanInput) -> Reply;
    /// Replace the workspace file of the record `id` with `input.source`,
    /// checked as a workspace import checks a file, when the file still has
    /// `input.revision`. Answers as [`Reads::source`](crate::Reads::source)
    /// does, with the saved file.
    fn edit_record(&self, id: &str, input: RecordSourceEdit) -> Reply;
}

/// Run the mutation `operation` names, one of [`operations`], on the record
/// `target` names, with `input` as the JSON object of its options.
///
/// `input` is what the matching POST endpoint takes as its body, and it is
/// checked the same way: an unknown option or a missing required one is
/// refused before the mutation runs. An unknown operation, a missing target
/// or a target on an operation that takes none is refused too.
pub fn apply(operation: &str, target: Option<&str>, input: &[u8], mutations: &dyn Mutations) -> Reply {
    let Some(&(_, needs_target, write)) = OPERATIONS.iter().find(|(name, ..)| *name == operation) else {
        return Reply::invalid_request(&format!("unknown operation `{operation}`"));
    };
    let id = match (needs_target, target) {
        (true, Some(id)) if !id.is_empty() => id.to_owned(),
        (true, _) => return Reply::invalid_request(&format!("`{operation}` needs a target")),
        (false, None) => String::new(),
        (false, Some(_)) => return Reply::invalid_request(&format!("`{operation}` takes no target")),
    };
    let write = write(id);
    mutate(write, input, mutations)
}

/// The operations [`apply`] runs, each with whether it names a target
/// record. Each runs the [`Mutations`] method of the same name, except
/// `update_source`, which runs [`Mutations::edit_record`].
pub fn operations() -> impl Iterator<Item = (&'static str, bool)> {
    OPERATIONS.iter().map(|&(name, needs_target, _)| (name, needs_target))
}

/// The body of a mutation that takes no options: an empty object.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct NoOptions {}

/// Parse a mutation's body and run it.
pub(crate) fn mutate(write: Write, body: &[u8], mutations: &dyn Mutations) -> Reply {
    match write {
        Write::Create => with_input(body, |input| mutations.create_task(input)),
        Write::Update(id) => with_input(body, |input| mutations.update_task(&id, input)),
        Write::Start(id) => with_input(body, |input| mutations.start_task(&id, input)),
        Write::Park(id) => with_input(body, |NoOptions {}| mutations.park_task(&id)),
        Write::Unpark(id) => with_input(body, |NoOptions {}| mutations.unpark_task(&id)),
        Write::Handoff(id) => with_input(body, |input| mutations.hand_off_task(&id, input)),
        Write::Complete(id) => with_input(body, |input| mutations.complete_task(&id, input)),
        Write::CreateCapture => with_input(body, |input| mutations.create_capture(input)),
        Write::PromoteCapture(id) => with_input(body, |input| mutations.promote_capture(&id, input)),
        Write::DiscardCapture(id) => with_input(body, |NoOptions {}| mutations.discard_capture(&id)),
        Write::CreateSpec => with_input(body, |input| mutations.create_spec(input)),
        Write::CreatePlan => with_input(body, |input| mutations.create_plan(input)),
        Write::ApplyPlan(id) => with_input(body, |input| mutations.apply_plan(&id, input)),
        Write::EditRecord(id) => with_input(body, |input| mutations.edit_record(&id, input)),
    }
}

/// Parse `body` as the JSON object of a command's options and pass them to
/// `run`. An empty body is an empty object.
fn with_input<T: DeserializeOwned>(body: &[u8], run: impl FnOnce(T) -> Reply) -> Reply {
    let body = if body.iter().all(u8::is_ascii_whitespace) { b"{}".as_slice() } else { body };
    match serde_json::from_slice::<T>(body) {
        Ok(input) => run(input),
        Err(error) => Reply::invalid_request(&format!("the request body is not valid: {error}")),
    }
}
