//! Tool inputs and outputs.
//!
//! Every successful tool returns the CLI's JSON envelope as structured
//! content: `{"format_version", "data", "warnings"?}`. `data` has the shape
//! the matching `arcl --json` command writes, and `format_version` is the
//! version that command reports. A change to a tool's input or output shape
//! comes with a new version, as for the CLI.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use arcl_core::domain::{
    Capture, CapturePromotion, Note, Phase, Plan, PlanningTask, Release, Spec, TaskId, TaskPriority,
};
use arcl_store::PlanningTaskView;

use crate::project::Warning;

/// The `format_version` of every envelope whose shape the readiness change
/// left alone.
pub const FORMAT_VERSION: u8 = 1;
/// The `format_version` of the task view, the context packet and the
/// completion result, whose readiness reasons are typed objects.
pub const READINESS_FORMAT_VERSION: u8 = 2;
/// The page size a list tool uses when the call gives no `limit`.
pub const DEFAULT_LIMIT: u32 = 50;
/// The largest `limit` a list tool accepts.
pub const MAX_LIMIT: u32 = 200;

/// A record kind, as `arcl list --kind` takes it.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RecordKind {
    Capture,
    Release,
    Spec,
    Plan,
    Phase,
    Task,
    Note,
}

impl RecordKind {
    /// Every kind, in the order `list` returns them.
    pub const ALL: [Self; 7] = [
        Self::Capture,
        Self::Note,
        Self::Phase,
        Self::Plan,
        Self::Release,
        Self::Spec,
        Self::Task,
    ];

    /// The kind named `name`, as record summaries spell it.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == name)
    }

    /// The kind's name in record summaries.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Capture => "capture",
            Self::Release => "release",
            Self::Spec => "spec",
            Self::Plan => "plan",
            Self::Phase => "phase",
            Self::Task => "task",
            Self::Note => "note",
        }
    }
}

/// A record status, as `arcl list --status` takes it.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RecordStatus {
    Captured,
    Promoted,
    Discarded,
    Open,
    Completed,
    Cancelled,
    Pending,
    InProgress,
    Parked,
}

impl RecordStatus {
    /// The status as records store it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Captured => "captured",
            Self::Promoted => "promoted",
            Self::Discarded => "discarded",
            Self::Open => "open",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Parked => "parked",
        }
    }
}

/// What completing a task does with its open descendants.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OpenSubtasks {
    /// Refuse while any descendant is pending, in progress or parked.
    #[default]
    Refuse,
    /// Complete the task and leave its descendants as they are, like
    /// `--allow-open-children`.
    Leave,
    /// Complete the task and its pending and in-progress descendants, like
    /// `--with-subtasks`.
    Complete,
}

/// The record a promotion created, keyed by its kind.
#[derive(Debug, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PromotedRecord {
    Spec(Spec),
    Task(PlanningTask),
    Note(Note),
}

/// The record `show` returns: a task view for a task, or `{"action":
/// "shown", "<kind>": record}` for any other kind.
#[derive(Debug, JsonSchema, Serialize)]
#[serde(untagged)]
pub enum ShowData {
    Task(Box<PlanningTaskView>),
    Capture(CaptureData),
    Spec(SpecData),
    Plan(PlanData),
    Phase(PhaseData),
    Note(NoteData),
    Release(ReleaseData),
}

/// A successful tool result.
#[derive(Debug, JsonSchema, Serialize)]
pub struct Envelope<T> {
    /// The version of `data`'s shape.
    pub format_version: u8,
    /// The result, shaped as the matching `arcl --json` command's `data`.
    pub data: T,
    /// Conditions the call went on despite, such as a workspace sync that did
    /// not run. Left out when there are none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[schemars(default)]
    pub warnings: Vec<Warning>,
}

/// One record by ID.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordInput {
    /// A record ID of any kind, such as `arcl-t-…` or `arcl-n-…`.
    pub id: String,
}

/// One task by ID.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskInput {
    /// The task ID, `arcl-t-…`.
    pub task_id: String,
}

/// Filters and a page for `list`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListInput {
    /// Only records of this kind.
    pub kind: Option<RecordKind>,
    /// Only records in one of these statuses. Each must apply to `kind`.
    #[serde(default)]
    pub status: Vec<RecordStatus>,
    /// The most records to return, from 1 to 200. Defaults to 50.
    #[schemars(range(min = 1, max = 200))]
    pub limit: Option<u32>,
    /// The `next_cursor` of the previous page.
    pub cursor: Option<String>,
}

