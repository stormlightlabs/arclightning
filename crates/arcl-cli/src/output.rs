use std::borrow::Cow;
use std::path::{Path, PathBuf};

use owo_colors::{OwoColorize, Stream};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use unicode_width::UnicodeWidthStr;

use crate::cli::{ColorChoice, TreeStyle};
use arcl_core::domain::{
    Capture, CaptureStatus, ContainerStatus, Note, Phase, Plan, PlanningTask, Release, ShortIds, Spec, TaskDependency,
    TaskStatus,
};
use arcl_repo::{
    vcs::{HeadState, PathState},
    workspace::{Change, RecordSource, RecordStatusKind, WorkspaceStatus},
};
use arcl_store::{
    CheckReport, PlanApplyResult, PlanDiff, PlanningCompletion, PlanningContext, PlanningTaskView, PlanningWork,
};

/// The record kinds human `arcl tree` output groups under a heading at the
/// top level, with each heading, in the order the sections print. They follow
/// the specs and the tasks outside any spec.
pub const TREE_SECTIONS: [(&str, &str); 3] = [("capture", "Inbox"), ("note", "Notes"), ("release", "Releases")];

/// The characters human `arcl tree` output draws with by default.
const UNICODE_TREE: TreeChars =
    TreeChars { branch: "├── ", last: "└── ", pipe: "│   ", blank: "    ", dot: " · ", dash: "–" };

/// The characters `arcl tree --ascii` draws with, as GNU `tree -A` does.
const ASCII_TREE: TreeChars =
    TreeChars { branch: "|-- ", last: "`-- ", pipe: "|   ", blank: "    ", dot: " - ", dash: "-" };

/// The heading for files with lines that look like conflict markers.
const MARKER_LABEL: &str = "Lines that look like conflict markers (sync accepts them as content):";

/// The JSON `format_version` of every envelope whose shape has not changed.
const FORMAT_VERSION: u8 = 1;

/// The JSON `format_version` of the task view (`show`, `explain`), context
/// and `task complete` envelopes. Version 2 made readiness reasons typed
/// objects and added `unblocked` to completion.
const READINESS_FORMAT_VERSION: u8 = 2;

/// The output format for a command result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputMode {
    Human,
    Plain,
    Json,
    Quiet,
}

/// The mutation represented by a release command's output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseMutation {
    Created,
    Updated,
    Completed,
    Cancelled,
}

impl ReleaseMutation {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }

    const fn human_verb(self) -> &'static str {
        match self {
            Self::Created => "Created",
            Self::Updated => "Updated",
            Self::Completed => "Completed",
            Self::Cancelled => "Cancelled",
        }
    }
}

/// The relationship change represented by a dependency command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DependencyMutation {
    Added,
    Removed,
}

impl DependencyMutation {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Removed => "removed",
        }
    }

    const fn verb(self) -> &'static str {
        match self {
            Self::Added => "Added",
            Self::Removed => "Removed",
        }
    }
}

/// Whether a `skill install` report comes from an install or a `--check`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillAction {
    /// `skill install` wrote missing and changed files.
    Installed,
    /// `skill install --check` compared the files and wrote nothing.
    Checked,
}

/// What `skill install` did, or with `--check` found, for one skill file.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillFileStatus {
    /// The file was missing and was written.
    Created,
    /// The file differed and was overwritten.
    Updated,
    /// The file already matched.
    Unchanged,
    /// `--check` found the file missing.
    Missing,
    /// `--check` found the file different.
    Changed,
}

impl SkillFileStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Unchanged => "unchanged",
            Self::Missing => "missing",
            Self::Changed => "changed",
        }
    }
}

/// The status a tree node's mark is computed from: the record's stored
/// status, with readiness for tasks and started work for containers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TreeStatus {
    /// A task, and whether it is ready to start.
    Task { status: TaskStatus, ready: bool },
    /// A spec, plan or phase, and whether a task under it is in progress or
    /// completed. A release always carries `started: false`.
    Container { status: ContainerStatus, started: bool },
    /// A capture.
    Capture(CaptureStatus),
    /// A note, which has no stored status.
    Note,
}

/// The mark at the start of each human `arcl tree` line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusMark {
    /// Completed, or a promoted capture.
    Done,
    /// Cancelled, or a discarded capture.
    Cancelled,
    /// A task in progress, or an open container with started or finished work.
    Active,
    /// A parked task.
    Parked,
    /// A pending task that is ready.
    Ready,
    /// A pending task that is not ready, another open container, a captured
    /// capture or a note.
    Waiting,
}

impl StatusMark {
    /// The mark for a node with `status`.
    pub const fn of(status: TreeStatus) -> Self {
        match status {
            TreeStatus::Task { status: TaskStatus::Completed, .. }
            | TreeStatus::Container { status: ContainerStatus::Completed, .. }
            | TreeStatus::Capture(CaptureStatus::Promoted) => Self::Done,
            TreeStatus::Task { status: TaskStatus::Cancelled, .. }
            | TreeStatus::Container { status: ContainerStatus::Cancelled, .. }
            | TreeStatus::Capture(CaptureStatus::Discarded) => Self::Cancelled,
            TreeStatus::Task { status: TaskStatus::InProgress, .. }
            | TreeStatus::Container { status: ContainerStatus::Open, started: true } => Self::Active,
            TreeStatus::Task { status: TaskStatus::Parked, .. } => Self::Parked,
            TreeStatus::Task { status: TaskStatus::Pending, ready: true } => Self::Ready,
            TreeStatus::Task { status: TaskStatus::Pending, ready: false }
            | TreeStatus::Container { status: ContainerStatus::Open, started: false }
            | TreeStatus::Capture(CaptureStatus::Captured)
            | TreeStatus::Note => Self::Waiting,
        }
    }

    /// The Unicode mark.
    pub const fn glyph(self) -> char {
        match self {
            Self::Done => '✓',
            Self::Cancelled => '✗',
            Self::Active => '◐',
            Self::Parked => '‖',
            Self::Ready => '●',
            Self::Waiting => '○',
        }
    }

    /// The ASCII form, for `arcl tree --ascii`.
    pub const fn ascii(self) -> char {
        match self {
            Self::Done => '+',
            Self::Cancelled => 'x',
            Self::Active => '~',
            Self::Parked => '=',
            Self::Ready => '*',
            Self::Waiting => 'o',
        }
    }
}

/// Output failures that can occur before the application boundary adds context.
#[derive(Debug, Error)]
pub enum OutputError {
    #[error("could not serialize JSON output: {0}")]
    Json(#[from] serde_json::Error),
}

/// One line among a parent's children in human `arcl tree` output.
enum TreeItem<'a> {
    /// A record, drawn with its subtree.
    Record(&'a ConnectedTreeNode),
    /// A run of consecutive finished phases, drawn as one line.
    Folded(Vec<&'a ConnectedTreeNode>),
}

/// The renderer used by the application boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Renderer {
    mode: OutputMode,
    color: ColorChoice,
}

impl Renderer {
    pub const fn new(mode: OutputMode, color: ColorChoice) -> Self {
        Self { mode, color }
    }

    /// The output mode this renderer writes.
    pub const fn mode(&self) -> OutputMode {
        self.mode
    }

    /// Render the result of starting the foundation-only application.
    pub fn render_startup(&self) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => self.msg(),
            OutputMode::Plain => "ready".to_owned(),
            OutputMode::Json => serde_json::to_string(&Envelope {
                format_version: FORMAT_VERSION,
                data: StartupData { status: "ready", message: "Arc Lightning foundation ready" },
            })?,
            OutputMode::Quiet => return Ok(None),
        };

