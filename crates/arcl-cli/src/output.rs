use std::path::Path;

use owo_colors::{OwoColorize, Stream};
use serde::Serialize;
use thiserror::Error;

use crate::cli::ColorChoice;
use arcl_core::domain::{Capture, Note, Phase, Plan, PlanningTask, Release, Spec, TaskDependency};
use arcl_store::{CheckReport, PlanApplyResult, PlanDiff, PlanningContext, PlanningTaskView};

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

/// Output failures that can occur before the application boundary adds context.
#[derive(Debug, Error)]
pub enum OutputError {
    #[error("could not serialize JSON output: {0}")]
    Json(#[from] serde_json::Error),
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

    /// Render the result of starting the foundation-only application.
    pub fn render_startup(&self) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => self.msg(),
            OutputMode::Plain => "ready".to_owned(),
            OutputMode::Json => serde_json::to_string(&Envelope {
                format_version: 1,
                data: StartupData { status: "ready", message: "Arc Lightning foundation ready" },
            })?,
            OutputMode::Quiet => return Ok(None),
        };

        Ok(Some(message))
    }

    /// Render the result of project initialization.
    pub fn render_init(&self, root: &Path, snapshot_enabled: bool) -> Result<Option<String>, OutputError> {
        let root = root.to_string_lossy();
        let message = match self.mode {
            OutputMode::Human => {
                let snapshot = if snapshot_enabled { " with snapshots enabled" } else { "" };
                format!("Initialized Arc Lightning in `{root}`{snapshot}")
            }
            OutputMode::Plain => format!("initialized\t{root}"),
            OutputMode::Json => serde_json::to_string(&Envelope {
                format_version: 1,
                data: InitData { status: "initialized", root: root.as_ref(), snapshot_enabled },
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

    /// Render a connected task inspection view.
    pub fn render_planning_task_view(&self, view: &PlanningTaskView) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => view.human(),
            OutputMode::Plain => view.plain(),
            OutputMode::Json => serde_json::to_string(&Envelope { format_version: 1, data: view })?,
            OutputMode::Quiet => return Ok(None),
        };
        Ok(Some(self.style(message)))
    }

    /// Render a focused connected task context packet.
    pub fn render_planning_context(&self, context: &PlanningContext) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => context.human(),
            OutputMode::Plain => context.plain(),
            OutputMode::Json => serde_json::to_string(&Envelope { format_version: 1, data: context })?,
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
                serde_json::to_string(&Envelope { format_version: 1, data: ReadyPlanningData { tasks } })?
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
                serde_json::to_string(&Envelope { format_version: 1, data: NextPlanningData { task } })?
            }
            OutputMode::Quiet => return Ok(None),
        };
        if self.mode == OutputMode::Plain && task.is_none() {
            return Ok(None);
        }
        Ok(Some(self.style(message)))
    }

    /// Render structured plan changes.
    pub fn render_plan_diff(&self, diff: &PlanDiff) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => diff.human(),
            OutputMode::Plain => diff.plain(),
            OutputMode::Json => serde_json::to_string(&Envelope { format_version: 1, data: diff })?,
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
            OutputMode::Json => serde_json::to_string(&Envelope { format_version: 1, data: result })?,
            OutputMode::Quiet => return Ok(None),
        };
        Ok(Some(self.style(message)))
    }

    /// Render compact summaries for the broad connected-record query.
    pub fn render_connected_summaries(&self, records: &[ConnectedSummary]) -> Result<Option<String>, OutputError> {
        self.render_collection("records", records, records)
    }

    /// Render the connected planning hierarchy.
    pub fn render_connected_tree(&self, nodes: &[ConnectedTreeNode]) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => nodes.iter().map(|node| node.human(0)).collect::<Vec<_>>().join("\n"),
            OutputMode::Plain => nodes
                .iter()
                .flat_map(|node| node.plain_lines(0))
                .collect::<Vec<_>>()
                .join("\n"),
            OutputMode::Json => serde_json::to_string(&Envelope { format_version: 1, data: TreeNodesData { nodes } })?,
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
            OutputMode::Json => serde_json::to_string(&Envelope { format_version: 1, data: detail })?,
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
                serde_json::to_string(&Envelope { format_version: 1, data: serde_json::Value::Object(data) })?
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
                serde_json::to_string(&Envelope { format_version: 1, data: serde_json::Value::Object(data) })?
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
                serde_json::to_string(&Envelope { format_version: 1, data: serde_json::Value::Object(data) })?
            }
            OutputMode::Quiet => return Ok(None),
        };
        Ok(Some(self.style(message)))
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
                serde_json::to_string(&Envelope { format_version: 1, data: serde_json::Value::Object(data) })?
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
                serde_json::to_string(&Envelope { format_version: 1, data: serde_json::Value::Object(data) })?
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
                format_version: 1,
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
                format_version: 1,
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
            OutputMode::Json => serde_json::to_string(&Envelope { format_version: 1, data: report })?,
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

#[derive(Debug, Serialize)]
struct StartupData {
    status: &'static str,
    message: &'static str,
}

#[derive(Debug, Serialize)]
struct InitData<'a> {
    status: &'static str,
    root: &'a str,
    snapshot_enabled: bool,
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
    /// Child records in the planning hierarchy.
    pub children: Vec<ConnectedTreeNode>,
}

impl ConnectedTreeNode {
    fn human(&self, depth: usize) -> String {
        let mut lines = vec![format!(
            "{}{} `{}` [{}]",
            "  ".repeat(depth),
            self.kind,
            self.id,
            self.title
        )];
        lines.extend(self.children.iter().map(|child| child.human(depth + 1)));
        lines.join("\n")
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
struct ReadyPlanningData<'a> {
    tasks: &'a [PlanningTask],
}

#[derive(Debug, Serialize)]
struct NextPlanningData<'a> {
    task: Option<&'a PlanningTask>,
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
    fn human(&self) -> String;
    fn plain(&self) -> String;
}

impl PlanningTaskViewOutput for PlanningTaskView {
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
                "Blockers: {}",
                self.blockers
                    .iter()
                    .map(|blocker| blocker.task.id.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
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
            lines.push(format!("Spec: {}", spec.id));
        }
        if let Some(plan) = &self.plan {
            lines.push(format!("Plan: {}", plan.id));
        }
        if let Some(phase) = &self.phase {
            lines.push(format!("Phase: {}", phase.id));
        }
        lines.extend(self.readiness.reasons.iter().map(|reason| format!("Reason: {reason}")));
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
        lines.extend(
            self.readiness
                .reasons
                .iter()
                .map(|reason| format!("reason\t{}", plain_escape(reason))),
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
        lines.extend(
            self.blockers
                .iter()
                .map(|blocker| format!("Blocker: {} [{}]", blocker.task.id, blocker.task.status.as_str())),
        );
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
        lines.extend(
            self.readiness
                .reasons
                .iter()
                .map(|reason| format!("reason\t{}", plain_escape(reason))),
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

fn plain_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\t', "\\t").replace('\n', "\\n")
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

    #[test]
    fn quiet_output_is_empty() {
        assert_eq!(
            Renderer::new(OutputMode::Quiet, ColorChoice::Always)
                .render_startup()
                .expect("rendering succeeds"),
            None
        );
    }
}