/// Filters and a page for `ready`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadyInput {
    /// Only tasks with one of these priorities.
    #[serde(default)]
    pub priority: Vec<TaskPriority>,
    /// Only tasks under this spec, `arcl-s-…`.
    pub spec: Option<String>,
    /// Only tasks under this plan, `arcl-pl-…`.
    pub plan: Option<String>,
    /// Only tasks under this phase, `arcl-ph-…`.
    pub phase: Option<String>,
    /// Only direct children of this task, `arcl-t-…`.
    pub parent: Option<String>,
    /// The most tasks to return, from 1 to 200. Defaults to 50.
    #[schemars(range(min = 1, max = 200))]
    pub limit: Option<u32>,
    /// The `next_cursor` of the previous page.
    pub cursor: Option<String>,
}

/// A new capture.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CaptureCreateInput {
    /// A short title.
    pub title: String,
    /// Markdown: what and why.
    #[serde(default)]
    pub body: String,
}

/// A capture to promote, and the record it becomes.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CapturePromoteInput {
    /// The capture ID, `arcl-c-…`.
    pub capture_id: String,
    /// The kind of record the capture becomes: `spec`, `task` or `note`, in
    /// any case. Checked after the capture is found, as the CLI checks it.
    #[schemars(required, extend("examples" = ["spec", "task", "note"]))]
    pub target: Option<String>,
    /// The new record's title. Defaults to the capture's.
    pub title: Option<String>,
    /// The new record's Markdown body. Defaults to the capture's.
    pub body: Option<String>,
    /// Markdown acceptance criteria for a spec.
    pub acceptance_criteria: Option<String>,
    /// For a task: the spec it is under, `arcl-s-…`.
    pub spec: Option<String>,
    /// For a task: the plan it is under, `arcl-pl-…`.
    pub plan: Option<String>,
    /// For a task: the phase it is under, `arcl-ph-…`.
    pub phase: Option<String>,
    /// For a task: the parent task, `arcl-t-…`.
    pub parent: Option<String>,
    /// For a task: `critical`, `high`, `normal` or `low`. Defaults to
    /// `normal`. Checked only when the target is a task, as the CLI checks it.
    #[serde(default = "normal_priority")]
    #[schemars(extend("examples" = ["critical", "high", "normal", "low"]))]
    pub priority: String,
    /// For a task: the order among its siblings. Defaults to 0.
    #[serde(default)]
    pub position: i64,
}

/// A new spec.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpecCreateInput {
    /// A short title.
    pub title: String,
    /// The Markdown spec body.
    #[serde(default)]
    pub body: String,
    /// Markdown acceptance criteria.
    #[serde(default)]
    pub acceptance_criteria: String,
}

/// A new plan under a spec.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanCreateInput {
    /// The spec the plan belongs to, `arcl-s-…`.
    pub spec: String,
    /// A short title.
    pub title: String,
    /// The Markdown plan body.
    #[serde(default)]
    pub body: String,
}

/// A new task.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskCreateInput {
    /// A short title.
    pub title: String,
    /// The Markdown body, with the acceptance criteria.
    #[serde(default)]
    pub body: String,
    /// The spec the task is under, `arcl-s-…`.
    pub spec: Option<String>,
    /// The plan the task is under, `arcl-pl-…`.
    pub plan: Option<String>,
    /// The phase the task is under, `arcl-ph-…`.
    pub phase: Option<String>,
    /// The parent task, `arcl-t-…`.
    pub parent: Option<String>,
    /// The priority. Defaults to `normal`.
    #[serde(default)]
    pub priority: TaskPriority,
    /// The order among its siblings. Defaults to 0.
    #[serde(default)]
    pub position: i64,
    /// Tasks that must complete before this one, `arcl-t-…`.
    #[serde(default)]
    pub blocked_by: Vec<String>,
}

/// Changes to a task's text and order. Fields left out stay as they are.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskUpdateInput {
    /// The task ID, `arcl-t-…`.
    pub task_id: String,
    /// A new title.
    pub title: Option<String>,
    /// A new Markdown body, replacing the old one.
    pub body: Option<String>,
    /// A new priority.
    pub priority: Option<TaskPriority>,
    /// A new order among its siblings.
    pub position: Option<i64>,
}