        Ok(Some(message))
    }

    /// Render the result of project initialization.
    pub fn render_init(&self, root: &Path, workspace_enabled: bool) -> Result<Option<String>, OutputError> {
        let root = root.to_string_lossy();
        let message = match self.mode {
            OutputMode::Human => {
                let workspace = if workspace_enabled { " with the workspace enabled" } else { "" };
                format!("Initialized Arc Lightning in `{root}`{workspace}")
            }
            OutputMode::Plain => format!("initialized\t{root}"),
            OutputMode::Json => serde_json::to_string(&Envelope {
                format_version: FORMAT_VERSION,
                data: InitData { status: "initialized", root: root.as_ref(), workspace_enabled },
            })?,
            OutputMode::Quiet => return Ok(None),
        };

        Ok(Some(match (self.mode, self.color) {
            (OutputMode::Human, ColorChoice::Always) => message.bright_blue().to_string(),
            (OutputMode::Human, ColorChoice::Auto) => message
                .if_supports_color(Stream::Stdout, |text| text.bright_blue())
                .to_string(),
            _ => message,
        }))
    }

    /// Render one connected-model mutation and include the affected record in JSON mode.
    pub fn render_capture(&self, action: &str, capture: &Capture) -> Result<Option<String>, OutputError> {
        self.render_named_mutation(action, "capture", &capture.id.to_string(), &capture.title, capture)
    }

    /// Render captures in their stable creation order.
    pub fn render_captures(&self, captures: &[Capture]) -> Result<Option<String>, OutputError> {
        self.render_collection(
            "captures",
            captures,
            &captures
                .iter()
                .map(|capture| {
                    ConnectedSummary::new(
                        "capture",
                        capture.id.to_string(),
                        capture.title.clone(),
                        capture.status.as_str(),
                    )
                })
                .collect::<Vec<_>>(),
        )
    }

    /// Render one owned specification mutation.
    pub fn render_spec(&self, action: &str, spec: &Spec) -> Result<Option<String>, OutputError> {
        self.render_named_mutation(action, "spec", &spec.id.to_string(), &spec.title, spec)
    }

    /// Render owned specifications in deterministic order.
    pub fn render_specs(&self, specs: &[Spec]) -> Result<Option<String>, OutputError> {
        self.render_collection(
            "specs",
            specs,
            &specs
                .iter()
                .map(|spec| {
                    ConnectedSummary::new("spec", spec.id.to_string(), spec.title.clone(), spec.status.as_str())
                })
                .collect::<Vec<_>>(),
        )
    }

    /// Render one persistent plan mutation.
    pub fn render_plan(&self, action: &str, plan: &Plan) -> Result<Option<String>, OutputError> {
        self.render_named_mutation(action, "plan", &plan.id.to_string(), &plan.title, plan)
    }

    /// Render persistent plans in deterministic order.
    pub fn render_plans(&self, plans: &[Plan]) -> Result<Option<String>, OutputError> {
        self.render_collection(
            "plans",
            plans,
            &plans
                .iter()
                .map(|plan| {
                    ConnectedSummary::new("plan", plan.id.to_string(), plan.title.clone(), plan.status.as_str())
                })
                .collect::<Vec<_>>(),
        )
    }

    /// Render one optional plan phase mutation.
    pub fn render_phase(&self, action: &str, phase: &Phase) -> Result<Option<String>, OutputError> {
        self.render_named_mutation(action, "phase", &phase.id.to_string(), &phase.title, phase)
    }

    /// Render optional plan phases in deterministic order.
    pub fn render_phases(&self, phases: &[Phase]) -> Result<Option<String>, OutputError> {
        self.render_collection(
            "phases",
            phases,
            &phases
                .iter()
                .map(|phase| {
                    ConnectedSummary::new(
                        "phase",
                        phase.id.to_string(),
                        phase.title.clone(),
                        phase.status.as_str(),
                    )
                })
                .collect::<Vec<_>>(),
        )
    }

    /// Render one connected planning task mutation.
    pub fn render_planning_task(&self, action: &str, task: &PlanningTask) -> Result<Option<String>, OutputError> {
        self.render_named_mutation(action, "task", &task.id.to_string(), &task.title, task)
    }

    /// Render a task completion with the subtasks it completed, in tree order,
    /// and the tasks it made ready. JSON lists their IDs as
    /// `completed_subtasks` and `unblocked`. Plain output prints the task ID,
    /// then a `subtask\t<id>` line for each subtask and an `unblocked\t<id>`
    /// line for each ready task.
    pub fn render_planning_completion(&self, completion: &PlanningCompletion) -> Result<Option<String>, OutputError> {
        let task = &completion.task;
        let subtasks = &completion.completed_subtasks;
        let unblocked = &completion.unblocked;
        let message = match self.mode {
            OutputMode::Human => std::iter::once(format!("Completed task `{}`: {}", task.id, task.title))
                .chain(
                    subtasks
                        .iter()
                        .map(|subtask| format!("Completed subtask `{}`: {}", subtask.id, subtask.title)),
                )
                .chain(
                    unblocked
                        .iter()
                        .map(|ready| format!("Unblocked task `{}`: {}", ready.id, ready.title)),
                )
                .collect::<Vec<_>>()
                .join("\n"),
            OutputMode::Plain => std::iter::once(task.id.to_string())
                .chain(subtasks.iter().map(|subtask| format!("subtask\t{}", subtask.id)))
                .chain(unblocked.iter().map(|ready| format!("unblocked\t{}", ready.id)))
                .collect::<Vec<_>>()
                .join("\n"),
            OutputMode::Json => serde_json::to_string(&Envelope {
                format_version: READINESS_FORMAT_VERSION,
                data: CompletionData {
                    action: "completed",
                    task,
                    completed_subtasks: subtasks.iter().map(|subtask| subtask.id.to_string()).collect(),
                    unblocked: unblocked.iter().map(|ready| ready.id.to_string()).collect(),
                },
            })?,
            OutputMode::Quiet => return Ok(None),
        };
        Ok(Some(self.style(message)))
    }

    /// Render connected planning tasks in deterministic order.
    pub fn render_planning_tasks(&self, tasks: &[PlanningTask]) -> Result<Option<String>, OutputError> {
        self.render_collection(
            "tasks",
            tasks,
            &tasks
                .iter()
                .map(|task| {
                    ConnectedSummary::new("task", task.id.to_string(), task.title.clone(), task.status.as_str())
                })
                .collect::<Vec<_>>(),
        )
    }

    /// Render one Markdown note mutation.
    pub fn render_note(&self, action: &str, note: &Note) -> Result<Option<String>, OutputError> {
        self.render_named_mutation(action, "note", &note.id.to_string(), &note.title, note)
    }

    /// Render Markdown notes in deterministic order.
    pub fn render_notes(&self, notes: &[Note]) -> Result<Option<String>, OutputError> {
        self.render_collection(
            "notes",
            notes,
            &notes
                .iter()
                .map(|note| ConnectedSummary::new("note", note.id.to_string(), note.title.clone(), "available"))
                .collect::<Vec<_>>(),
        )
    }

    /// Render one release mutation.
    pub fn render_connected_release(&self, action: &str, release: &Release) -> Result<Option<String>, OutputError> {
        self.render_named_mutation(action, "release", &release.id.to_string(), &release.title, release)
    }

    /// Render releases in deterministic order.
    pub fn render_releases(&self, releases: &[Release]) -> Result<Option<String>, OutputError> {
        self.render_collection(
            "releases",
            releases,
            &releases
                .iter()
                .map(|release| {
                    ConnectedSummary::new(
                        "release",
                        release.id.to_string(),
                        release.title.clone(),
                        release.status.as_str(),
                    )
                })
                .collect::<Vec<_>>(),
        )
    }

    /// Render a connected task inspection view. Human output shows each
    /// record ID as `ids` shortens it, readiness reasons included; plain and
    /// JSON output keep full IDs.
    pub fn render_planning_task_view(
        &self, view: &PlanningTaskView, ids: &ShortIds,
    ) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => view.human(ids),
            OutputMode::Plain => view.plain(),
            OutputMode::Json => {
                serde_json::to_string(&Envelope { format_version: READINESS_FORMAT_VERSION, data: view })?
            }
            OutputMode::Quiet => return Ok(None),
        };
        Ok(Some(self.style(message)))
    }

    /// Render a focused connected task context packet.
    pub fn render_planning_context(&self, context: &PlanningContext) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => context.human(),
            OutputMode::Plain => context.plain(),
            OutputMode::Json => {
                serde_json::to_string(&Envelope { format_version: READINESS_FORMAT_VERSION, data: context })?
            }
            OutputMode::Quiet => return Ok(None),
        };
        Ok(Some(self.style(message)))
    }

    /// Render the Work list `arcl serve` answers on `/api/work`. Its JSON
    /// carries typed readiness reasons, so it uses the readiness version.
    /// Human and plain output print one task ID per line.
    pub fn render_planning_work(&self, work: &PlanningWork) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human | OutputMode::Plain => work
                .tasks
                .iter()
                .map(|item| item.task.id.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
            OutputMode::Json => {
                serde_json::to_string(&Envelope { format_version: READINESS_FORMAT_VERSION, data: work })?
            }
            OutputMode::Quiet => return Ok(None),
        };
        Ok(Some(self.style(message)))
    }

    /// Render connected ready-work results.
    pub fn render_planning_ready_tasks(&self, tasks: &[PlanningTask]) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => {
                if tasks.is_empty() {
                    "No ready work.".to_owned()
                } else {
                    tasks
                        .iter()
                        .map(PlanningTask::human_line)
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            }
            OutputMode::Plain => tasks
                .iter()
                .map(|task| task.id.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
            OutputMode::Json => {
                serde_json::to_string(&Envelope { format_version: FORMAT_VERSION, data: ReadyPlanningData { tasks } })?
            }
            OutputMode::Quiet => return Ok(None),
        };
        if self.mode == OutputMode::Plain && tasks.is_empty() {
            return Ok(None);
        }
        Ok(Some(self.style(message)))
    }

    /// Render the first connected ready task, or a stable empty JSON result.
    pub fn render_next_planning_task(&self, task: Option<&PlanningTask>) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => task.map_or_else(|| "No ready work.".to_owned(), PlanningTask::human_line),
            OutputMode::Plain => task.map(|task| task.id.to_string()).unwrap_or_default(),
            OutputMode::Json => {
                serde_json::to_string(&Envelope { format_version: FORMAT_VERSION, data: NextPlanningData { task } })?
            }
            OutputMode::Quiet => return Ok(None),
        };
        if self.mode == OutputMode::Plain && task.is_none() {
            return Ok(None);
        }
        Ok(Some(self.style(message)))
    }

    /// Render the task `next --start` started, or that nothing was ready.
    /// JSON uses the `next` shape, `{"task": ...}`, with a null task when
    /// nothing started; plain output prints the task ID or nothing.
    pub fn render_started_next_planning_task(
        &self, task: Option<&PlanningTask>,
    ) -> Result<Option<String>, OutputError> {
        let message = match (self.mode, task) {
            (OutputMode::Human, Some(task)) => format!("Started task `{}`: {}", task.id, task.title),
            (OutputMode::Human, None) => "Nothing to start: no task is ready.".to_owned(),
            (OutputMode::Plain, Some(task)) => task.id.to_string(),
            (OutputMode::Json, _) => {
                serde_json::to_string(&Envelope { format_version: FORMAT_VERSION, data: NextPlanningData { task } })?
            }
            (OutputMode::Plain, None) | (OutputMode::Quiet, _) => return Ok(None),
        };
        Ok(Some(self.style(message)))
    }

    /// Render structured plan changes.
    pub fn render_plan_diff(&self, diff: &PlanDiff) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => diff.human(),
            OutputMode::Plain => diff.plain(),
            OutputMode::Json => serde_json::to_string(&Envelope { format_version: FORMAT_VERSION, data: diff })?,
            OutputMode::Quiet => return Ok(None),
        };
        Ok(Some(self.style(message)))
    }

    /// Render the result of applying structured plan input.
    pub fn render_plan_apply(&self, result: &PlanApplyResult) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => format!(
                "Applied plan `{}` ({} phase(s), {} task(s)).",
                result.plan.id,
                result.phases.len(),
                result.tasks.len()
            ),
            OutputMode::Plain => result.plan.id.to_string(),
            OutputMode::Json => serde_json::to_string(&Envelope { format_version: FORMAT_VERSION, data: result })?,
            OutputMode::Quiet => return Ok(None),
        };
        Ok(Some(self.style(message)))
    }

    /// Render compact summaries for the broad connected-record query.
    pub fn render_connected_summaries(&self, records: &[ConnectedSummary]) -> Result<Option<String>, OutputError> {
        self.render_collection("records", records, records)
    }

    /// Render the connected planning hierarchy. `view` shapes human output
    /// only; `--plain` and `--json` print every node given.
    pub fn render_connected_tree(
        &self, nodes: &[ConnectedTreeNode], view: TreeView,
    ) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => human_tree(nodes, view),
            OutputMode::Plain => nodes
                .iter()
                .flat_map(|node| node.plain_lines(0))
                .collect::<Vec<_>>()
                .join("\n"),
            OutputMode::Json => {
                serde_json::to_string(&Envelope { format_version: FORMAT_VERSION, data: TreeNodesData { nodes } })?
            }
            OutputMode::Quiet => return Ok(None),
        };
        Ok(Some(self.style(message)))
    }

    /// Render one plan with its directly owned records.
    pub fn render_plan_detail(&self, detail: &PlanDetail) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => format!(
                "Plan `{}` [{}] {}\nPhases: {}\nTasks: {}\nDependencies: {}",
                detail.plan.id,
                detail.plan.status.as_str(),
                detail.plan.title,
                detail.phases.len(),
                detail.tasks.len(),
                detail.dependencies.len()
            ),
            OutputMode::Plain => format!(
                "plan\\t{}\\t{}\\t{}\\t{}\\t{}",
                detail.plan.id,
                detail.plan.status.as_str(),
                plain_escape(&detail.plan.title),
                detail.phases.len(),
                detail.tasks.len()
            ),
            OutputMode::Json => serde_json::to_string(&Envelope { format_version: FORMAT_VERSION, data: detail })?,
            OutputMode::Quiet => return Ok(None),
        };
        Ok(Some(self.style(message)))
    }

    /// Render one explicit relationship or membership mutation.
    pub fn render_relationship<T: Serialize>(
        &self, action: &str, relationship: &T, id: &str,
    ) -> Result<Option<String>, OutputError> {
        self.render_relationship_with_key(action, "relationship", relationship, id)
    }

    /// Render one release membership mutation.
    pub fn render_membership<T: Serialize>(
        &self, action: &str, membership: &T, id: &str,
    ) -> Result<Option<String>, OutputError> {
        self.render_relationship_with_key(action, "membership", membership, id)
    }

    /// Render one note-link mutation.
    pub fn render_note_link<T: Serialize>(
        &self, action: &str, link: &T, id: &str,
    ) -> Result<Option<String>, OutputError> {
        self.render_relationship_with_key(action, "link", link, id)
    }

    fn render_relationship_with_key<T: Serialize>(
        &self, action: &str, key: &str, relationship: &T, id: &str,
    ) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => format!("{} relationship `{id}`", human_action(action)),
            OutputMode::Plain => id.to_owned(),
            OutputMode::Json => {
                let mut data = serde_json::Map::new();
                data.insert("action".to_owned(), serde_json::Value::String(action.to_owned()));
                data.insert(key.to_owned(), serde_json::to_value(relationship)?);
                serde_json::to_string(&Envelope {
                    format_version: FORMAT_VERSION,
                    data: serde_json::Value::Object(data),
                })?
            }
            OutputMode::Quiet => return Ok(None),
        };
        Ok(Some(self.style(message)))
    }

    /// Render capture promotion with both provenance directions and destination.
    pub fn render_capture_promotion(
        &self, result: &arcl_store::CapturePromotionResult,
    ) -> Result<Option<String>, OutputError> {
        let destination = match &result.record {
            arcl_store::CapturePromotionRecord::Spec(spec) => format!("spec `{}`", spec.id),
            arcl_store::CapturePromotionRecord::Task(task) => format!("task `{}`", task.id),
            arcl_store::CapturePromotionRecord::Note(note) => format!("note `{}`", note.id),
        };
        let message = match self.mode {
            OutputMode::Human => format!("Promoted capture `{}` to {destination}", result.capture.id),
            OutputMode::Plain => match result.promotion.target {
                arcl_core::domain::CapturePromotionTarget::Spec(id) => id.to_string(),
                arcl_core::domain::CapturePromotionTarget::Task(id) => id.to_string(),
                arcl_core::domain::CapturePromotionTarget::Note(id) => id.to_string(),
            },
            OutputMode::Json => {
                let mut data = serde_json::Map::new();
                data.insert("capture".to_owned(), serde_json::to_value(&result.capture)?);
                data.insert("promotion".to_owned(), serde_json::to_value(&result.promotion)?);
                let (kind, record) = match &result.record {
                    arcl_store::CapturePromotionRecord::Spec(record) => ("spec", serde_json::to_value(record)?),
                    arcl_store::CapturePromotionRecord::Task(record) => ("task", serde_json::to_value(record)?),
                    arcl_store::CapturePromotionRecord::Note(record) => ("note", serde_json::to_value(record)?),
                };
                data.insert(kind.to_owned(), record);
                serde_json::to_string(&Envelope {
                    format_version: FORMAT_VERSION,
                    data: serde_json::Value::Object(data),
                })?
            }
            OutputMode::Quiet => return Ok(None),
        };
        Ok(Some(self.style(message)))
    }

    /// Render one explicit relationship or membership list.
    pub fn render_relationships<T: Serialize>(
        &self, key: &str, relationships: &[T], lines: &[String], empty: &str,
    ) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => {
                if lines.is_empty() {
                    empty.to_owned()
                } else {
                    lines.join("\n")
                }
            }
            OutputMode::Plain => lines.join("\n"),
            OutputMode::Json => {
                let mut data = serde_json::Map::new();
                data.insert(key.to_owned(), serde_json::to_value(relationships)?);
                serde_json::to_string(&Envelope {
                    format_version: FORMAT_VERSION,
                    data: serde_json::Value::Object(data),
                })?
            }
            OutputMode::Quiet => return Ok(None),
        };
        Ok(Some(self.style(message)))
    }

    /// Render a record `arcl show` found. Human and plain output show `id`,
    /// which callers shorten for human output only; JSON output holds the
    /// record with its full ID.
    pub fn render_shown<T: Serialize>(
        &self, kind: &str, id: &str, title: &str, record: &T,
    ) -> Result<Option<String>, OutputError> {
        self.render_named_mutation("shown", kind, id, title, record)
    }

    fn render_named_mutation<T: Serialize>(
        &self, action: &str, kind: &str, id: &str, title: &str, record: &T,
    ) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => format!("{} {} `{id}`: {title}", human_action(action), kind),
            OutputMode::Plain => id.to_owned(),
            OutputMode::Json => {
                let mut data = serde_json::Map::new();
                data.insert("action".to_owned(), serde_json::Value::String(action.to_owned()));
                data.insert(kind.to_owned(), serde_json::to_value(record)?);
                serde_json::to_string(&Envelope {
                    format_version: FORMAT_VERSION,
                    data: serde_json::Value::Object(data),
                })?
            }
            OutputMode::Quiet => return Ok(None),
        };
        Ok(Some(self.style(message)))
    }

    fn render_collection<T: Serialize>(
        &self, key: &str, records: &[T], summaries: &[ConnectedSummary],
    ) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => {
                if summaries.is_empty() {
                    "No records.".to_owned()
                } else {
                    summaries
                        .iter()
                        .map(ConnectedSummary::human_line)
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            }
            OutputMode::Plain => summaries
                .iter()
                .map(|summary| summary.id.clone())
                .collect::<Vec<_>>()
                .join("\n"),
            OutputMode::Json => {
                let mut data = serde_json::Map::new();
                data.insert(key.to_owned(), serde_json::to_value(records)?);
                serde_json::to_string(&Envelope {
                    format_version: FORMAT_VERSION,
                    data: serde_json::Value::Object(data),
                })?
            }
            OutputMode::Quiet => return Ok(None),
        };
        Ok(Some(self.style(message)))
    }

    /// Render one release mutation and include the affected record in JSON mode.
    pub fn render_release(&self, mutation: ReleaseMutation, release: &Release) -> Result<Option<String>, OutputError> {
        let id = release.id.to_string();
        let message = match self.mode {
            OutputMode::Human => format!("{} release `{id}`: {}", mutation.human_verb(), release.title),
            OutputMode::Plain | OutputMode::Quiet => id,
            OutputMode::Json => serde_json::to_string(&Envelope {
                format_version: FORMAT_VERSION,
                data: ReleaseMutationData { action: mutation.as_str(), release },
            })?,
        };

        Ok(Some(match (self.mode, self.color) {
            (OutputMode::Human, ColorChoice::Always) => message.bright_blue().to_string(),
            (OutputMode::Human, ColorChoice::Auto) => message
                .if_supports_color(Stream::Stdout, |text| text.bright_blue())
                .to_string(),
            _ => message,
        }))
    }

    /// Render one connected task dependency relationship change.
    pub fn render_dependency(
        &self, mutation: DependencyMutation, dependency: &TaskDependency,
    ) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => format!(
                "{} dependency `{}` blocked by `{}`",
                mutation.verb(),
                dependency.task_id,
                dependency.blocker_id
            ),
            OutputMode::Plain => dependency.task_id.to_string(),
            OutputMode::Json => serde_json::to_string(&Envelope {
                format_version: FORMAT_VERSION,
                data: DependencyMutationData { action: mutation.as_str(), dependency },
            })?,
            OutputMode::Quiet => return Ok(None),
        };
        Ok(Some(self.style(message)))
    }

    /// Render database and graph integrity results.
    pub fn render_check(&self, report: &CheckReport) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => {
                let mut lines =
                    vec![if report.valid { "Check passed.".to_owned() } else { "Check failed.".to_owned() }];
                lines.extend(report.errors.iter().map(|error| format!("error: {error}")));
                lines.join("\n")
            }
            OutputMode::Plain => report
                .errors
                .iter()
                .map(|error| format!("error\t{}", plain_escape(error)))
                .collect::<Vec<_>>()
                .join("\n"),
            OutputMode::Json => serde_json::to_string(&Envelope { format_version: FORMAT_VERSION, data: report })?,
            OutputMode::Quiet => return Ok(None),
        };
        Ok(Some(self.style(message)))
    }

    /// Render the URL `arcl serve` listens on. Quiet mode prints the URL
    /// alone, since it is the command's result.
    pub fn render_serve(&self, url: &str) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => format!("Serving Arc Lightning at {url}\nPress Ctrl-C to stop."),
            OutputMode::Plain | OutputMode::Quiet => url.to_owned(),
            OutputMode::Json => serde_json::to_string(&Envelope {
                format_version: FORMAT_VERSION,
                data: ServeData { status: "serving", url },
            })?,
        };
        Ok(Some(self.style(message)))
    }

    /// Render a record's workspace file for the web app's raw editor. Human
    /// and plain output print the file itself.
    pub fn render_record_source(&self, source: &RecordSource) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human | OutputMode::Plain => source.content.trim_end_matches('\n').to_owned(),
            OutputMode::Json => serde_json::to_string(&Envelope {
                format_version: FORMAT_VERSION,
                data: RecordSourceData {
                    id: &source.id,
                    kind: source.kind.as_str(),
                    path: source.path.to_string_lossy().into_owned(),
                    source: &source.content,
                    revision: &source.revision,
                },
            })?,
            OutputMode::Quiet => return Ok(None),
        };
        Ok(Some(message))
    }

    /// Render a `skill install` report. Human output lists only the files
    /// that are not unchanged; plain output lists every file as
    /// `<status>\t<path>`.
    pub fn render_skill(&self, report: &SkillReport) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => {
                let changed = report
                    .files
                    .iter()
                    .filter(|file| file.status != SkillFileStatus::Unchanged)
                    .collect::<Vec<_>>();
                let root = &report.root;
                let heading = match (report.action, changed.is_empty()) {
                    (SkillAction::Checked, true) => format!("The Arc Lightning skill in `{root}` matches this arcl."),
                    (SkillAction::Checked, false) => {
                        format!("The Arc Lightning skill in `{root}` differs from this arcl:")
                    }
                    (SkillAction::Installed, true) => format!("The Arc Lightning skill in `{root}` is up to date."),
                    (SkillAction::Installed, false) => format!("Installed the Arc Lightning skill in `{root}`:"),
                };
                std::iter::once(heading)
                    .chain(
                        changed
                            .iter()
                            .map(|file| format!("  {} {}", file.status.as_str(), file.path)),
                    )
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            OutputMode::Plain => report
                .files
                .iter()
                .map(|file| format!("{}\t{}", file.status.as_str(), plain_escape(&file.path)))
                .collect::<Vec<_>>()
                .join("\n"),
            OutputMode::Json => serde_json::to_string(&Envelope { format_version: FORMAT_VERSION, data: report })?,
            OutputMode::Quiet => return Ok(None),
        };
        Ok(Some(self.style(message)))
    }

    /// Render what `web export` wrote. Human output names the directory and
    /// each changed or removed document; plain output lists them as
    /// `<changed|removed>\t<path>`.
    pub fn render_web_export(&self, dir: &Path, manifest: &WebManifest) -> Result<Option<String>, OutputError> {
        let listed = || {
            let changed = manifest.changed.iter().map(|path| ("changed", path));
            changed.chain(manifest.removed.iter().map(|path| ("removed", path)))
        };
        let message = match self.mode {
            OutputMode::Human => {
                let heading = format!(
                    "Exported {} documents to `{}`: {} changed, {} removed.",
                    manifest.documents.len(),
                    dir.display(),
                    manifest.changed.len(),
                    manifest.removed.len()
                );
                std::iter::once(heading)
                    .chain(listed().map(|(status, path)| format!("  {status} {path}")))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            OutputMode::Plain => listed()
                .map(|(status, path)| format!("{status}\t{}", plain_escape(path)))
                .collect::<Vec<_>>()
                .join("\n"),
            OutputMode::Json => serde_json::to_string(&Envelope { format_version: FORMAT_VERSION, data: manifest })?,
            OutputMode::Quiet => return Ok(None),
        };
        Ok(Some(self.style(message)))
    }

    /// Render the result of each edit `web apply` read, in order. Plain
    /// output lists them as `<status>\t<id>\t<op>\t<target>`.
    pub fn render_web_apply(&self, results: &[AppliedEdit]) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => results
                .iter()
                .map(|result| {
                    let target = result
                        .target
                        .as_deref()
                        .map(|target| format!(" {target}"))
                        .unwrap_or_default();
                    let op = result.op.as_deref().unwrap_or("an entry");
                    let id = result.id.as_deref().unwrap_or("no ID");
                    let edit = format!("{op}{target} (queued as {id})");
                    match result.status {
                        AppliedStatus::Applied => format!("Applied {edit}"),
                        AppliedStatus::Skipped => format!("Skipped {edit}: it is not queued"),
                        AppliedStatus::Failed => {
                            let error = result.error.as_ref();
                            let text = |field: &str| error.and_then(|error| error[field].as_str()).unwrap_or_default();
                            format!("Failed {edit}: {} ({})", text("message"), text("code"))
                        }
                    }
                })
                .collect::<Vec<_>>()
                .join("\n"),
            OutputMode::Plain => results
                .iter()
                .map(|result| {
                    let target = result.target.as_deref().unwrap_or_default();
                    let id = result.id.as_deref().unwrap_or_default();
                    let op = result.op.as_deref().unwrap_or_default();
                    let fields = [result.status.as_str(), id, op, target];
                    fields.map(plain_escape).join("\t")
                })
                .collect::<Vec<_>>()
                .join("\n"),
            OutputMode::Json => {
                serde_json::to_string(&Envelope { format_version: FORMAT_VERSION, data: AppliedData { results } })?
            }
            OutputMode::Quiet => return Ok(None),
        };
        Ok(Some(self.style(message)))
    }

    /// Render a workspace status report.
    pub fn render_workspace_status(&self, status: &WorkspaceStatus) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => human_workspace_status(status),
            OutputMode::Plain => plain_workspace_status(status),
            OutputMode::Json => serde_json::to_string(&Envelope { format_version: FORMAT_VERSION, data: status })?,
            OutputMode::Quiet => return Ok(None),
        };
        Ok(Some(self.style(message)))
    }

    fn style(&self, message: String) -> String {
        match (self.mode, self.color) {
            (OutputMode::Human, ColorChoice::Always) => message.bright_blue().to_string(),
            (OutputMode::Human, ColorChoice::Auto) => message
                .if_supports_color(Stream::Stdout, |text| text.bright_blue())
                .to_string(),
            _ => message,
        }
    }

    fn msg(&self) -> String {
        const MESSAGE: &str = "Arc Lightning foundation ready";
        match self.color {
            ColorChoice::Always => MESSAGE.bright_blue().to_string(),
            ColorChoice::Never => MESSAGE.to_owned(),
            ColorChoice::Auto => MESSAGE
                .if_supports_color(Stream::Stdout, |text| text.bright_blue())
                .to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
struct Envelope<T> {
    format_version: u8,
    data: T,
}

/// A record's workspace file, as `GET /api/records/{id}/source` answers it.
#[derive(Debug, Serialize)]
struct RecordSourceData<'a> {
    id: &'a str,
    kind: &'static str,
    /// The path relative to the workspace root.
    path: String,
    source: &'a str,
    revision: &'a str,
}

#[derive(Debug, Serialize)]
struct StartupData {
    status: &'static str,
    message: &'static str,
}

#[derive(Debug, Serialize)]
struct ServeData<'a> {
    status: &'static str,
    url: &'a str,
}

