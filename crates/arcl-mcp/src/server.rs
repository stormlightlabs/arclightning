//! The MCP server and its tools.
//!
//! Each tool calls the application operations on [`Database`] directly, under
//! the workspace sync in [`crate::project`], and returns the CLI's JSON
//! envelope as structured content. Each tool's `_meta` names the
//! `format_version` its result carries as `arcl/format_version`; `show`
//! returns version 2 for a task and 1 for any other record, as the CLI does.

use std::cell::OnceCell;
use std::path::PathBuf;
use std::time::Duration;

use rmcp::{
    Json, ServerHandler,
    handler::server::router::tool::ToolRouter,
    model::{Implementation, MetaObject, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router,
};
use tokio_util::sync::CancellationToken;

use arcl_core::prelude::*;
use arcl_store::prelude::*;

use crate::error::ToolError;
use crate::params::Parameters;
use crate::project::{self, Access};
use crate::tools::*;
// The MCP input enum, which the store prelude's `OpenSubtasks` would otherwise make ambiguous.
use crate::tools::OpenSubtasks;

/// The `_meta` key that names the `format_version` a tool's result carries.
pub const FORMAT_VERSION_META_KEY: &str = "arcl/format_version";

/// A read tool's description, ending with the sentence that qualifies its
/// `read_only_hint`: the call changes no records, but with the workspace on
/// it syncs first, as a command does.
macro_rules! read_description {
    ($text:literal) => {
        concat!(
            $text,
            " With the workspace on, the call first imports workspace changes into the database and exports \
            the result, as a command does."
        )
    };
}

const INSTRUCTIONS: &str = "Arc Lightning tracks this project's tasks. Find work with `ready`, \
claim it with `task_start`, read `context`, leave notes on decisions and findings with \
`task_note`, make a task wait on a pull request, run or date with `gate_add`, then finish with \
`task_complete` or park it with `task_handoff`. `explain` says why a task is not ready. Record work \
outside the task with `capture_create`. Every result is `{format_version, data}`; check the version before reading data.";

/// The Arc Lightning MCP server for the project that holds its directory.
#[derive(Clone, Debug)]
pub struct ArclServer {
    start: PathBuf,
    lock_wait: Duration,
    tool_router: ToolRouter<Self>,
}

impl ArclServer {
    /// A server for the project that holds `start` or one of its ancestors.
    /// Each call finds the project again, so a project initialized after the
    /// server starts is picked up.
    pub fn new(start: PathBuf) -> Self {
        Self { start, lock_wait: project::LOCK_WAIT, tool_router: Self::tool_router() }
    }

    /// The same server with calls waiting at most `lock_wait` for the sync
    /// lock instead of [`project::LOCK_WAIT`].
    pub fn with_lock_wait(mut self, lock_wait: Duration) -> Self {
        self.lock_wait = lock_wait;
        self
    }

    /// The tools this server lists.
    pub fn tools(&self) -> Vec<rmcp::model::Tool> {
        self.tool_router.list_all()
    }

    /// Run `operation` on a blocking thread with the project's sync around
    /// it, and wrap its value in an envelope of `format_version`.
    async fn call<T, F>(
        &self, access: Access, format_version: u8, cancelled: CancellationToken, operation: F,
    ) -> Result<Json<Envelope<T>>, ToolError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Database) -> Result<T, ToolError> + Send + 'static,
    {
        let (data, warnings) = self.run(access, cancelled, operation).await?;
        Ok(Json(Envelope { format_version, data, warnings }))
    }

    /// Run `operation` on a blocking thread. The call stops before
    /// `operation` starts if the client cancels the request, or if this future
    /// is dropped, as a client-side timeout does.
    async fn run<T, F>(
        &self, access: Access, cancelled: CancellationToken, operation: F,
    ) -> Result<(T, Vec<project::Warning>), ToolError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Database) -> Result<T, ToolError> + Send + 'static,
    {
        let start = self.start.clone();
        let lock_wait = self.lock_wait;
        let token = cancelled.child_token();
        let _cancel_on_drop = token.clone().drop_guard();
        tokio::task::spawn_blocking(move || project::run(&start, access, lock_wait, &token, operation))
            .await
            .map_err(|error| ToolError::new(1, format!("the tool call stopped: {error}")))?
    }
}

