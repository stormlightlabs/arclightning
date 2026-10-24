//! The MCP server and its tools.
//!
//! Each tool calls the application operations on [`Database`] directly, under
//! the workspace sync in [`crate::project`], and returns the CLI's JSON
//! envelope as structured content. Each tool's `_meta` names the
//! `format_version` its result carries as `arcl/format_version`; `show`
//! returns version 2 for a task and 1 for any other record, as the CLI does.

use std::path::PathBuf;

use rmcp::{
    Json, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, MetaObject, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router,
};

use arcl_core::domain::{CaptureId, NoteId, PhaseId, PlanId, ReleaseId, SpecId, TaskAction, TaskId, validate_title};
use arcl_store::{
    Database, PlanningContext, PlanningReadyFilter, PlanningTaskCreate, PlanningTaskUpdate, StorageError,
};

use crate::error::ToolError;
use crate::project::{self, Access};
use crate::tools::*;

/// The `_meta` key that names the `format_version` a tool's result carries.
pub const FORMAT_VERSION_META_KEY: &str = "arcl/format_version";

const INSTRUCTIONS: &str = "Arc Lightning tracks this project's tasks. Find work with `ready`, \
claim it with `task_start`, read `context`, then finish with `task_complete` or park it with \
`task_handoff`. `explain` says why a task is not ready. Record work outside the task with \
`capture_create`. Every result is `{format_version, data}`; check the version before reading data.";

/// The Arc Lightning MCP server for the project that holds its directory.
#[derive(Clone, Debug)]
pub struct ArclServer {
    start: PathBuf,
    tool_router: ToolRouter<Self>,
}

impl ArclServer {
    /// A server for the project that holds `start` or one of its ancestors.
    /// Each call finds the project again, so a project initialized after the
    /// server starts is picked up.
    pub fn new(start: PathBuf) -> Self {
        Self { start, tool_router: Self::tool_router() }
    }

    /// The tools this server lists.
    pub fn tools(&self) -> Vec<rmcp::model::Tool> {
        self.tool_router.list_all()
    }

    /// Run `operation` on a blocking thread with the project's sync around
    /// it, and wrap its value in an envelope of `format_version`.
    async fn call<T, F>(&self, access: Access, format_version: u8, operation: F) -> Result<Json<Envelope<T>>, ToolError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Database) -> Result<T, ToolError> + Send + 'static,
    {
        let (data, warnings) = self.run(access, operation).await?;
        Ok(Json(Envelope { format_version, data, warnings }))
    }

    async fn run<T, F>(&self, access: Access, operation: F) -> Result<(T, Vec<project::Warning>), ToolError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Database) -> Result<T, ToolError> + Send + 'static,
    {
        let start = self.start.clone();
        tokio::task::spawn_blocking(move || project::run(&start, access, operation))
            .await
            .map_err(|error| ToolError::new(1, format!("the tool call stopped: {error}")))?
    }
}