#[derive(Debug, Serialize)]
struct InitData<'a> {
    status: &'static str,
    root: &'a str,
    workspace_enabled: bool,
}

#[derive(Debug, Serialize)]
struct ReleaseMutationData<'a> {
    action: &'static str,
    release: &'a Release,
}

#[derive(Debug, Serialize)]
struct DependencyMutationData<'a> {
    action: &'static str,
    dependency: &'a TaskDependency,
}

/// One skill file in a [`SkillReport`].
#[derive(Clone, Debug, Serialize)]
pub struct SkillFile {
    /// The path relative to the project root, with `/` separators.
    pub path: String,
    /// What the install did to the file, or what `--check` found.
    pub status: SkillFileStatus,
}

/// The result of `skill install`, with or without `--check`.
#[derive(Clone, Debug, Serialize)]
pub struct SkillReport {
    /// `installed`, or `checked` under `--check`.
    pub action: SkillAction,
    /// The project root the paths are relative to.
    pub root: String,
    /// One entry per agent directory and skill file, in install order.
    pub files: Vec<SkillFile>,
}

/// The manifest `web export` writes beside the documents, and reads back on
/// the next export to tell which documents changed.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WebManifest {
    /// The manifest's own version, 1.
    pub format_version: u8,
    /// The Git commit HEAD named at the export, if the project is in Git.
    pub commit: Option<String>,
    /// When the export ran, in UTC, as RFC 3339.
    pub synced_at: String,
    /// A digest of every document but `meta/sync`, which changes only when one of them does.
    pub revision: String,
    /// Every document path, in the order to write them.
    pub documents: Vec<String>,
    /// The documents that are new or differ from the files of the last export,
    /// in the order to write them; every document under `--full`.
    pub changed: Vec<String>,
    /// The documents the last export had and this one does not, to delete.
    pub removed: Vec<String>,
}