#[tool_router(router = tool_router)]
impl ArclServer {
    /// Show one record by ID, like `arcl show`.
    #[tool(
        description = read_description!("Show one record by ID, like `arcl show`. A task returns its readiness view \
        (format_version 2); any other record returns {action: \"shown\", <kind>: record} (format_version 1)."),
        annotations(read_only_hint = true, open_world_hint = false),
        meta = version_meta(READINESS_FORMAT_VERSION)
    )]
    pub async fn show(
        &self, Parameters(input): Parameters<RecordInput>, cancelled: CancellationToken,
    ) -> Result<Json<Envelope<ShowData>>, ToolError> {
        let input = input?;
        let ((format_version, data), warnings) = self
            .run(Access::Read, cancelled, move |database| {
                show_record(database, &input.id)
            })
            .await?;
        Ok(Json(Envelope { format_version, data, warnings }))
    }

    /// List record summaries, like `arcl list`.
    #[tool(
        description = read_description!("List record summaries ordered by kind, then ID, like `arcl list`. Filter by kind \
        and status. Returns at most `limit` records (1 to 200, default 50) and a `next_cursor` while more remain."),
        annotations(read_only_hint = true, open_world_hint = false),
        meta = version_meta(FORMAT_VERSION)
    )]
    pub async fn list(
        &self, Parameters(input): Parameters<ListInput>, cancelled: CancellationToken,
    ) -> Result<Json<Envelope<ListData>>, ToolError> {
        let input = input?;
        self.call(Access::Read, FORMAT_VERSION, cancelled, move |database| {
            list_records(database, input)
        })
        .await
    }

    /// List the tasks that can start, like `arcl ready`.
    #[tool(
        description = read_description!("List the tasks that can start, in order, like `arcl ready`. Returns at most \
        `limit` tasks (1 to 200, default 50) and a `next_cursor` while more remain."),
        annotations(read_only_hint = true, open_world_hint = false),
        meta = version_meta(FORMAT_VERSION)
    )]
    pub async fn ready(
        &self, Parameters(input): Parameters<ReadyInput>, cancelled: CancellationToken,
    ) -> Result<Json<Envelope<ReadyData>>, ToolError> {
        let input = input?;
        self.call(Access::Read, FORMAT_VERSION, cancelled, move |database| {
            ready_tasks(database, input)
        })
        .await
    }

    /// Explain a task's readiness, like `arcl explain`.
    #[tool(
        description = read_description!("Explain whether a task is ready and why not, like `arcl explain`. \
        `data.readiness.reasons` lists typed reasons."),
        annotations(read_only_hint = true, open_world_hint = false),
        meta = version_meta(READINESS_FORMAT_VERSION)
    )]
    pub async fn explain(
        &self, Parameters(input): Parameters<TaskInput>, cancelled: CancellationToken,
    ) -> Result<Json<Envelope<PlanningTaskView>>, ToolError> {
        let input = input?;
        self.call(Access::Read, READINESS_FORMAT_VERSION, cancelled, move |database| {
            Ok(database.planning_task_view(record_id::<TaskId>(database, &input.task_id)?)?)
        })
        .await
    }

    /// Return a task's context packet, like `arcl context`.
    #[tool(
        description = read_description!("Return a task with its ancestors, spec, plan, phase, blockers, dependents, \
        readiness, earlier evidence and the most recent notes, like `arcl context`. `data.task.handoff` holds the \
        last resume note; `data.omitted_notes` counts older notes, which `show` lists."),
        annotations(read_only_hint = true, open_world_hint = false),
        meta = version_meta(READINESS_FORMAT_VERSION)
    )]
    pub async fn context(
        &self, Parameters(input): Parameters<TaskInput>, cancelled: CancellationToken,
    ) -> Result<Json<Envelope<PlanningContext>>, ToolError> {
        let input = input?;
        self.call(Access::Read, READINESS_FORMAT_VERSION, cancelled, move |database| {
            Ok(database.planning_context(record_id::<TaskId>(database, &input.task_id)?)?)
        })
        .await
    }

    /// Capture work to do later, like `arcl capture create`.
    #[tool(
        description = "Capture work found outside the current task, like `arcl capture create`.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        meta = version_meta(FORMAT_VERSION)
    )]
    pub async fn capture_create(
        &self, Parameters(input): Parameters<CaptureCreateInput>, cancelled: CancellationToken,
    ) -> Result<Json<Envelope<CaptureData>>, ToolError> {
        let input = input?;
        self.call(Access::Write, FORMAT_VERSION, cancelled, move |database| {
            validate_title(&input.title)?;
            let capture = database.create_capture(input.title, input.body)?;
            Ok(CaptureData { action: "created".to_owned(), capture })
        })
        .await
    }

    /// Promote a capture to a spec, task or note, like `arcl capture promote`.
    #[tool(
        description = "Promote a capture to a spec, task or note, like `arcl capture promote`. The new \
        record takes the capture's title and body unless the call gives others. `spec`, `plan`, `phase` \
        and `parent` apply only to a task.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false),
        meta = version_meta(FORMAT_VERSION)
    )]
    pub async fn capture_promote(
        &self, Parameters(input): Parameters<CapturePromoteInput>, cancelled: CancellationToken,
    ) -> Result<Json<Envelope<CapturePromotionData>>, ToolError> {
        let input = input?;
        self.call(Access::Write, FORMAT_VERSION, cancelled, move |database| {
            promote_capture(database, input)
        })
        .await
    }

    /// Create a spec, like `arcl spec create`.
    #[tool(
        description = "Create a spec, like `arcl spec create`.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        meta = version_meta(FORMAT_VERSION)
    )]
    pub async fn spec_create(
        &self, Parameters(input): Parameters<SpecCreateInput>, cancelled: CancellationToken,
    ) -> Result<Json<Envelope<SpecData>>, ToolError> {
        let input = input?;
        self.call(Access::Write, FORMAT_VERSION, cancelled, move |database| {
            validate_title(&input.title)?;
            let spec = database.create_spec(input.title, input.body, input.acceptance_criteria)?;
            Ok(SpecData { action: "created".to_owned(), spec })
        })
        .await
    }

    /// Create a plan under a spec, like `arcl plan create`.
    #[tool(
        description = "Create a plan under a spec, like `arcl plan create`.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        meta = version_meta(FORMAT_VERSION)
    )]
    pub async fn plan_create(
        &self, Parameters(input): Parameters<PlanCreateInput>, cancelled: CancellationToken,
    ) -> Result<Json<Envelope<PlanData>>, ToolError> {
        let input = input?;
        self.call(Access::Write, FORMAT_VERSION, cancelled, move |database| {
            validate_title(&input.title)?;
            let spec_id = record_id::<SpecId>(database, &input.spec)?;
            let plan = database.create_plan(spec_id, input.title, input.body)?;
            Ok(PlanData { action: "created".to_owned(), plan })
        })
        .await
    }

    /// Create a task, like `arcl task create`.
    #[tool(
        description = "Create a task, like `arcl task create`, optionally under a spec, plan, phase \
        or parent task and blocked by other tasks.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        meta = version_meta(FORMAT_VERSION)
    )]
    pub async fn task_create(
        &self, Parameters(input): Parameters<TaskCreateInput>, cancelled: CancellationToken,
    ) -> Result<Json<Envelope<TaskData>>, ToolError> {
        let input = input?;
        self.call(Access::Write, FORMAT_VERSION, cancelled, move |database| {
            validate_title(&input.title)?;
            let records = Records::default();
            let create = PlanningTaskCreate {
                project_id: database.project()?.id,
                spec_id: input
                    .spec
                    .as_deref()
                    .map(|id| records.parse::<SpecId>(database, id))
                    .transpose()?,
                plan_id: input
                    .plan
                    .as_deref()
                    .map(|id| records.parse::<PlanId>(database, id))
                    .transpose()?,
                phase_id: input
                    .phase
                    .as_deref()
                    .map(|id| records.parse::<PhaseId>(database, id))
                    .transpose()?,
                parent_id: input
                    .parent
                    .as_deref()
                    .map(|id| records.parse::<TaskId>(database, id))
                    .transpose()?,
                title: input.title,
                body: input.body,
                priority: input.priority,
                position: input.position,
            };
            let blockers = input
                .blocked_by
                .iter()
                .map(|blocker| records.parse::<TaskId>(database, blocker))
                .collect::<Result<Vec<_>, _>>()?;
            let task = database.create_planning_task_with_dependencies(create, &blockers)?;
            Ok(TaskData { action: "created".to_owned(), task })
        })
        .await
    }

    /// Change a task's title, body, priority or position, like `arcl task update`.
    #[tool(
        description = "Change a task's title, body, priority or position, like `arcl task update`. \
        Fields left out stay as they are.",
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = false),
        meta = version_meta(FORMAT_VERSION)
    )]
    pub async fn task_update(
        &self, Parameters(input): Parameters<TaskUpdateInput>, cancelled: CancellationToken,
    ) -> Result<Json<Envelope<TaskData>>, ToolError> {
        let input = input?;
        self.call(Access::Write, FORMAT_VERSION, cancelled, move |database| {
            let id = record_id::<TaskId>(database, &input.task_id)?;
            if let Some(title) = &input.title {
                validate_title(title)?;
            }
            let update = PlanningTaskUpdate {
                title: input.title,
                body: input.body,
                priority: input.priority,
                position: input.position,
                ..PlanningTaskUpdate::default()
            };
            let task = database.update_planning_task(id, update)?;
            Ok(TaskData { action: "updated".to_owned(), task })
        })
        .await
    }

    /// Create a note, like `arcl note create`.
    #[tool(
        description = "Create a note, like `arcl note create`.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        meta = version_meta(FORMAT_VERSION)
    )]
    pub async fn note_create(
        &self, Parameters(input): Parameters<NoteCreateInput>, cancelled: CancellationToken,
    ) -> Result<Json<Envelope<NoteData>>, ToolError> {
        let input = input?;
        self.call(Access::Write, FORMAT_VERSION, cancelled, move |database| {
            validate_title(&input.title)?;
            let note = database.create_note(input.title, input.body)?;
            Ok(NoteData { action: "created".to_owned(), note })
        })
        .await
    }

    /// Start a ready task, like `arcl task start` without `--force`.
    #[tool(
        description = "Move a ready task to in progress, like `arcl task start`. A task that is not \
        ready fails with `invalid_request`; read the reasons with `explain`. There is no force option. \
        `claim` sets the task's assignee in the same write. A task an assignee holds fails with `conflict` \
        unless `claim` names that assignee.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        meta = version_meta(FORMAT_VERSION)
    )]
    pub async fn task_start(
        &self, Parameters(input): Parameters<TaskStartInput>, cancelled: CancellationToken,
    ) -> Result<Json<Envelope<TaskData>>, ToolError> {
        let input = input?;
        self.call(Access::Write, FORMAT_VERSION, cancelled, move |database| {
            let id = record_id::<TaskId>(database, &input.task_id)?;
            let task = match &input.claim {
                Some(name) => database.claim_planning_task(id, false, name)?,
                None => database.start_planning_task(id, false)?,
            };
            Ok(TaskData { action: "started".to_owned(), task })
        })
        .await
    }

    /// Leave a resume note and park the task, like `arcl task handoff`.
    #[tool(
        description = "Leave a resume note on a task and park it, like `arcl task handoff`. \
        The next session resumes it with `task_unpark`, then `task_start`.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        meta = version_meta(FORMAT_VERSION)
    )]
    pub async fn task_handoff(
        &self, Parameters(input): Parameters<TaskHandoffInput>, cancelled: CancellationToken,
    ) -> Result<Json<Envelope<TaskData>>, ToolError> {
        let input = input?;
        self.call(Access::Write, FORMAT_VERSION, cancelled, move |database| {
            let task = database.handoff_planning_task(record_id::<TaskId>(database, &input.task_id)?, input.note)?;
            Ok(TaskData { action: "handed_off".to_owned(), task })
        })
        .await
    }

    /// Append a timestamped note to a task, like `arcl task note`.
    #[tool(
        description = "Append a timestamped note to a task, like `arcl task note`, to record a decision, \
        observation, blocker or result while working. Notes are kept in order, and arcl only appends \
        them; editing or deleting one in the task file and syncing changes or removes it. A task in any \
        status takes them. Empty text, text over 2000 characters, text with control or format characters \
        other than newline and tab, or a line that is exactly `+++` fails with `invalid_request`.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        meta = version_meta(FORMAT_VERSION)
    )]
    pub async fn task_note(
        &self, Parameters(input): Parameters<TaskNoteInput>, cancelled: CancellationToken,
    ) -> Result<Json<Envelope<TaskNoteData>>, ToolError> {
        let input = input?;
        self.call(Access::Write, FORMAT_VERSION, cancelled, move |database| {
            let id = record_id::<TaskId>(database, &input.task_id)?;
            let note = database.add_task_note(id, input.kind, &input.text)?;
            Ok(TaskNoteData { action: "noted".to_owned(), note })
        })
        .await
    }

    /// Add an open gate to a task, like `arcl gate add`.
    #[tool(
        description = "Make a task wait on a pull request merging, a GitHub Actions run succeeding or a date \
        arriving, like `arcl gate add`. The task is not ready while the gate is open; `gate_check` marks it met. \
        A target of the wrong shape, or a completed or cancelled task, fails with `invalid_request`.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        meta = version_meta(FORMAT_VERSION)
    )]
    pub async fn gate_add(
        &self, Parameters(input): Parameters<GateAddInput>, cancelled: CancellationToken,
    ) -> Result<Json<Envelope<GateData>>, ToolError> {
        let input = input?;
        self.call(Access::Write, FORMAT_VERSION, cancelled, move |database| {
            let id = record_id::<TaskId>(database, &input.task_id)?;
            let gate = database.add_gate(id, input.kind, &input.target)?;
            Ok(GateData { action: "added".to_owned(), gate })
        })
        .await
    }

    /// List gates on one task or on every task, like `arcl gate list`.
    #[tool(
        description = read_description!("List the gates on one task, or on every task, open and met, like \
        `arcl gate list`."),
        annotations(read_only_hint = true, open_world_hint = false),
        meta = version_meta(FORMAT_VERSION)
    )]
    pub async fn gate_list(
        &self, Parameters(input): Parameters<GateScopeInput>, cancelled: CancellationToken,
    ) -> Result<Json<Envelope<GateListData>>, ToolError> {
        let input = input?;
        self.call(Access::Read, FORMAT_VERSION, cancelled, move |database| {
            let task = input
                .task_id
                .as_deref()
                .map(|id| record_id::<TaskId>(database, id))
                .transpose()?;
            Ok(GateListData { gates: database.gates(task)? })
        })
        .await
    }

    /// Check open gates and mark met ones, like `arcl gate check`.
    #[tool(
        description = "Check the open gates on one task, or on every task, and mark the met ones, like \
        `arcl gate check`. Date gates compare with today's UTC date. Pull request and run gates ask the GitHub \
        CLI, `gh`; without it they are `skipped` and `gh_missing` is true. One gate's error is reported as its \
        `error` outcome and never fails the call.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        meta = version_meta(FORMAT_VERSION)
    )]
    pub async fn gate_check(
        &self, Parameters(input): Parameters<GateScopeInput>, cancelled: CancellationToken,
    ) -> Result<Json<Envelope<GateCheckData>>, ToolError> {
        let input = input?;
        self.call(Access::Write, FORMAT_VERSION, cancelled, move |database| {
            let task = input
                .task_id
                .as_deref()
                .map(|id| record_id::<TaskId>(database, id))
                .transpose()?;
            let report = arcl_repo::gate::check_gates(database, task, &today_utc())?;
            Ok(GateCheckData { action: "checked".to_owned(), checks: report.checks, gh_missing: report.gh_missing })
        })
        .await
    }

    /// Return a parked task to pending, like `arcl task unpark`.
    #[tool(
        description = "Return a parked task to pending, like `arcl task unpark`.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        meta = version_meta(FORMAT_VERSION)
    )]
    pub async fn task_unpark(
        &self, Parameters(input): Parameters<TaskInput>, cancelled: CancellationToken,
    ) -> Result<Json<Envelope<TaskData>>, ToolError> {
        let input = input?;
        self.call(Access::Write, FORMAT_VERSION, cancelled, move |database| {
            let task = database.transition_planning_task(
                record_id::<TaskId>(database, &input.task_id)?,
                TaskAction::Unpark,
                false,
            )?;
            Ok(TaskData { action: "unparked".to_owned(), task })
        })
        .await
    }

    /// Complete a task with evidence, like `arcl task complete`.
    #[tool(
        description = "Complete a task with evidence, like `arcl task complete`. `data.unblocked` \
        lists the tasks this made ready.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        meta = version_meta(READINESS_FORMAT_VERSION)
    )]
    pub async fn task_complete(
        &self, Parameters(input): Parameters<TaskCompleteInput>, cancelled: CancellationToken,
    ) -> Result<Json<Envelope<CompletionData>>, ToolError> {
        let input = input?;
        self.call(Access::Write, READINESS_FORMAT_VERSION, cancelled, move |database| {
            let open_subtasks = match input.open_subtasks {
                OpenSubtasks::Refuse => arcl_store::OpenSubtasks::Refuse,
                OpenSubtasks::Leave => arcl_store::OpenSubtasks::Leave,
                OpenSubtasks::Complete => arcl_store::OpenSubtasks::Complete,
            };
            let completion = database.complete_planning_task(
                record_id::<TaskId>(database, &input.task_id)?,
                open_subtasks,
                input.evidence,
            )?;
            Ok(CompletionData {
                action: "completed".to_owned(),
                task: completion.task,
                completed_subtasks: completion.completed_subtasks.iter().map(|task| task.id).collect(),
                unblocked: completion.unblocked.iter().map(|task| task.id).collect(),
            })
        })
        .await
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for ArclServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("arcl-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}

/// The records one tool call resolves short ID arguments against, read on
/// first use so a call with several IDs reads them once.
#[derive(Default)]
struct Records(OnceCell<Vec<RecordRef>>);

impl Records {
    /// Parse a record ID argument of kind `T`. A full ID parses as it is; a
    /// short ID, such as `535B` or `t-535B`, resolves among records of kind
    /// `T` first, unless it names a kind.
    fn parse<T: PrefixedId>(&self, database: &Database, input: &str) -> Result<T, ToolError> {
        if is_full_id(input) {
            return Ok(T::parse(input)?);
        }
        let records = match self.0.get() {
            Some(records) => records,
            None => {
                let read = database.record_refs()?;
                self.0.get_or_init(|| read)
            }
        };
        let record = resolve_short_id(input, records, Some(T::PREFIX)).map_err(StorageError::from)?;
        Ok(T::parse(&record.id)?)
    }
}

/// The `_meta` object naming the `format_version` a tool's result carries.
fn version_meta(format_version: u8) -> MetaObject {
    let mut meta = serde_json::Map::new();
    meta.insert(FORMAT_VERSION_META_KEY.to_owned(), format_version.into());
    MetaObject(meta)
}

/// Promote a capture as `arcl capture promote` does: the capture's title and
/// body unless the input gives others, and task placement only for a task.
/// The checks run in the CLI's order and with its rules, so a call both reject
/// fails the same way: the capture ID, a target given, the capture's
/// existence, the target's kind, then for a spec or note no task placement,
/// or for a task the priority and placement IDs, then the store's own checks,
/// title included.
fn promote_capture(database: &mut Database, input: CapturePromoteInput) -> Result<CapturePromotionData, ToolError> {
    let records = Records::default();
    let id = records.parse::<CaptureId>(database, &input.capture_id)?;
    let target = input
        .target
        .ok_or_else(|| ToolError::invalid_request("capture promotion requires a target: spec, task, or note"))?;
    let capture = database
        .capture(id)?
        .ok_or_else(|| StorageError::CaptureNotFound { id: id.to_string() })?;
    let title = input.title.unwrap_or(capture.title);
    let body = input.body.unwrap_or(capture.body);
    let placed = input.spec.is_some() || input.plan.is_some() || input.phase.is_some() || input.parent.is_some();
    let misplaced = || ToolError::invalid_request("task placement options apply only when promoting to a task");
    let result = match CapturePromotionKind::parse(&target) {
        Some(CapturePromotionKind::Spec) => {
            if placed {
                return Err(misplaced());
            }
            database.promote_capture_to_spec(id, title, body, input.acceptance_criteria.unwrap_or_default())?
        }
        Some(CapturePromotionKind::Task) => {
            let priority = input
                .priority
                .as_deref()
                .map_or(Ok(TaskPriority::Normal), TaskPriority::parse)?;
            database.promote_capture_to_task(
                id,
                CaptureTaskPromotion {
                    spec_id: input
                        .spec
                        .as_deref()
                        .map(|id| records.parse::<SpecId>(database, id))
                        .transpose()?,
                    plan_id: input
                        .plan
                        .as_deref()
                        .map(|id| records.parse::<PlanId>(database, id))
                        .transpose()?,
                    phase_id: input
                        .phase
                        .as_deref()
                        .map(|id| records.parse::<PhaseId>(database, id))
                        .transpose()?,
                    parent_id: input
                        .parent
                        .as_deref()
                        .map(|id| records.parse::<TaskId>(database, id))
                        .transpose()?,
                    title,
                    body,
                    priority,
                    position: input.position,
                },
            )?
        }
        Some(CapturePromotionKind::Note) => {
            if placed {
                return Err(misplaced());
            }
            database.promote_capture_to_note(id, title, body)?
        }
        None => {
            return Err(ToolError::invalid_request(format!(
                "unknown capture promotion target `{}`; use spec, task, or note",
                target.to_ascii_lowercase()
            )));
        }
    };
    let record = match result.record {
        CapturePromotionRecord::Spec(spec) => PromotedRecord::Spec(spec),
        CapturePromotionRecord::Task(task) => PromotedRecord::Task(task),
        CapturePromotionRecord::Note(note) => PromotedRecord::Note(note),
    };
    Ok(CapturePromotionData { capture: result.capture, promotion: result.promotion, record })
}

/// Look a record up by its ID prefix. Returns the envelope version with it.
fn show_record(database: &mut Database, id: &str) -> Result<(u8, ShowData), ToolError> {
    let resolved = database.resolve_record_id(id, None)?;
    let id = resolved.as_str();
    let shown = || "shown".to_owned();
    if id.starts_with(TaskId::PREFIX) {
        let view = database.planning_task_view(TaskId::parse(id)?)?;
        return Ok((READINESS_FORMAT_VERSION, ShowData::Task(Box::new(view))));
    }
    let data = if id.starts_with(CaptureId::PREFIX) {
        let capture = database
            .capture(CaptureId::parse(id)?)?
            .ok_or_else(|| StorageError::CaptureNotFound { id: id.to_owned() })?;
        ShowData::Capture(CaptureData { action: shown(), capture })
    } else if id.starts_with(SpecId::PREFIX) {
        let spec = database
            .spec(SpecId::parse(id)?)?
            .ok_or_else(|| StorageError::SpecNotFound { id: id.to_owned() })?;
        ShowData::Spec(SpecData { action: shown(), spec })
    } else if id.starts_with(PlanId::PREFIX) {
        let plan = database
            .plan(PlanId::parse(id)?)?
            .ok_or_else(|| StorageError::PlanNotFound { id: id.to_owned() })?;
        ShowData::Plan(PlanData { action: shown(), plan })
    } else if id.starts_with(PhaseId::PREFIX) {
        let phase = database
            .phase(PhaseId::parse(id)?)?
            .ok_or_else(|| StorageError::PhaseNotFound { id: id.to_owned() })?;
        ShowData::Phase(PhaseData { action: shown(), phase })
    } else if id.starts_with(NoteId::PREFIX) {
        let note = database
            .note(NoteId::parse(id)?)?
            .ok_or_else(|| StorageError::NoteNotFound { id: id.to_owned() })?;
        ShowData::Note(NoteData { action: shown(), note })
    } else if id.starts_with(ReleaseId::PREFIX) {
        let release = database
            .release(ReleaseId::parse(id)?)?
            .ok_or_else(|| StorageError::ReleaseNotFound { id: id.to_owned() })?;
        ShowData::Release(ReleaseData { action: shown(), release })
    } else {
        return Err(ToolError::invalid_request(format!("unknown record ID `{id}`")));
    };
    Ok((FORMAT_VERSION, data))
}

/// Parse a tool call's one record ID argument of kind `T`, as
/// [`Records::parse`] does.
fn record_id<T: PrefixedId>(database: &Database, input: &str) -> Result<T, ToolError> {
    Records::default().parse(database, input)
}

/// One page of record summaries matching `input`, ordered by kind, then ID.
/// The cursor is the `<kind>:<id>` of the last record on the previous page,
/// so records added before it do not shift later pages.
fn list_records(database: &mut Database, input: ListInput) -> Result<ListData, ToolError> {
    let limit = page_limit(input.limit)?;
    if let Some(kind) = input.kind
        && let Some(status) = input.status.iter().find(|status| !status_applies(kind, **status))
    {
        return Err(ToolError::invalid_request(format!(
            "status `{}` does not apply to {} records",
            status.as_str(),
            kind.as_str()
        )));
    }
    let after = input.cursor.as_deref().map(list_cursor).transpose()?;

    let wanted = |kind: RecordKind| input.kind.is_none_or(|wanted| wanted == kind);
    let mut records = Vec::new();
    let mut push = |kind: RecordKind, id: String, title: &str, status: &str| {
        records.push(RecordSummary {
            kind: kind.as_str().to_owned(),
            id,
            title: title.to_owned(),
            status: status.to_owned(),
        });
    };
    if wanted(RecordKind::Capture) {
        for capture in database.captures()? {
            push(
                RecordKind::Capture,
                capture.id.to_string(),
                &capture.title,
                capture.status.as_str(),
            );
        }
    }
    if wanted(RecordKind::Release) {
        for release in database.releases()? {
            push(
                RecordKind::Release,
                release.id.to_string(),
                &release.title,
                release.status.as_str(),
            );
        }
    }
    if wanted(RecordKind::Spec) {
        for spec in database.specs()? {
            push(RecordKind::Spec, spec.id.to_string(), &spec.title, spec.status.as_str());
        }
    }
    if wanted(RecordKind::Plan) {
        for plan in database.plans()? {
            push(RecordKind::Plan, plan.id.to_string(), &plan.title, plan.status.as_str());
        }
    }
    if wanted(RecordKind::Phase) {
        for phase in database.phases()? {
            push(
                RecordKind::Phase,
                phase.id.to_string(),
                &phase.title,
                phase.status.as_str(),
            );
        }
    }
    if wanted(RecordKind::Task) {
        for task in database.planning_tasks()? {
            push(RecordKind::Task, task.id.to_string(), &task.title, task.status.as_str());
        }
    }
    if wanted(RecordKind::Note) {
        for note in database.notes()? {
            push(RecordKind::Note, note.id.to_string(), &note.title, "available");
        }
    }
    records
        .retain(|record| input.status.is_empty() || input.status.iter().any(|status| status.as_str() == record.status));
    records.sort_by(|left, right| left.kind.cmp(&right.kind).then_with(|| left.id.cmp(&right.id)));
    if let Some((kind, id)) = after {
        records.retain(|record| (record.kind.as_str(), record.id.as_str()) > (kind.as_str(), id));
    }
    let next_cursor = (records.len() > limit).then(|| {
        let last = &records[limit - 1];
        format!("{}:{}", last.kind, last.id)
    });
    records.truncate(limit);
    Ok(ListData { records, next_cursor })
}

/// Read a `list` cursor, `<kind>:<id>`: a known kind and an ID of that kind.
fn list_cursor(cursor: &str) -> Result<(RecordKind, &str), ToolError> {
    let invalid = || ToolError::invalid_request(format!("cursor `{cursor}` is not one `list` returned"));
    let (kind, id) = cursor.split_once(':').ok_or_else(invalid)?;
    let kind = RecordKind::parse(kind).ok_or_else(invalid)?;
    let parsed = match kind {
        RecordKind::Capture => CaptureId::parse(id).map(drop),
        RecordKind::Release => ReleaseId::parse(id).map(drop),
        RecordKind::Spec => SpecId::parse(id).map(drop),
        RecordKind::Plan => PlanId::parse(id).map(drop),
        RecordKind::Phase => PhaseId::parse(id).map(drop),
        RecordKind::Task => TaskId::parse(id).map(drop),
        RecordKind::Note => NoteId::parse(id).map(drop),
    };
    parsed.map_err(|_| invalid())?;
    Ok((kind, id))
}

/// One page of ready tasks. The cursor is the ready-order key of the last
/// task on the previous page, `<priority>:<position>:<id>`, and the page
/// holds the tasks after it. A task that starts or completes between pages
/// leaves the order without shifting the tasks after it.
fn ready_tasks(database: &mut Database, input: ReadyInput) -> Result<ReadyData, ToolError> {
    let limit = page_limit(input.limit)?;
    let after = input.cursor.as_deref().map(ready_cursor).transpose()?;
    let records = Records::default();
    let filter = PlanningReadyFilter {
        priorities: input.priority,
        spec_id: input
            .spec
            .as_deref()
            .map(|id| records.parse::<SpecId>(database, id))
            .transpose()?,
        plan_id: input
            .plan
            .as_deref()
            .map(|id| records.parse::<PlanId>(database, id))
            .transpose()?,
        phase_id: input
            .phase
            .as_deref()
            .map(|id| records.parse::<PhaseId>(database, id))
            .transpose()?,
        parent_id: input
            .parent
            .as_deref()
            .map(|id| records.parse::<TaskId>(database, id))
            .transpose()?,
    };
    let mut tasks = database.ready_planning_tasks_filtered(&filter)?;
    if let Some(after) = after {
        tasks.retain(|task| ready_key(task) > after);
    }
    let next_cursor = (tasks.len() > limit).then(|| {
        let (priority, position, id) = ready_key(&tasks[limit - 1]);
        format!("{}:{position}:{id}", priority.as_str())
    });
    tasks.truncate(limit);
    Ok(ReadyData { tasks, next_cursor })
}

/// A task's place in the ready order, as `ready` sorts it.
fn ready_key(task: &PlanningTask) -> (TaskPriority, i64, TaskId) {
    (task.priority, task.position, task.id)
}

/// Read a `ready` cursor, `<priority>:<position>:<id>`.
fn ready_cursor(cursor: &str) -> Result<(TaskPriority, i64, TaskId), ToolError> {
    let invalid = || ToolError::invalid_request(format!("cursor `{cursor}` is not one `ready` returned"));
    let mut parts = cursor.splitn(3, ':');
    let (Some(priority), Some(position), Some(id)) = (parts.next(), parts.next(), parts.next()) else {
        return Err(invalid());
    };
    let priority = TaskPriority::parse(priority).map_err(|_| invalid())?;
    let position = position.parse::<i64>().map_err(|_| invalid())?;
    let id = TaskId::parse(id).map_err(|_| invalid())?;
    Ok((priority, position, id))
}

/// The page size for `limit`: [`DEFAULT_LIMIT`] when absent, and an error
/// outside 1 to [`MAX_LIMIT`].
fn page_limit(limit: Option<u32>) -> Result<usize, ToolError> {
    match limit.unwrap_or(DEFAULT_LIMIT) {
        limit @ 1..=MAX_LIMIT => Ok(limit as usize),
        limit => Err(ToolError::invalid_request(format!(
            "limit {limit} is outside 1 to {MAX_LIMIT}"
        ))),
    }
}

/// Whether records of `kind` can have `status`, as `arcl list` checks it.
fn status_applies(kind: RecordKind, status: RecordStatus) -> bool {
    use RecordStatus::*;
    match kind {
        RecordKind::Capture => matches!(status, Captured | Promoted | Discarded),
        RecordKind::Release | RecordKind::Spec | RecordKind::Plan | RecordKind::Phase => {
            matches!(status, Open | Completed | Cancelled)
        }
        RecordKind::Task => matches!(status, Pending | InProgress | Parked | Completed | Cancelled),
        RecordKind::Note => false,
    }
}