/// A new note.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoteCreateInput {
    /// A short title. Decisions use `Decision: <name>`.
    pub title: String,
    /// The Markdown note body.
    #[serde(default)]
    pub body: String,
}

/// A resume note for a task.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskHandoffInput {
    /// The task ID, `arcl-t-…`.
    pub task_id: String,
    /// Markdown: where the work stands and what is next.
    pub note: String,
}

/// A task to complete.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskCompleteInput {
    /// The task ID, `arcl-t-…`.
    pub task_id: String,
    /// Markdown: what shows the work is done.
    pub evidence: Option<String>,
    /// What to do with open descendants. Defaults to `refuse`.
    #[serde(default)]
    pub open_subtasks: OpenSubtasks,
}

/// A capture and what the tool did to it.
#[derive(Debug, JsonSchema, Serialize)]
pub struct CaptureData {
    /// `created` or `shown`.
    pub action: String,
    /// The capture.
    pub capture: Capture,
}

/// A promoted capture, as `arcl capture promote` returns it: the capture,
/// the promotion, and the new record under its kind (`spec`, `task` or
/// `note`).
#[derive(Debug, JsonSchema, Serialize)]
pub struct CapturePromotionData {
    /// The capture, now promoted.
    pub capture: Capture,
    /// The link from the capture to the new record.
    pub promotion: CapturePromotion,
    /// The new record.
    #[serde(flatten)]
    pub record: PromotedRecord,
}

/// A spec and what the tool did to it.
#[derive(Debug, JsonSchema, Serialize)]
pub struct SpecData {
    /// `created` or `shown`.
    pub action: String,
    /// The spec.
    pub spec: Spec,
}

/// A plan and what the tool did to it.
#[derive(Debug, JsonSchema, Serialize)]
pub struct PlanData {
    /// `created` or `shown`.
    pub action: String,
    /// The plan.
    pub plan: Plan,
}

/// A phase that `show` returned.
#[derive(Debug, JsonSchema, Serialize)]
pub struct PhaseData {
    /// `shown`.
    pub action: String,
    /// The phase.
    pub phase: Phase,
}

/// A note and what the tool did to it.
#[derive(Debug, JsonSchema, Serialize)]
pub struct NoteData {
    /// `created` or `shown`.
    pub action: String,
    /// The note.
    pub note: Note,
}

/// A release that `show` returned.
#[derive(Debug, JsonSchema, Serialize)]
pub struct ReleaseData {
    /// `shown`.
    pub action: String,
    /// The release.
    pub release: Release,
}

/// A task and what the tool did to it.
#[derive(Debug, JsonSchema, Serialize)]
pub struct TaskData {
    /// `created`, `updated`, `started`, `handed_off` or `unparked`.
    pub action: String,
    /// The task after the change.
    pub task: PlanningTask,
}

/// A completed task, as `arcl task complete` returns it.
#[derive(Debug, JsonSchema, Serialize)]
pub struct CompletionData {
    /// `completed`.
    pub action: String,
    /// The completed task.
    pub task: PlanningTask,
    /// The descendants the completion completed, in tree order.
    pub completed_subtasks: Vec<TaskId>,
    /// Tasks that were not ready before and are ready now, in `ready` order.
    pub unblocked: Vec<TaskId>,
}

/// One page of record summaries, ordered by kind, then ID.
#[derive(Debug, JsonSchema, Serialize)]
pub struct ListData {
    /// The records on this page.
    pub records: Vec<RecordSummary>,
    /// Pass as `cursor` for the next page. Left out on the last page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// A record's kind, ID, title and status, as `arcl list` prints it.
#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct RecordSummary {
    /// The record kind.
    pub kind: String,
    /// The record ID.
    pub id: String,
    /// The record title.
    pub title: String,
    /// The record status. Notes have none, so theirs is `available`.
    pub status: String,
}

/// One page of ready tasks, in `ready` order.
#[derive(Debug, JsonSchema, Serialize)]
pub struct ReadyData {
    /// The ready tasks on this page.
    pub tasks: Vec<PlanningTask>,
    /// Pass as `cursor` for the next page. Left out on the last page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// The priority a promoted task gets when the call names none, as the CLI's
/// `--priority` defaults.
fn normal_priority() -> String {
    TaskPriority::Normal.as_str().to_owned()
}