/// What became of one queued edit.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppliedStatus {
    /// The change ran and saved.
    Applied,
    /// The change was refused, with the CLI's error.
    Failed,
    /// The entry's status was not `queued`, or it had none, so it did not run.
    Skipped,
}

impl AppliedStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}

/// The result of one edit `web apply` read.
#[derive(Clone, Debug, Serialize)]
pub struct AppliedEdit {
    /// The queued entry's ID, or `None` for a malformed entry without a
    /// string `id`.
    pub id: Option<String>,
    /// The operation it named, or `None` for a malformed entry without a
    /// string `op`.
    pub op: Option<String>,
    /// The record it named, if any.
    pub target: Option<String>,
    /// Whether the edit applied, failed or was skipped.
    pub status: AppliedStatus,
    /// The `data` of the change's JSON envelope, when it applied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    /// The CLI's error, with its `code` and `message`, when it failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<serde_json::Value>,
    /// The warnings the change's syncs gave, as `{"code", "message"}` objects.
    pub warnings: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct AppliedData<'a> {
    results: &'a [AppliedEdit],
}

/// A compact connected-record summary used by human and plain list output.
#[derive(Clone, Debug, Serialize)]
pub struct ConnectedSummary {
    pub kind: &'static str,
    pub id: String,
    pub title: String,
    pub status: String,
}

impl ConnectedSummary {
    pub fn new(kind: &'static str, id: String, title: String, status: &str) -> Self {
        Self { kind, id, title, status: status.to_owned() }
    }

    fn human_line(&self) -> String {
        format!("{}\t{}\t[{}]\t{}", self.kind, self.id, self.status, self.title)
    }
}

trait PlanningTaskOutput {
    fn human_line(&self) -> String;
}

impl PlanningTaskOutput for PlanningTask {
    fn human_line(&self) -> String {
        format!(
            "{}\t[{}]\t{}\t{}",
            self.id,
            self.priority.as_str(),
            self.status.as_str(),
            self.title
        )
    }
}

/// A node in the connected planning hierarchy.
#[derive(Clone, Debug, Serialize)]
pub struct ConnectedTreeNode {
    /// The connected record kind.
    pub kind: &'static str,
    /// The typed record identifier.
    pub id: String,
    /// The record title.
    pub title: String,
    /// The record lifecycle status.
    pub status: String,
    /// The status mark human output leads the line with. `--plain` and
    /// `--json` carry `status` instead.
    #[serde(skip)]
    pub mark: StatusMark,
    /// The unfinished tasks that block this task directly, which human
    /// output names at the end of the line. The root of `arcl tree <id>`
    /// also lists those it inherits from its ancestors.
    #[serde(skip)]
    pub blocked_by: Vec<String>,
    /// Child records in the planning hierarchy.
    pub children: Vec<ConnectedTreeNode>,
}

impl ConnectedTreeNode {
    /// The human line for this record: its mark and title, a phase's
    /// done/total task count, and a task's open blockers.
    fn human_line(&self, view: TreeView) -> String {
        let mut line = format!("{} {}", view.mark(self.mark), human_escape(&self.title));
        if self.kind == "phase" {
            let (done, total) = task_progress(&self.children);
            if total > 0 {
                line.push_str(&format!("  {done}/{total}"));
            }
        }
        if !self.blocked_by.is_empty() {
            line.push_str(&format!("  blocked by {}", self.blocked_by.join(", ")));
        }
        line
    }

    fn plain_lines(&self, depth: usize) -> Vec<String> {
        let mut lines = vec![format!(
            "{}\t{}\t{}\t{}\t{}",
            depth,
            self.kind,
            self.id,
            self.status,
            plain_escape(&self.title)
        )];
        lines.extend(self.children.iter().flat_map(|child| child.plain_lines(depth + 1)));
        lines
    }
}

/// What human `arcl tree` output shows and how it draws it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TreeView {
    /// The nodes are one record's subtree, from `arcl tree <id>`: no
    /// sections and no footer.
    pub rooted: bool,
    /// Unfold finished phases, and list the Inbox, Notes and Releases
    /// sections in place of the footer.
    pub all: bool,
    /// Draw with ASCII characters only.
    pub ascii: bool,
    /// The number of record levels to print, or `None` for every level.
    pub depth: Option<u16>,
    /// The layout: folded, full or outline.
    pub style: TreeStyle,
}

impl TreeView {
    const fn chars(self) -> TreeChars {
        if self.ascii { ASCII_TREE } else { UNICODE_TREE }
    }

    const fn mark(self, mark: StatusMark) -> char {
        if self.ascii { mark.ascii() } else { mark.glyph() }
    }

    /// Whether records at `level`, counting top-level records as `1`, print.
    fn shows(self, level: usize) -> bool {
        self.depth.is_none_or(|depth| level <= usize::from(depth))
    }
}

/// The connectors and separators of one human `arcl tree` drawing style.
#[derive(Clone, Copy, Debug)]
struct TreeChars {
    /// Before a child that has a later sibling.
    branch: &'static str,
    /// Before the last child.
    last: &'static str,
    /// Below a child that has a later sibling.
    pipe: &'static str,
    /// Below the last child.
    blank: &'static str,
    /// Between the parts of a folded line or the footer.
    dot: &'static str,
    /// Between the first and last number of a folded range.
    dash: &'static str,
}

/// One line of human `arcl tree` output before its style lays it out.
struct TreeLine<'a> {
    /// The record the line draws, or `None` for a heading or a folded run.
    record: Option<&'a ConnectedTreeNode>,
    /// The connectors, then the mark, title and suffixes, or the heading or
    /// folded run.
    text: String,
    /// The levels below the top level, or below a section heading, that
    /// `--style outline` indents the title by.
    indent: usize,
}

#[derive(Debug, Serialize)]
struct TreeNodesData<'a> {
    nodes: &'a [ConnectedTreeNode],
}

/// The enriched records shown by `plan show`.
#[derive(Clone, Debug, Serialize)]
pub struct PlanDetail {
    pub plan: Plan,
    pub phases: Vec<Phase>,
    pub tasks: Vec<PlanningTask>,
    pub dependencies: Vec<TaskDependency>,
}

#[derive(Debug, Serialize)]
struct CompletionData<'a> {
    action: &'static str,
    task: &'a PlanningTask,
    completed_subtasks: Vec<String>,
    unblocked: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ReadyPlanningData<'a> {
    tasks: &'a [PlanningTask],
}

#[derive(Debug, Serialize)]
struct NextPlanningData<'a> {
    task: Option<&'a PlanningTask>,
}

/// The place of a top-level record `kind` in [`TREE_SECTIONS`], or `None`
/// for the specs and tasks that print before every section.
pub fn tree_section(kind: &str) -> Option<usize> {
    TREE_SECTIONS.iter().position(|(section, _)| *section == kind)
}