#[tool_router(router = tool_router)]
impl ArclServer {
    /// Show one record by ID, like `arcl show`.
    #[tool(
        description = "Show one record by ID, like `arcl show`. A task returns its readiness view \
        (format_version 2); any other record returns {action: \"shown\", <kind>: record} (format_version 1).",
        annotations(read_only_hint = true, open_world_hint = false),
        meta = version_meta(READINESS_FORMAT_VERSION)
    )]
    pub async fn show(
        &self, Parameters(input): Parameters<RecordInput>,
    ) -> Result<Json<Envelope<ShowData>>, ToolError> {
        let ((format_version, data), warnings) = self
            .run(Access::Read, move |database| show_record(database, &input.id))
            .await?;
        Ok(Json(Envelope { format_version, data, warnings }))
    }

    /// List record summaries, like `arcl list`.
    #[tool(
        description = "List record summaries ordered by kind, then ID, like `arcl list`. Filter by kind \
        and status. Returns at most `limit` records (1 to 200, default 50) and a `next_cursor` while more remain.",
        annotations(read_only_hint = true, open_world_hint = false),
        meta = version_meta(FORMAT_VERSION)
    )]
    pub async fn list(&self, Parameters(input): Parameters<ListInput>) -> Result<Json<Envelope<ListData>>, ToolError> {
        self.call(Access::Read, FORMAT_VERSION, move |database| {
            list_records(database, input)
        })
        .await
    }

    /// List the tasks that can start, like `arcl ready`.
    #[tool(
        description = "List the tasks that can start, in order, like `arcl ready`. Returns at most \
        `limit` tasks (1 to 200, default 50) and a `next_cursor` while more remain.",
        annotations(read_only_hint = true, open_world_hint = false),
        meta = version_meta(FORMAT_VERSION)
    )]
    pub async fn ready(
        &self, Parameters(input): Parameters<ReadyInput>,
    ) -> Result<Json<Envelope<ReadyData>>, ToolError> {
        self.call(Access::Read, FORMAT_VERSION, move |database| {
            ready_tasks(database, input)
        })
        .await
    }

    /// Explain a task's readiness, like `arcl explain`.
    #[tool(
        description = "Explain whether a task is ready and why not, like `arcl explain`. \
        `data.readiness.reasons` lists typed reasons.",
        annotations(read_only_hint = true, open_world_hint = false),
        meta = version_meta(READINESS_FORMAT_VERSION)
    )]
    pub async fn explain(
        &self, Parameters(input): Parameters<TaskInput>,
    ) -> Result<Json<Envelope<arcl_store::PlanningTaskView>>, ToolError> {
        self.call(Access::Read, READINESS_FORMAT_VERSION, move |database| {
            Ok(database.planning_task_view(TaskId::parse(&input.task_id)?)?)
        })
        .await
    }

    /// Return a task's context packet, like `arcl context`.
    #[tool(
        description = "Return a task with its ancestors, spec, plan, phase, blockers, dependents, \
        readiness and earlier evidence, like `arcl context`. `data.task.handoff` holds the last resume note.",
        annotations(read_only_hint = true, open_world_hint = false),
        meta = version_meta(READINESS_FORMAT_VERSION)
    )]
    pub async fn context(
        &self, Parameters(input): Parameters<TaskInput>,
    ) -> Result<Json<Envelope<PlanningContext>>, ToolError> {
        self.call(Access::Read, READINESS_FORMAT_VERSION, move |database| {
            Ok(database.planning_context(TaskId::parse(&input.task_id)?)?)
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
        &self, Parameters(input): Parameters<CaptureCreateInput>,
    ) -> Result<Json<Envelope<CaptureData>>, ToolError> {
        self.call(Access::Write, FORMAT_VERSION, move |database| {
            validate_title(&input.title)?;
            let capture = database.create_capture(input.title, input.body)?;
            Ok(CaptureData { action: "created".to_owned(), capture })
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
        &self, Parameters(input): Parameters<SpecCreateInput>,
    ) -> Result<Json<Envelope<SpecData>>, ToolError> {
        self.call(Access::Write, FORMAT_VERSION, move |database| {
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
        &self, Parameters(input): Parameters<PlanCreateInput>,
    ) -> Result<Json<Envelope<PlanData>>, ToolError> {
        self.call(Access::Write, FORMAT_VERSION, move |database| {
            validate_title(&input.title)?;
            let spec_id = SpecId::parse(&input.spec)?;
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
        &self, Parameters(input): Parameters<TaskCreateInput>,
    ) -> Result<Json<Envelope<TaskData>>, ToolError> {
        self.call(Access::Write, FORMAT_VERSION, move |database| {
            validate_title(&input.title)?;
            let create = PlanningTaskCreate {
                project_id: database.project()?.id,
                spec_id: input.spec.as_deref().map(SpecId::parse).transpose()?,
                plan_id: input.plan.as_deref().map(PlanId::parse).transpose()?,
                phase_id: input.phase.as_deref().map(PhaseId::parse).transpose()?,
                parent_id: input.parent.as_deref().map(TaskId::parse).transpose()?,
                title: input.title,
                body: input.body,
                priority: input.priority,
                position: input.position,
            };
            let blockers = input
                .blocked_by
                .iter()
                .map(|blocker| TaskId::parse(blocker))
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
        &self, Parameters(input): Parameters<TaskUpdateInput>,
    ) -> Result<Json<Envelope<TaskData>>, ToolError> {
        self.call(Access::Write, FORMAT_VERSION, move |database| {
            let id = TaskId::parse(&input.task_id)?;
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
        &self, Parameters(input): Parameters<NoteCreateInput>,
    ) -> Result<Json<Envelope<NoteData>>, ToolError> {
        self.call(Access::Write, FORMAT_VERSION, move |database| {
            validate_title(&input.title)?;
            let note = database.create_note(input.title, input.body)?;
            Ok(NoteData { action: "created".to_owned(), note })
        })
        .await
    }

    /// Start a ready task, like `arcl task start` without `--force`.
    #[tool(
        description = "Move a ready task to in progress, like `arcl task start`. A task that is not \
        ready fails with `invalid_request`; read the reasons with `explain`. There is no force option.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        meta = version_meta(FORMAT_VERSION)
    )]
    pub async fn task_start(
        &self, Parameters(input): Parameters<TaskInput>,
    ) -> Result<Json<Envelope<TaskData>>, ToolError> {
        self.call(Access::Write, FORMAT_VERSION, move |database| {
            let task = database.start_planning_task(TaskId::parse(&input.task_id)?, false)?;
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
        &self, Parameters(input): Parameters<TaskHandoffInput>,
    ) -> Result<Json<Envelope<TaskData>>, ToolError> {
        self.call(Access::Write, FORMAT_VERSION, move |database| {
            let task = database.handoff_planning_task(TaskId::parse(&input.task_id)?, input.note)?;
            Ok(TaskData { action: "handed_off".to_owned(), task })
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
        &self, Parameters(input): Parameters<TaskInput>,
    ) -> Result<Json<Envelope<TaskData>>, ToolError> {
        self.call(Access::Write, FORMAT_VERSION, move |database| {
            let task = database.transition_planning_task(TaskId::parse(&input.task_id)?, TaskAction::Unpark, false)?;
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
        &self, Parameters(input): Parameters<TaskCompleteInput>,
    ) -> Result<Json<Envelope<CompletionData>>, ToolError> {
        self.call(Access::Write, READINESS_FORMAT_VERSION, move |database| {
            let open_subtasks = match input.open_subtasks {
                OpenSubtasks::Refuse => arcl_store::OpenSubtasks::Refuse,
                OpenSubtasks::Leave => arcl_store::OpenSubtasks::Leave,
                OpenSubtasks::Complete => arcl_store::OpenSubtasks::Complete,
            };
            let completion =
                database.complete_planning_task(TaskId::parse(&input.task_id)?, open_subtasks, input.evidence)?;
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

/// The `_meta` object naming the `format_version` a tool's result carries.
fn version_meta(format_version: u8) -> MetaObject {
    let mut meta = serde_json::Map::new();
    meta.insert(FORMAT_VERSION_META_KEY.to_owned(), format_version.into());
    MetaObject(meta)
}

/// Look a record up by its ID prefix. Returns the envelope version with it.
fn show_record(database: &mut Database, id: &str) -> Result<(u8, ShowData), ToolError> {
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
    let after = input
        .cursor
        .as_deref()
        .map(|cursor| {
            cursor
                .split_once(':')
                .ok_or_else(|| ToolError::invalid_request(format!("cursor `{cursor}` is not one `list` returned")))
        })
        .transpose()?;

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
        records.retain(|record| (record.kind.as_str(), record.id.as_str()) > (kind, id));
    }
    let next_cursor = (records.len() > limit).then(|| {
        let last = &records[limit - 1];
        format!("{}:{}", last.kind, last.id)
    });
    records.truncate(limit);
    Ok(ListData { records, next_cursor })
}

/// One page of ready tasks. The cursor is the offset of the next page in
/// `ready` order, which changes as tasks start and complete.
fn ready_tasks(database: &mut Database, input: ReadyInput) -> Result<ReadyData, ToolError> {
    let limit = page_limit(input.limit)?;
    let offset = match input.cursor.as_deref() {
        Some(cursor) => cursor
            .parse::<usize>()
            .map_err(|_| ToolError::invalid_request(format!("cursor `{cursor}` is not one `ready` returned")))?,
        None => 0,
    };
    let filter = PlanningReadyFilter {
        priorities: input.priority,
        spec_id: input.spec.as_deref().map(SpecId::parse).transpose()?,
        plan_id: input.plan.as_deref().map(PlanId::parse).transpose()?,
        phase_id: input.phase.as_deref().map(PhaseId::parse).transpose()?,
        parent_id: input.parent.as_deref().map(TaskId::parse).transpose()?,
    };
    let ready = database.ready_planning_tasks_filtered(&filter)?;
    let next_cursor = (ready.len() > offset.saturating_add(limit)).then(|| (offset + limit).to_string());
    let tasks = ready.into_iter().skip(offset).take(limit).collect();
    Ok(ReadyData { tasks, next_cursor })
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