/// Renders tree nodes for human output in `view.style`. Nodes outside
/// [`TREE_SECTIONS`] print flush-left with their subtrees below. The folded
/// style folds each run of finished phases into one line.
///
/// Unless `view` is rooted, the section nodes follow as a one-line footer,
/// or with `view.all` under each section's heading. A rooted tree prints its
/// root as a record of any kind.
fn human_tree(nodes: &[ConnectedTreeNode], view: TreeView) -> String {
    let (sectioned, records): (Vec<_>, Vec<_>) = nodes
        .iter()
        .partition(|node| !view.rooted && tree_section(node.kind).is_some());
    let mut lines = Vec::new();
    for node in records {
        lines.push(TreeLine { record: Some(node), text: node.human_line(view), indent: 0 });
        push_children(&mut lines, &node.children, "", 2, 1, view);
    }
    if !view.rooted && view.all {
        for (kind, heading) in TREE_SECTIONS {
            let members = sectioned
                .iter()
                .copied()
                .filter(|node| node.kind == kind)
                .collect::<Vec<_>>();
            if !members.is_empty() {
                lines.push(TreeLine { record: None, text: heading.to_owned(), indent: 0 });
                push_children(&mut lines, members, "", 1, 1, view);
            }
        }
    }
    let mut output = match view.style {
        TreeStyle::Folded => lines.into_iter().map(|line| line.text).collect(),
        TreeStyle::Full => full_lines(lines),
        TreeStyle::Outline => outline_lines(&lines),
    };
    if !view.rooted
        && !view.all
        && let Some(footer) = tree_footer(&sectioned, view)
    {
        output.push(footer);
    }
    output.join("\n")
}

/// Appends `children`, at `level` and `indent`, below a line drawn with
/// `prefix`. Each run of finished phases folds into one line in the folded
/// style unless `view.all` is set.
fn push_children<'a>(
    lines: &mut Vec<TreeLine<'a>>, children: impl IntoIterator<Item = &'a ConnectedTreeNode>, prefix: &str,
    level: usize, indent: usize, view: TreeView,
) {
    if !view.shows(level) {
        return;
    }
    let chars = view.chars();
    let items = tree_items(children, view.style == TreeStyle::Folded && !view.all);
    let count = items.len();
    for (index, item) in items.into_iter().enumerate() {
        let last = index + 1 == count;
        let connector = if last { chars.last } else { chars.branch };
        match item {
            TreeItem::Record(node) => {
                lines.push(TreeLine {
                    record: Some(node),
                    text: format!("{prefix}{connector}{}", node.human_line(view)),
                    indent,
                });
                let below = format!("{prefix}{}", if last { chars.blank } else { chars.pipe });
                push_children(lines, &node.children, &below, level + 1, indent + 1, view);
            }
            TreeItem::Folded(run) => lines.push(TreeLine {
                record: None,
                text: format!("{prefix}{connector}{}", folded_line(&run, view)),
                indent,
            }),
        }
    }
}

/// The `--style full` lines: each record line padded to the widest record
/// line, measured in terminal columns, then two spaces and its short ID
/// aligned right. Headings keep no ID. East Asian Ambiguous characters count
/// as one column.
fn full_lines(lines: Vec<TreeLine>) -> Vec<String> {
    let records = lines.iter().filter_map(|line| line.record.map(|node| (line, node)));
    let width = records.clone().map(|(line, _)| line.text.width()).max().unwrap_or(0);
    let id_width = records.map(|(_, node)| node.id.width()).max().unwrap_or(0);
    lines
        .into_iter()
        .map(|line| match line.record {
            Some(node) => {
                let pad = width - line.text.width();
                format!("{}{:pad$}  {:>id_width$}", line.text, "", node.id)
            }
            None => line.text,
        })
        .collect()
}

/// The `--style outline` lines: a STATUS, ID and TITLE header, then one row
/// per record with its stored status, short ID and title, the title behind
/// `. ` per indent. A heading sits in the TITLE column. No records, no rows.
fn outline_lines(lines: &[TreeLine]) -> Vec<String> {
    let records = lines.iter().filter_map(|line| line.record);
    let status_width = records.clone().map(|node| node.status.len()).max();
    let Some(status_width) = status_width.map(|width| width.max("STATUS".len())) else {
        return Vec::new();
    };
    let id_width = records.map(|node| node.id.len()).max().unwrap_or(0).max("ID".len());
    let mut rows = vec![format!("{:<status_width$}  {:<id_width$}  TITLE", "STATUS", "ID")];
    rows.extend(lines.iter().map(|line| match line.record {
        Some(node) => format!(
            "{:<status_width$}  {:<id_width$}  {}{}",
            node.status,
            node.id,
            ". ".repeat(line.indent),
            human_escape(&node.title)
        ),
        None => format!("{:status_width$}  {:id_width$}  {}", "", "", line.text),
    }));
    rows
}

/// Groups `nodes` into tree lines, with each run of consecutive finished
/// phases in one [`TreeItem::Folded`] when `fold` is set.
fn tree_items<'a>(nodes: impl IntoIterator<Item = &'a ConnectedTreeNode>, fold: bool) -> Vec<TreeItem<'a>> {
    let mut items = Vec::new();
    for node in nodes {
        if fold && is_finished_phase(node) {
            if let Some(TreeItem::Folded(run)) = items.last_mut() {
                run.push(node);
            } else {
                items.push(TreeItem::Folded(vec![node]));
            }
        } else {
            items.push(TreeItem::Record(node));
        }
    }
    items
}

/// Whether `node` is a completed phase whose counted tasks are all done. A
/// phase completed over open tasks stays expanded so those tasks show.
fn is_finished_phase(node: &ConnectedTreeNode) -> bool {
    if node.kind != "phase" || node.mark != StatusMark::Done {
        return false;
    }
    let (done, total) = task_progress(&node.children);
    done == total
}

/// The line for a run of finished phases, such as
/// `✓ Milestones 1–6  done · 29 tasks`. One phase keeps its title. A longer
/// run names its range when each title before its first colon is
/// `Milestone <n>` or `Phase <n>`, with consecutive numbers, and counts
/// phases otherwise.
fn folded_line(run: &[&ConnectedTreeNode], view: TreeView) -> String {
    let chars = view.chars();
    let tasks = run.iter().map(|phase| task_progress(&phase.children).1).sum::<usize>();
    let label = match run {
        [phase] => human_escape(&phase.title).into_owned(),
        _ => folded_range(run, chars.dash).unwrap_or_else(|| format!("{} phases", run.len())),
    };
    let noun = if tasks == 1 { "task" } else { "tasks" };
    format!(
        "{} {label}  done{}{tasks} {noun}",
        view.mark(StatusMark::Done),
        chars.dot
    )
}

/// The range label for a run of phases, such as `Milestones 1–6`, or `None`
/// unless every title is `Milestone` or `Phase` with the same word and
/// consecutive numbers.
fn folded_range(run: &[&ConnectedTreeNode], dash: &str) -> Option<String> {
    let numbered = run
        .iter()
        .map(|phase| numbered_title(&phase.title))
        .collect::<Option<Vec<_>>>()?;
    let (word, from) = *numbered.first()?;
    let (_, to) = *numbered.last()?;
    let consecutive = numbered
        .windows(2)
        .all(|pair| pair[1].0 == word && pair[0].1.checked_add(1) == Some(pair[1].1));
    (["Milestone", "Phase"].contains(&word) && consecutive).then(|| format!("{word}s {from}{dash}{to}"))
}

/// Splits a title such as `Milestone 7: Local web app` into its word and
/// number, `("Milestone", 7)`, or `None` when the part before the first
/// colon does not end in a number.
fn numbered_title(title: &str) -> Option<(&str, u64)> {
    let head = title.split(':').next()?.trim();
    let (word, number) = head.rsplit_once(' ')?;
    if number.is_empty() || !number.chars().all(|character| character.is_ascii_digit()) {
        return None;
    }
    Some((word, number.parse().ok()?))
}

/// Counts the completed tasks and all tasks at any depth in `nodes`,
/// leaving out cancelled tasks.
fn task_progress(nodes: &[ConnectedTreeNode]) -> (usize, usize) {
    nodes.iter().fold((0, 0), |(done, total), node| {
        let (child_done, child_total) = task_progress(&node.children);
        let (own_done, own_total) = match node.mark {
            _ if node.kind != "task" => (0, 0),
            StatusMark::Cancelled => (0, 0),
            StatusMark::Done => (1, 1),
            _ => (0, 1),
        };
        (done + own_done + child_done, total + own_total + child_total)
    })
}

/// The footer that sums up the section nodes: open captures, notes and
/// release names, such as `Inbox: 21 open · Notes: 17 · Releases: v0.1.0`.
/// Leaves out empty parts, and returns `None` when every part is empty.
fn tree_footer(nodes: &[&ConnectedTreeNode], view: TreeView) -> Option<String> {
    let open = nodes
        .iter()
        .filter(|node| node.kind == "capture" && node.status == CaptureStatus::Captured.as_str())
        .count();
    let notes = nodes.iter().filter(|node| node.kind == "note").count();
    let releases = nodes
        .iter()
        .filter(|node| node.kind == "release")
        .map(|node| human_escape(&node.title))
        .collect::<Vec<_>>();
    let mut parts = Vec::new();
    if open > 0 {
        parts.push(format!("Inbox: {open} open"));
    }
    if notes > 0 {
        parts.push(format!("Notes: {notes}"));
    }
    if !releases.is_empty() {
        parts.push(format!("Releases: {}", releases.join(", ")));
    }
    (!parts.is_empty()).then(|| parts.join(view.chars().dot))
}

fn human_action(action: &str) -> &str {
    match action {
        "created" => "Created",
        "updated" => "Updated",
        "discarded" => "Discarded",
        "promoted" => "Promoted",
        "completed" => "Completed",
        "cancelled" => "Cancelled",
        "started" => "Started",
        "parked" => "Parked",
        "unparked" => "Unparked",
        "handed_off" => "Handed off",
        "added" => "Added",
        "removed" => "Removed",
        _ => action,
    }
}

trait PlanningTaskViewOutput {
    fn human(&self, ids: &ShortIds) -> String;
    fn plain(&self) -> String;
}

impl PlanningTaskViewOutput for PlanningTaskView {
    fn human(&self, ids: &ShortIds) -> String {
        let mut lines = vec![
            format!(
                "Task `{}` [{}] {}",
                ids.short(&self.task.id.to_string()),
                self.task.status.as_str(),
                self.task.title
            ),
            format!("Ready: {}", self.readiness.ready),
        ];
        if let Some(assignee) = &self.task.assignee {
            lines.push(format!("Assignee: {assignee}"));
        }
        lines.extend([
            format!(
                "Blockers: {}",
                self.blockers
                    .iter()
                    .chain(&self.inherited_blockers)
                    .map(|blocker| match blocker.ancestor {
                        Some(ancestor) => format!(
                            "{} (via `{}`)",
                            ids.short(&blocker.task.id.to_string()),
                            ids.short(&ancestor.to_string())
                        ),
                        None => ids.short(&blocker.task.id.to_string()).to_owned(),
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            format!(
                "Ancestors: {}",
                self.ancestors
                    .iter()
                    .map(|task| ids.short(&task.id.to_string()).to_owned())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ]);
        if let Some(spec) = &self.spec {
            lines.push(format!("Spec: {}", ids.short(&spec.id.to_string())));
        }
        if let Some(plan) = &self.plan {
            lines.push(format!("Plan: {}", ids.short(&plan.id.to_string())));
        }
        if let Some(phase) = &self.phase {
            lines.push(format!("Phase: {}", ids.short(&phase.id.to_string())));
        }
        lines.extend(
            self.readiness
                .reasons
                .iter()
                .map(|reason| format!("Reason: {}", ids.shorten_text(&reason.message))),
        );
        if !self.task.body.is_empty() {
            lines.push(format!("Body: {}", self.task.body));
        }
        if !self.task.handoff.is_empty() {
            lines.push(format!("Handoff: {}", self.task.handoff));
        }
        if !self.task.evidence.is_empty() {
            lines.push(format!("Evidence: {}", self.task.evidence));
        }
        lines.join("\n")
    }

    fn plain(&self) -> String {
        let mut lines = vec![
            format!(
                "task\t{}\t{}\t{}\t{}\t{}",
                self.task.id,
                self.task.status.as_str(),
                self.task.priority.as_str(),
                self.readiness.ready,
                self.readiness.blocked,
            ),
            format!("title\t{}", plain_escape(&self.task.title)),
            format!("body\t{}", plain_escape(&self.task.body)),
            format!(
                "ancestors\t{}",
                self.ancestors
                    .iter()
                    .map(|task| task.id.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            format!(
                "blockers\t{}",
                self.blockers
                    .iter()
                    .map(|blocker| blocker.task.id.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            format!("handoff\t{}", plain_escape(&self.task.handoff)),
            format!("evidence\t{}", plain_escape(&self.task.evidence)),
        ];
        if let Some(assignee) = &self.task.assignee {
            lines.push(format!("assignee\t{}", plain_escape(assignee)));
        }
        lines.extend(self.inherited_blockers.iter().filter_map(|blocker| {
            blocker
                .ancestor
                .map(|ancestor| format!("inherited\t{}\t{ancestor}", blocker.task.id))
        }));
        lines.extend(
            self.readiness
                .reasons
                .iter()
                .map(|reason| format!("reason\t{}", plain_escape(&reason.message))),
        );
        lines.join("\n")
    }
}

trait PlanningContextOutput {
    fn human(&self) -> String;
    fn plain(&self) -> String;
}

impl PlanningContextOutput for PlanningContext {
    fn human(&self) -> String {
        let mut lines = vec![
            format!(
                "Task `{}` [{}] {}",
                self.task.id,
                self.task.status.as_str(),
                self.task.title
            ),
            format!("Ready: {}", self.readiness.ready),
            format!(
                "Ancestors: {}",
                self.ancestors
                    .iter()
                    .map(|task| task.id.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ];
        if let Some(spec) = &self.spec {
            lines.push(format!("Spec: {}\n{}", spec.id, spec.body));
        }
        if let Some(plan) = &self.plan {
            lines.push(format!("Plan: {}\n{}", plan.id, plan.body));
        }
        if let Some(phase) = &self.phase {
            lines.push(format!("Phase: {}\n{}", phase.id, phase.body));
        }
        lines.extend(self.readiness.reasons.iter().map(|reason| format!("Reason: {reason}")));
        lines.push(format!(
            "Body: {}",
            if self.task.body.is_empty() { "none" } else { &self.task.body }
        ));
        lines.push(format!(
            "Handoff: {}",
            if self.task.handoff.is_empty() { "none" } else { &self.task.handoff }
        ));
        lines.push(format!(
            "Evidence: {}",
            if self.task.evidence.is_empty() { "none" } else { &self.task.evidence }
        ));
        lines.extend(self.blockers.iter().chain(&self.inherited_blockers).map(|blocker| {
            let via = blocker
                .ancestor
                .map(|ancestor| format!(" via `{ancestor}`"))
                .unwrap_or_default();
            format!("Blocker: {} [{}]{via}", blocker.task.id, blocker.task.status.as_str())
        }));
        lines.join("\n")
    }

    fn plain(&self) -> String {
        let mut lines = vec![
            format!(
                "task\t{}\t{}\t{}",
                self.task.id,
                self.task.status.as_str(),
                plain_escape(&self.task.title)
            ),
            format!("ready\t{}\tblocked\t{}", self.readiness.ready, self.readiness.blocked),
            format!("body\t{}", plain_escape(&self.task.body)),
            format!("handoff\t{}", plain_escape(&self.task.handoff)),
            format!("evidence\t{}", plain_escape(&self.task.evidence)),
        ];
        if let Some(spec) = &self.spec {
            lines.push(format!("spec\t{}\t{}", spec.id, plain_escape(&spec.body)));
        }
        if let Some(plan) = &self.plan {
            lines.push(format!("plan\t{}\t{}", plan.id, plain_escape(&plan.body)));
        }
        if let Some(phase) = &self.phase {
            lines.push(format!("phase\t{}\t{}", phase.id, plain_escape(&phase.body)));
        }
        lines.extend(
            self.blockers
                .iter()
                .map(|blocker| format!("blocker\t{}\t{}", blocker.task.id, blocker.task.status.as_str())),
        );
        lines.extend(self.inherited_blockers.iter().filter_map(|blocker| {
            blocker.ancestor.map(|ancestor| {
                format!(
                    "inherited_blocker\t{}\t{}\t{ancestor}",
                    blocker.task.id,
                    blocker.task.status.as_str()
                )
            })
        }));
        lines.extend(
            self.readiness
                .reasons
                .iter()
                .map(|reason| format!("reason\t{}", plain_escape(&reason.message))),
        );
        lines.join("\n")
    }
}

trait PlanDiffOutput {
    fn human(&self) -> String;
    fn plain(&self) -> String;
}

impl PlanDiffOutput for PlanDiff {
    fn human(&self) -> String {
        let mut lines = vec![format!("Plan `{}`", self.plan_id)];
        lines.extend(
            self.phases
                .iter()
                .map(|item| format!("phase\t{}\t{:?}", item.key, item.change)),
        );
        lines.extend(
            self.tasks
                .iter()
                .map(|item| format!("task\t{}\t{:?}", item.key, item.change)),
        );
        lines.extend(
            self.dependencies
                .iter()
                .map(|item| format!("dependency\t{}\t{}\t{:?}", item.task, item.blocker, item.change)),
        );
        lines.join("\n")
    }

    fn plain(&self) -> String {
        let mut lines = Vec::new();
        lines.extend(
            self.phases
                .iter()
                .map(|item| format!("phase\t{}\t{}", item.key, format_change(item.change))),
        );
        lines.extend(
            self.tasks
                .iter()
                .map(|item| format!("task\t{}\t{}", item.key, format_change(item.change))),
        );
        lines.extend(self.dependencies.iter().map(|item| {
            format!(
                "dependency\t{}\t{}\t{}",
                item.task,
                item.blocker,
                format_change(item.change)
            )
        }));
        lines.join("\n")
    }
}

fn format_change(change: arcl_store::PlanChange) -> &'static str {
    match change {
        arcl_store::PlanChange::Create => "create",
        arcl_store::PlanChange::Update => "update",
        arcl_store::PlanChange::Unchanged => "unchanged",
    }
}

/// `value` with each control character written out visibly, as `\n`, `\t`,
/// `\r`, `\x1b` or `\u{85}`, for human tree output. A title holding a
/// newline would otherwise start a fake row, and one holding an escape
/// sequence would reach the terminal. Other text, backslashes included, is
/// left as it is.
fn human_escape(value: &str) -> Cow<'_, str> {
    if !value.chars().any(char::is_control) {
        return Cow::Borrowed(value);
    }
    let mut escaped = String::with_capacity(value.len() + 4);
    for character in value.chars() {
        match character {
            '\n' => escaped.push_str("\\n"),
            '\t' => escaped.push_str("\\t"),
            '\r' => escaped.push_str("\\r"),
            control if u32::from(control) < 0x80 && control.is_control() => {
                escaped.push_str(&format!("\\x{:02x}", u32::from(control)));
            }
            control if control.is_control() => escaped.push_str(&format!("\\u{{{:x}}}", u32::from(control))),
            other => escaped.push(other),
        }
    }
    Cow::Owned(escaped)
}

fn plain_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\t', "\\t").replace('\n', "\\n")
}

fn human_workspace_status(status: &WorkspaceStatus) -> String {
    let mut lines = Vec::new();
    if let Some(head) = &status.head {
        lines.push(format!("Git: {}", head_text(head)));
    }
    if status.is_clean() {
        lines.push("The database and the workspace match the last sync.".to_owned());
        push_paths(&mut lines, MARKER_LABEL, &status.conflict_markers);
        return lines.join("\n");
    }
    if !status.records.is_empty() {
        lines.push("Changed since the last sync:".to_owned());
        for record in &status.records {
            let git = record
                .git
                .map(|state| format!(" (Git: {})", path_state_slug(state)))
                .unwrap_or_default();
            lines.push(format!(
                "  {:<34} {}{git}",
                record_text(record.state, record.database, record.workspace),
                record.path.display()
            ));
        }
    }
    if !status.git_changes.is_empty() {
        lines.push("Changed in Git, matching the last sync:".to_owned());
        lines.extend(
            status
                .git_changes
                .iter()
                .map(|change| format!("  {:<34} {}", path_state_slug(change.git), change.path.display())),
        );
    }
    push_paths(&mut lines, "Unmerged in Git:", &status.unmerged);
    push_paths(&mut lines, MARKER_LABEL, &status.conflict_markers);
    push_paths(&mut lines, "Outside the workspace layout:", &status.unexpected);
    lines.push(
        if status.sync_refuses() {
            "Sync will stop until these are resolved."
        } else if !status.records.is_empty() {
            "Run `arcl workspace sync` to carry these changes."
        } else {
            "Sync has nothing to carry; commit these files in Git."
        }
        .to_owned(),
    );
    lines.join("\n")
}

fn push_paths(lines: &mut Vec<String>, label: &str, paths: &[PathBuf]) {
    if !paths.is_empty() {
        lines.push(label.to_owned());
        lines.extend(paths.iter().map(|path| format!("  {}", path.display())));
    }
}

fn plain_workspace_status(status: &WorkspaceStatus) -> String {
    let mut lines = Vec::new();
    if let Some(head) = &status.head {
        lines.push(match head {
            HeadState::Branch { branch, commit } => format!("head\tbranch\t{}\t{commit}", plain_escape(branch)),
            HeadState::Detached { commit } => format!("head\tdetached\t-\t{commit}"),
            HeadState::Unborn { branch } => format!("head\tunborn\t{}\t-", plain_escape(branch)),
            HeadState::Unknown => "head\tunknown\t-\t-".to_owned(),
        });
    }
    for record in &status.records {
        lines.push(format!(
            "record\t{}\t{}\t{}\t{}\t{}",
            kind_slug(record.state),
            record.database.map_or("-", change_slug),
            record.workspace.map_or("-", change_slug),
            record.git.map_or("-", path_state_slug),
            plain_escape(&record.path.to_string_lossy()),
        ));
    }
    lines.extend(status.git_changes.iter().map(|change| {
        format!(
            "git\t{}\t{}",
            path_state_slug(change.git),
            plain_escape(&change.path.to_string_lossy())
        )
    }));
    for (label, paths) in [
        ("unmerged", &status.unmerged),
        ("conflict-markers", &status.conflict_markers),
        ("unexpected", &status.unexpected),
    ] {
        lines.extend(
            paths
                .iter()
                .map(|path| format!("{label}\t{}", plain_escape(&path.to_string_lossy()))),
        );
    }
    lines.join("\n")
}

fn head_text(head: &HeadState) -> String {
    match head {
        HeadState::Branch { branch, commit } => format!("on branch {branch} at {}", short(commit)),
        HeadState::Detached { commit } => format!("detached HEAD at {}", short(commit)),
        HeadState::Unborn { branch } => format!("on branch {branch} with no commits yet"),
        HeadState::Unknown => "HEAD names no branch or commit".to_owned(),
    }
}

fn short(commit: &str) -> &str {
    commit.get(..7).unwrap_or(commit)
}

fn record_text(kind: RecordStatusKind, database: Option<Change>, workspace: Option<Change>) -> String {
    match (kind, database, workspace) {
        (RecordStatusKind::DatabaseChanged, Some(change), _) => format!("database {}", change_slug(change)),
        (RecordStatusKind::WorkspaceChanged, _, Some(change)) => format!("workspace {}", change_slug(change)),
        (RecordStatusKind::Converged, Some(change), _) => format!("both {}", change_slug(change)),
        (RecordStatusKind::Divergent, Some(database), Some(workspace)) => {
            format!(
                "divergent: database {}, workspace {}",
                change_slug(database),
                change_slug(workspace)
            )
        }
        (kind, _, _) => kind_slug(kind).to_owned(),
    }
}

const fn kind_slug(kind: RecordStatusKind) -> &'static str {
    match kind {
        RecordStatusKind::DatabaseChanged => "database-changed",
        RecordStatusKind::WorkspaceChanged => "workspace-changed",
        RecordStatusKind::Converged => "converged",
        RecordStatusKind::Divergent => "divergent",
    }
}

const fn change_slug(change: Change) -> &'static str {
    match change {
        Change::Added => "added",
        Change::Modified => "modified",
        Change::Removed => "removed",
    }
}

const fn path_state_slug(state: PathState) -> &'static str {
    match state {
        PathState::Tracked => "tracked",
        PathState::Untracked => "untracked",
        PathState::Modified => "modified",
        PathState::Deleted => "deleted",
        PathState::Conflicted => "conflicted",
        PathState::Absent => "absent",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::ColorChoice;

    #[test]
    fn plain_output_is_stable_and_unstyled() {
        let rendered = Renderer::new(OutputMode::Plain, ColorChoice::Always)
            .render_startup()
            .expect("rendering succeeds")
            .expect("plain output is not quiet");

        assert_eq!(rendered, "ready");
        assert!(!rendered.contains('\u{1b}'));
    }

    fn json_version(rendered: Option<String>) -> serde_json::Value {
        let rendered = rendered.expect("JSON output is not quiet");
        serde_json::from_str::<serde_json::Value>(&rendered).expect("JSON parses")["format_version"].clone()
    }

    #[test]
    fn completion_json_uses_the_readiness_version_and_ready_keeps_the_first() {
        let renderer = Renderer::new(OutputMode::Json, ColorChoice::Never);
        let task = PlanningTask::new(
            arcl_core::domain::ProjectId::new(),
            None,
            None,
            None,
            None,
            "Task".to_owned(),
            String::new(),
            arcl_core::domain::TaskPriority::Normal,
            0,
        )
        .expect("task is valid");
        let completion =
            PlanningCompletion { task: task.clone(), completed_subtasks: Vec::new(), unblocked: Vec::new() };

        assert_eq!(
            json_version(renderer.render_planning_completion(&completion).expect("renders")),
            2
        );
        assert_eq!(
            json_version(renderer.render_planning_ready_tasks(&[task]).expect("renders")),
            1
        );
    }

    #[test]
    fn quiet_output_is_empty() {
        assert_eq!(
            Renderer::new(OutputMode::Quiet, ColorChoice::Always)
                .render_startup()
                .expect("rendering succeeds"),
            None
        );
    }

    #[test]
    fn every_status_has_a_mark_and_its_ascii_form() {
        let cases = [
            (
                TreeStatus::Task { status: TaskStatus::Completed, ready: false },
                '✓',
                '+',
            ),
            (
                TreeStatus::Task { status: TaskStatus::Cancelled, ready: false },
                '✗',
                'x',
            ),
            (
                TreeStatus::Task { status: TaskStatus::InProgress, ready: false },
                '◐',
                '~',
            ),
            (TreeStatus::Task { status: TaskStatus::Parked, ready: false }, '‖', '='),
            (TreeStatus::Task { status: TaskStatus::Pending, ready: true }, '●', '*'),
            (TreeStatus::Task { status: TaskStatus::Pending, ready: false }, '○', 'o'),
            (
                TreeStatus::Container { status: ContainerStatus::Completed, started: true },
                '✓',
                '+',
            ),
            (
                TreeStatus::Container { status: ContainerStatus::Cancelled, started: true },
                '✗',
                'x',
            ),
            (
                TreeStatus::Container { status: ContainerStatus::Open, started: true },
                '◐',
                '~',
            ),
            (
                TreeStatus::Container { status: ContainerStatus::Open, started: false },
                '○',
                'o',
            ),
            (TreeStatus::Capture(CaptureStatus::Promoted), '✓', '+'),
            (TreeStatus::Capture(CaptureStatus::Discarded), '✗', 'x'),
            (TreeStatus::Capture(CaptureStatus::Captured), '○', 'o'),
            (TreeStatus::Note, '○', 'o'),
        ];
        for (status, glyph, ascii) in cases {
            let mark = StatusMark::of(status);
            assert_eq!((mark.glyph(), mark.ascii()), (glyph, ascii), "{status:?}");
        }
    }

    #[test]
    fn human_tree_lines_start_with_the_mark_and_plain_and_json_omit_it() {
        let node = ConnectedTreeNode {
            kind: "phase",
            id: "arcl-ph-1".to_owned(),
            title: "Phase".to_owned(),
            status: "open".to_owned(),
            mark: StatusMark::Active,
            blocked_by: Vec::new(),
            children: vec![ConnectedTreeNode {
                kind: "task",
                id: "arcl-t-1".to_owned(),
                title: "Task".to_owned(),
                status: "pending".to_owned(),
                mark: StatusMark::Ready,
                blocked_by: vec!["arcl-t-2".to_owned()],
                children: Vec::new(),
            }],
        };
        let nodes = [node];
        let render = |mode| {
            Renderer::new(mode, ColorChoice::Never)
                .render_connected_tree(&nodes, TreeView::default())
                .expect("rendering succeeds")
                .expect("output is not quiet")
        };
        assert_eq!(
            render(OutputMode::Human),
            "◐ Phase  0/1\n└── ● Task  blocked by arcl-t-2"
        );
        assert_eq!(
            render(OutputMode::Plain),
            "0\tphase\tarcl-ph-1\topen\tPhase\n1\ttask\tarcl-t-1\tpending\tTask"
        );
        let json = serde_json::from_str::<serde_json::Value>(&render(OutputMode::Json)).expect("JSON parses");
        let keys = |value: &serde_json::Value| {
            let mut keys = value
                .as_object()
                .expect("node object")
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            keys.sort();
            keys
        };
        let expected = ["children", "id", "kind", "status", "title"];
        assert_eq!(keys(&json["data"]["nodes"][0]), expected);
        assert_eq!(keys(&json["data"]["nodes"][0]["children"][0]), expected);
    }

    /// A tree node of `kind` with `title`, `mark` and `children`.
    fn node(kind: &'static str, title: &str, mark: StatusMark, children: Vec<ConnectedTreeNode>) -> ConnectedTreeNode {
        let status = if kind == "capture" { "captured" } else { "open" };
        ConnectedTreeNode {
            kind,
            id: format!("id-{title}"),
            title: title.to_owned(),
            status: status.to_owned(),
            mark,
            blocked_by: Vec::new(),
            children,
        }
    }

    /// A spec over a plan with two finished phases, an open one and a
    /// finished one, then a capture, a note and two releases.
    fn sample_tree() -> Vec<ConnectedTreeNode> {
        let task = |title: &str, mark| node("task", title, mark, Vec::new());
        let mut blocked = task("T5: Blocked", StatusMark::Waiting);
        blocked.blocked_by = vec!["1A2B".to_owned()];
        let plan = node(
            "plan",
            "Roadmap",
            StatusMark::Active,
            vec![
                node(
                    "phase",
                    "Milestone 1: First",
                    StatusMark::Done,
                    vec![task("T1: One", StatusMark::Done)],
                ),
                node(
                    "phase",
                    "Milestone 2: Second",
                    StatusMark::Done,
                    vec![node(
                        "task",
                        "T2: Two",
                        StatusMark::Done,
                        vec![task("T2.1: Sub", StatusMark::Done)],
                    )],
                ),
                node(
                    "phase",
                    "Milestone 3: Third",
                    StatusMark::Active,
                    vec![
                        task("T3: Done", StatusMark::Done),
                        task("T4: Active", StatusMark::Active),
                        task("T6: Dropped", StatusMark::Cancelled),
                        blocked,
                    ],
                ),
                node(
                    "phase",
                    "Milestone 4: Fourth",
                    StatusMark::Done,
                    vec![task("T7: Seven", StatusMark::Done)],
                ),
            ],
        );
        vec![
            node("spec", "Spec", StatusMark::Active, vec![plan]),
            node("capture", "Idea", StatusMark::Waiting, Vec::new()),
            node("note", "Decision", StatusMark::Waiting, Vec::new()),
            node("release", "v0.1.0", StatusMark::Waiting, Vec::new()),
            node("release", "v0.2.0", StatusMark::Waiting, Vec::new()),
        ]
    }

    fn human(nodes: &[ConnectedTreeNode], view: TreeView) -> String {
        Renderer::new(OutputMode::Human, ColorChoice::Never)
            .render_connected_tree(nodes, view)
            .expect("rendering succeeds")
            .expect("output is not quiet")
    }

    #[test]
    fn folded_tree_folds_finished_phase_runs_and_ends_with_the_footer() {
        assert_eq!(
            human(&sample_tree(), TreeView::default()),
            "\
◐ Spec
└── ◐ Roadmap
    ├── ✓ Milestones 1–2  done · 3 tasks
    ├── ◐ Milestone 3: Third  1/3
    │   ├── ✓ T3: Done
    │   ├── ◐ T4: Active
    │   ├── ✗ T6: Dropped
    │   └── ○ T5: Blocked  blocked by 1A2B
    └── ✓ Milestone 4: Fourth  done · 1 task
Inbox: 1 open · Notes: 1 · Releases: v0.1.0, v0.2.0"
        );
    }

    #[test]
    fn unfolded_tree_shows_every_phase_and_the_sections() {
        assert_eq!(
            human(&sample_tree(), TreeView { all: true, ..TreeView::default() }),
            "\
◐ Spec
└── ◐ Roadmap
    ├── ✓ Milestone 1: First  1/1
    │   └── ✓ T1: One
    ├── ✓ Milestone 2: Second  2/2
    │   └── ✓ T2: Two
    │       └── ✓ T2.1: Sub
    ├── ◐ Milestone 3: Third  1/3
    │   ├── ✓ T3: Done
    │   ├── ◐ T4: Active
    │   ├── ✗ T6: Dropped
    │   └── ○ T5: Blocked  blocked by 1A2B
    └── ✓ Milestone 4: Fourth  1/1
        └── ✓ T7: Seven
Inbox
└── ○ Idea
Notes
└── ○ Decision
Releases
├── ○ v0.1.0
└── ○ v0.2.0"
        );
    }

    #[test]
    fn ascii_tree_uses_only_ascii() {
        let folded = human(&sample_tree(), TreeView { ascii: true, ..TreeView::default() });
        assert_eq!(
            folded,
            "\
~ Spec
`-- ~ Roadmap
    |-- + Milestones 1-2  done - 3 tasks
    |-- ~ Milestone 3: Third  1/3
    |   |-- + T3: Done
    |   |-- ~ T4: Active
    |   |-- x T6: Dropped
    |   `-- o T5: Blocked  blocked by 1A2B
    `-- + Milestone 4: Fourth  done - 1 task
Inbox: 1 open - Notes: 1 - Releases: v0.1.0, v0.2.0"
        );
        let all = human(
            &sample_tree(),
            TreeView { ascii: true, all: true, ..TreeView::default() },
        );
        assert!(all.is_ascii(), "{all}");
    }

    #[test]
    fn depth_limits_the_record_levels() {
        assert_eq!(
            human(&sample_tree(), TreeView { depth: Some(3), ..TreeView::default() }),
            "\
◐ Spec
└── ◐ Roadmap
    ├── ✓ Milestones 1–2  done · 3 tasks
    ├── ◐ Milestone 3: Third  1/3
    └── ✓ Milestone 4: Fourth  done · 1 task
Inbox: 1 open · Notes: 1 · Releases: v0.1.0, v0.2.0"
        );
        assert_eq!(
            human(
                &sample_tree(),
                TreeView { depth: Some(1), all: true, ..TreeView::default() }
            ),
            "◐ Spec\nInbox\n└── ○ Idea\nNotes\n└── ○ Decision\nReleases\n├── ○ v0.1.0\n└── ○ v0.2.0"
        );
    }

    #[test]
    fn rooted_tree_prints_the_root_without_folding_it_or_a_footer() {
        let phase = node(
            "phase",
            "Milestone 1: First",
            StatusMark::Done,
            vec![node("task", "T1: One", StatusMark::Done, Vec::new())],
        );
        let rooted = TreeView { rooted: true, ..TreeView::default() };
        assert_eq!(
            human(std::slice::from_ref(&phase), rooted),
            "✓ Milestone 1: First  1/1\n└── ✓ T1: One"
        );
        let capture = node("capture", "Idea", StatusMark::Waiting, Vec::new());
        assert_eq!(human(std::slice::from_ref(&capture), rooted), "○ Idea");
    }

    #[test]
    fn footer_leaves_out_empty_parts_and_runs_without_numbers_count_phases() {
        let nodes = [
            node(
                "plan",
                "Plan",
                StatusMark::Active,
                vec![
                    node("phase", "Design", StatusMark::Done, Vec::new()),
                    node("phase", "Build", StatusMark::Done, Vec::new()),
                ],
            ),
            node("note", "Decision", StatusMark::Waiting, Vec::new()),
        ];
        assert_eq!(
            human(&nodes, TreeView::default()),
            "◐ Plan\n└── ✓ 2 phases  done · 0 tasks\nNotes: 1"
        );
        assert_eq!(
            human(&nodes[..1], TreeView::default()),
            "◐ Plan\n└── ✓ 2 phases  done · 0 tasks"
        );
    }

    #[test]
    fn completed_phase_with_open_tasks_stays_expanded() {
        let task = |title: &str, mark| node("task", title, mark, Vec::new());
        let mut blocked = task("T3: Next", StatusMark::Waiting);
        blocked.blocked_by = vec!["T2".to_owned()];
        let nodes = [node(
            "plan",
            "Plan",
            StatusMark::Active,
            vec![
                node(
                    "phase",
                    "Milestone 1: First",
                    StatusMark::Done,
                    vec![task("T1: One", StatusMark::Done)],
                ),
                node(
                    "phase",
                    "Milestone 2: Second",
                    StatusMark::Done,
                    vec![
                        task("T2: Open", StatusMark::Waiting),
                        task("T4: Dropped", StatusMark::Cancelled),
                    ],
                ),
                node("phase", "Milestone 3: Third", StatusMark::Active, vec![blocked]),
            ],
        )];
        assert_eq!(
            human(&nodes, TreeView::default()),
            "\
◐ Plan
├── ✓ Milestone 1: First  done · 1 task
├── ✓ Milestone 2: Second  0/1
│   ├── ○ T2: Open
│   └── ✗ T4: Dropped
└── ◐ Milestone 3: Third  0/1
    └── ○ T3: Next  blocked by T2"
        );
    }

    /// [`sample_tree`] with IDs numbered in tree order from `1`, so they
    /// differ in width, and each task's stored status read off its mark.
    fn numbered_tree() -> Vec<ConnectedTreeNode> {
        fn number(nodes: &mut [ConnectedTreeNode], next: &mut usize) {
            for node in nodes {
                *next += 1;
                node.id = next.to_string();
                node.status = match (node.kind, node.mark) {
                    (_, StatusMark::Done) => "completed",
                    (_, StatusMark::Cancelled) => "cancelled",
                    ("task", StatusMark::Active) => "in_progress",
                    ("task", _) => "pending",
                    _ => node.status.as_str(),
                }
                .to_owned();
                number(&mut node.children, next);
            }
        }
        let mut nodes = sample_tree();
        number(&mut nodes, &mut 0);
        nodes
    }

    #[test]
    fn full_tree_unfolds_every_phase_and_aligns_short_ids_on_the_right() {
        let full = TreeView { style: TreeStyle::Full, ..TreeView::default() };
        assert_eq!(
            human(&numbered_tree(), full),
            "\
◐ Spec                                       1
└── ◐ Roadmap                                2
    ├── ✓ Milestone 1: First  1/1            3
    │   └── ✓ T1: One                        4
    ├── ✓ Milestone 2: Second  2/2           5
    │   └── ✓ T2: Two                        6
    │       └── ✓ T2.1: Sub                  7
    ├── ◐ Milestone 3: Third  1/3            8
    │   ├── ✓ T3: Done                       9
    │   ├── ◐ T4: Active                    10
    │   ├── ✗ T6: Dropped                   11
    │   └── ○ T5: Blocked  blocked by 1A2B  12
    └── ✓ Milestone 4: Fourth  1/1          13
        └── ✓ T7: Seven                     14
Inbox: 1 open · Notes: 1 · Releases: v0.1.0, v0.2.0"
        );
    }

    #[test]
    fn full_tree_follows_ascii_all_and_depth() {
        let view = TreeView { style: TreeStyle::Full, ascii: true, all: true, depth: Some(2), ..TreeView::default() };
        assert_eq!(
            human(&numbered_tree(), view),
            "\
~ Spec           1
`-- ~ Roadmap    2
Inbox
`-- o Idea      15
Notes
`-- o Decision  16
Releases
|-- o v0.1.0    17
`-- o v0.2.0    18"
        );
    }

    #[test]
    fn full_tree_measures_wide_titles_in_terminal_columns() {
        let mut spec = node(
            "spec",
            "日本語のタスク",
            StatusMark::Active,
            vec![node("task", "Plain", StatusMark::Waiting, Vec::new())],
        );
        spec.id = "1".to_owned();
        spec.children[0].id = "2".to_owned();
        let full = TreeView { style: TreeStyle::Full, ..TreeView::default() };
        assert_eq!(human(&[spec], full), "◐ 日本語のタスク  1\n└── ○ Plain       2");
    }

    #[test]
    fn outline_tree_escapes_a_newline_so_it_adds_no_row() {
        let mut task = node(
            "task",
            "Line one\nin_progress  FAKE  injected",
            StatusMark::Waiting,
            Vec::new(),
        );
        task.id = "1".to_owned();
        let outline = TreeView { style: TreeStyle::Outline, ..TreeView::default() };
        assert_eq!(
            human(&[task], outline),
            "STATUS  ID  TITLE\nopen    1   Line one\\nin_progress  FAKE  injected"
        );
    }

    #[test]
    fn full_tree_measures_tab_and_escape_titles_after_escaping_them() {
        let mut spec = node(
            "spec",
            "A\tB",
            StatusMark::Active,
            vec![node("task", "\u{1b}[31mRed", StatusMark::Waiting, Vec::new())],
        );
        spec.id = "1".to_owned();
        spec.children[0].id = "2".to_owned();
        let full = TreeView { style: TreeStyle::Full, ..TreeView::default() };
        assert_eq!(human(&[spec], full), "◐ A\\tB             1\n└── ○ \\x1b[31mRed  2");
    }

    #[test]
    fn folded_tree_escapes_control_characters_in_titles_and_the_footer() {
        let task = node("task", "Bell\u{7} and\r\nbreak", StatusMark::Waiting, Vec::new());
        let release = node("release", "v1\n", StatusMark::Done, Vec::new());
        assert_eq!(
            human(&[task, release], TreeView::default()),
            "○ Bell\\x07 and\\r\\nbreak\nReleases: v1\\n"
        );
    }

    #[test]
    fn outline_tree_prints_status_id_and_title_columns_with_dots() {
        let outline = TreeView { style: TreeStyle::Outline, ..TreeView::default() };
        assert_eq!(
            human(&numbered_tree(), outline),
            "\
STATUS       ID  TITLE
open         1   Spec
open         2   . Roadmap
completed    3   . . Milestone 1: First
completed    4   . . . T1: One
completed    5   . . Milestone 2: Second
completed    6   . . . T2: Two
completed    7   . . . . T2.1: Sub
open         8   . . Milestone 3: Third
completed    9   . . . T3: Done
in_progress  10  . . . T4: Active
cancelled    11  . . . T6: Dropped
pending      12  . . . T5: Blocked
completed    13  . . Milestone 4: Fourth
completed    14  . . . T7: Seven
Inbox: 1 open · Notes: 1 · Releases: v0.1.0, v0.2.0"
        );
    }

    #[test]
    fn outline_tree_follows_ascii_all_and_depth() {
        let view =
            TreeView { style: TreeStyle::Outline, ascii: true, all: true, depth: Some(2), ..TreeView::default() };
        let outline = human(&numbered_tree(), view);
        assert_eq!(
            outline,
            "\
STATUS    ID  TITLE
open      1   Spec
open      2   . Roadmap
              Inbox
captured  15  . Idea
              Notes
open      16  . Decision
              Releases
open      17  . v0.1.0
open      18  . v0.2.0"
        );
        let footer = human(
            &numbered_tree(),
            TreeView { style: TreeStyle::Outline, ascii: true, depth: Some(1), ..TreeView::default() },
        );
        assert_eq!(
            footer,
            "STATUS  ID  TITLE\nopen    1   Spec\nInbox: 1 open - Notes: 1 - Releases: v0.1.0, v0.2.0"
        );
        let rooted = TreeView { style: TreeStyle::Outline, rooted: true, ..TreeView::default() };
        assert_eq!(human(&[], rooted), "");
    }

    #[test]
    fn folded_range_needs_a_known_word_and_consecutive_numbers() {
        let phases = |titles: &[&str]| -> Vec<ConnectedTreeNode> {
            titles
                .iter()
                .map(|title| node("phase", title, StatusMark::Done, Vec::new()))
                .collect()
        };
        let range = |titles: &[&str]| {
            let nodes = phases(titles);
            folded_range(&nodes.iter().collect::<Vec<_>>(), "–")
        };
        assert_eq!(
            range(&["Phase 2: A", "Phase 3: B", "Phase 4: C"]).as_deref(),
            Some("Phases 2–4")
        );
        assert_eq!(range(&["Batch 1", "Batch 2"]), None);
        assert_eq!(range(&["Milestone 1: A", "Milestone 5: B"]), None);
        assert_eq!(range(&["Milestone 1: A", "Milestone 3: B", "Milestone 3: C"]), None);
        assert_eq!(range(&["Milestone 1: A", "Phase 2: B"]), None);
    }
}
