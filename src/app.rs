use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::{fs, fs::OpenOptions};

use anyhow::Context;
use thiserror::Error;

use crate::snapshot::{
    ProjectConfig, SnapshotError, SnapshotExportError, SnapshotImportError, SnapshotManifest, encode_manifest,
    export_graph, import_snapshot, resolve_snapshot_root,
};
use crate::storage::{
    CaptureTaskPromotion, CaptureUpdate, ConnectedGraph, Database, ListFilter, NoteUpdate, PhaseUpdate, PlanUpdate,
    PlanningReadyFilter, PlanningTaskCreate, PlanningTaskUpdate, SnapshotBaseFile, SpecUpdate, StorageError,
};
use crate::vcs::{GixVcs, Vcs, VcsError};
use crate::{cli::*, domain::*, output::*};

const ARCL_DIRECTORY: &str = ".arcl";
const CONFIG_FILE: &str = "config.toml";
const DATABASE_FILE: &str = "arcl.db";
const GITIGNORE_FILE: &str = ".gitignore";
const REQUIRED_GITIGNORE_ENTRIES: &[&str] = &["/arcl.db", "/arcl.db-*", "/*.tmp", "/conflicts/"];
const SNAPSHOT_DIRECTORIES: &[&str] = &["ideas", "releases", "epics", "milestones", "tasks"];

type CResult<T> = Result<T, CommandError>;
type IResult<T> = Result<T, InitError>;

/// A typed application failure carrying the process exit category.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct ApplicationError {
    message: String,
    exit_code: u8,
}

impl ApplicationError {
    /// Return the stable process exit category for this failure.
    pub const fn exit_code(&self) -> u8 {
        self.exit_code
    }

    /// Render a stable machine-readable error envelope.
    pub fn json(&self) -> String {
        serde_json::json!({
            "format_version": 1,
            "error": {
                "code": match self.exit_code {
                    3 => "invalid_request",
                    4 => "conflict",
                    5 => "not_found",
                    _ => "application_error",
                },
                "message": self.message,
                "exit_code": self.exit_code,
            }
        })
        .to_string()
    }

    fn from_init(error: InitError) -> Self {
        let exit_code = error.exit_code();
        let message = if exit_code == 3 { format!("invalid project: {error}") } else { error.to_string() };
        Self { message, exit_code }
    }

    fn from_command<E>(error: E) -> Self
    where
        E: Into<CommandError>,
    {
        let error: CommandError = error.into();
        let exit_code = error.exit_code();
        let message = if exit_code == 3 { format!("invalid project or record: {error}") } else { error.to_string() };
        Self { message, exit_code }
    }
}

#[derive(Debug, Error)]
enum InitError {
    #[error(transparent)]
    Vcs(#[from] VcsError),
    #[error("could not create Arc Lightning directory `{path}`: {source}")]
    CreateDirectory { path: PathBuf, source: io::Error },
    #[error("could not read project configuration `{path}`: {source}")]
    ReadConfig { path: PathBuf, source: io::Error },
    #[error("project configuration `{path}` is invalid: {source}")]
    InvalidConfig { path: PathBuf, source: SnapshotError },
    #[error("could not write project configuration `{path}`: {source}")]
    WriteConfig { path: PathBuf, source: io::Error },
    #[error("could not render project configuration `{path}`: {source}")]
    RenderConfig { path: PathBuf, source: SnapshotError },
    #[error("could not create snapshot directory `{path}`: {source}")]
    CreateSnapshotDirectory { path: PathBuf, source: io::Error },
    #[error("could not render snapshot manifest `{path}`: {source}")]
    RenderSnapshotManifest { path: PathBuf, source: SnapshotError },
    #[error("could not write snapshot manifest `{path}`: {source}")]
    WriteSnapshotManifest { path: PathBuf, source: io::Error },
    #[error("could not read scoped ignore file `{path}`: {source}")]
    ReadGitignore { path: PathBuf, source: io::Error },
    #[error("could not write scoped ignore file `{path}`: {source}")]
    WriteGitignore { path: PathBuf, source: io::Error },
    #[error("could not initialize database `{path}`: {source}")]
    OpenDatabase { path: PathBuf, source: StorageError },
}

impl InitError {
    fn exit_code(&self) -> u8 {
        match self {
            Self::Vcs(
                VcsError::Discovery { .. } | VcsError::BareRepository { .. } | VcsError::MissingWorktree { .. },
            )
            | Self::InvalidConfig { .. } => 3,
            _ => 1,
        }
    }
}

#[derive(Debug, Error)]
enum CommandError {
    #[error(transparent)]
    Vcs(#[from] VcsError),
    #[error("Arc Lightning is not initialized in project directory `{root}`; run `arcl init` first")]
    NotInitialized { root: PathBuf },
    #[error("could not read project configuration `{path}`: {source}")]
    ReadConfig { path: PathBuf, source: io::Error },
    #[error("could not determine the current directory: {source}")]
    CurrentDirectory { source: io::Error },
    #[error("project configuration `{path}` is invalid: {source}")]
    InvalidConfig { path: PathBuf, source: SnapshotError },
    #[error("could not open Arc Lightning database `{path}`: {source}")]
    OpenDatabase { path: PathBuf, source: StorageError },
    #[error("snapshot export is not enabled in project `{root}`")]
    SnapshotDisabled { root: PathBuf },
    #[error(transparent)]
    SnapshotExport(#[from] SnapshotExportError),
    #[error(transparent)]
    SnapshotImport(Box<SnapshotImportError>),
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("Markdown file `{path}` could not be read: {source}")]
    ReadMarkdown { path: PathBuf, source: io::Error },
    #[error("Markdown file `{path}` is not valid UTF-8")]
    InvalidMarkdown { path: PathBuf },
    #[error("standard input could not be read: {source}")]
    ReadStdin { source: io::Error },
    #[error("standard input is a terminal; pipe UTF-8 Markdown to a `*-file -` option or use an inline option")]
    StdinIsTerminal,
    #[error("standard input is not valid UTF-8")]
    InvalidStdin,
    #[error("invalid list filter: {message}")]
    InvalidFilter { message: String },
    #[error("database or graph integrity check failed: {message}")]
    Integrity { message: String },
}

impl From<SnapshotImportError> for CommandError {
    fn from(error: SnapshotImportError) -> Self {
        Self::SnapshotImport(Box::new(error))
    }
}

impl CommandError {
    fn exit_code(&self) -> u8 {
        match self {
            Self::Vcs(error) => match error {
                VcsError::Discovery { .. } | VcsError::BareRepository { .. } | VcsError::MissingWorktree { .. } => 3,
                VcsError::PathOutsideWorktree { .. } | VcsError::Operation { .. } => 1,
            },
            Self::NotInitialized { .. }
            | Self::InvalidConfig { .. }
            | Self::SnapshotDisabled { .. }
            | Self::Domain(_) => 3,
            Self::SnapshotExport(error) => match error {
                SnapshotExportError::Conflict { .. } => 4,
                _ => 1,
            },
            Self::SnapshotImport(error) => error.exit_code(),
            Self::ReadConfig { .. }
            | Self::ReadMarkdown { .. }
            | Self::ReadStdin { .. }
            | Self::OpenDatabase { source: StorageError::Sqlite(_), .. }
            | Self::CurrentDirectory { .. } => 1,
            Self::OpenDatabase { source, .. } => u8::from(source),
            Self::Storage(error) => u8::from(error),
            Self::InvalidMarkdown { .. }
            | Self::InvalidStdin
            | Self::InvalidFilter { .. }
            | Self::Integrity { .. }
            | Self::StdinIsTerminal => 3,
        }
    }
}

impl From<&StorageError> for u8 {
    fn from(value: &StorageError) -> Self {
        match value {
            StorageError::ProjectNotFound => 3,
            StorageError::IdeaNotFound { .. }
            | StorageError::ReleaseNotFound { .. }
            | StorageError::EpicNotFound { .. }
            | StorageError::MilestoneNotFound { .. }
            | StorageError::TaskNotFound { .. }
            | StorageError::DependencyNotFound { .. }
            | StorageError::CaptureNotFound { .. }
            | StorageError::SpecNotFound { .. }
            | StorageError::PlanNotFound { .. }
            | StorageError::PhaseNotFound { .. }
            | StorageError::PlanningTaskNotFound { .. }
            | StorageError::NoteNotFound { .. }
            | StorageError::ReleaseMembershipNotFound { .. }
            | StorageError::NoteLinkNotFound { .. }
            | StorageError::RecordLinkNotFound { .. }
            | StorageError::PlanningDependencyNotFound { .. } => 5,
            StorageError::InvalidProject(_)
            | StorageError::InvalidCapture(_)
            | StorageError::InvalidSpec(_)
            | StorageError::InvalidPlan(_)
            | StorageError::InvalidPhase(_)
            | StorageError::InvalidPlanningTask(_)
            | StorageError::InvalidNote(_)
            | StorageError::InvalidMembership(_)
            | StorageError::InvalidLink(_)
            | StorageError::InvalidPlanningDependency(_)
            | StorageError::InvalidIdea(_)
            | StorageError::InvalidRelease(_)
            | StorageError::InvalidEpic(_)
            | StorageError::InvalidMilestone(_)
            | StorageError::InvalidTask(_)
            | StorageError::InvalidDependency(_)
            | StorageError::DuplicateSpec { .. }
            | StorageError::CaptureNotPromotable { .. }
            | StorageError::AmbiguousCapturePromotion { .. }
            | StorageError::InvalidPlanInput(_)
            | StorageError::IdeaNotPromotable { .. }
            | StorageError::InconsistentPromotion { .. }
            | StorageError::NewerDatabase { .. }
            | StorageError::MigrationGap { .. } => 3,
            StorageError::Sqlite(_) => 1,
        }
    }
}

struct OpenProject {
    root: PathBuf,
    config: ProjectConfig,
    database: Database,
}

struct ProjectLocation {
    root: PathBuf,
    config: ProjectConfig,
    database_path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Initialization {
    root: PathBuf,
    snapshot_enabled: bool,
}

enum CaptureResult {
    Mutation { action: &'static str, capture: Capture },
    Promotion(Box<crate::storage::CapturePromotionResult>),
    List(Vec<Capture>),
}

enum PlanResult {
    Mutation { action: &'static str, plan: Plan },
    Detail(PlanDetail),
    Diff(crate::storage::PlanDiff),
    Applied(crate::storage::PlanApplyResult),
    List(Vec<Plan>),
}

/// Run one parsed CLI invocation at the application boundary.
pub fn run(cli: Cli) -> anyhow::Result<()> {
    let mode = if cli.json {
        OutputMode::Json
    } else if cli.plain {
        OutputMode::Plain
    } else if cli.quiet {
        OutputMode::Quiet
    } else {
        OutputMode::Human
    };
    let renderer = Renderer::new(mode, cli.color);

    match cli.command {
        Some(Command::Init { snapshot }) => {
            let start = std::env::current_dir().context("could not determine the current directory")?;
            let initialization = initialize(&start, snapshot).map_err(ApplicationError::from_init)?;
            write_output(renderer.render_init(&initialization.root, initialization.snapshot_enabled)?)
        }
        Some(Command::Capture { command }) => {
            let result = exec_capture(command).map_err(ApplicationError::from_command)?;
            let message = match result {
                CaptureResult::Mutation { action, capture } => renderer.render_capture(action, &capture),
                CaptureResult::Promotion(result) => renderer.render_capture_promotion(&result),
                CaptureResult::List(captures) => renderer.render_captures(&captures),
            }?;
            write_output(message)
        }
        Some(Command::Release { command }) => exec_release(command, &renderer),
        Some(Command::Spec { command }) => exec_spec(command, &renderer),
        Some(Command::Plan { command }) => exec_plan(command, &renderer),
        Some(Command::Task { command }) => exec_task(command, &renderer),
        Some(Command::Dependency { command }) => exec_dependency(command, &renderer),
        Some(Command::Note { command }) => exec_note(command, &renderer),
        Some(Command::Show { id }) => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            let message = show_record(&database, &id, &renderer).map_err(ApplicationError::from_command)?;
            write_output(message)
        }
        Some(Command::List { filters }) => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            let summaries = list_connected(&database, filters).map_err(ApplicationError::from_command)?;
            write_output(renderer.render_connected_summaries(&summaries)?)
        }
        Some(Command::Tree { id }) => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            let graph = database.connected_graph().map_err(ApplicationError::from_command)?;
            let nodes = connected_tree(&graph, id.as_deref()).map_err(ApplicationError::from_command)?;
            write_output(renderer.render_connected_tree(&nodes)?)
        }
        Some(Command::Explain { task_id }) => {
            let id = parse_task_id(&task_id).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            let view = database
                .planning_task_view(id)
                .map_err(ApplicationError::from_command)?;
            write_output(renderer.render_planning_task_view(&view)?)
        }
        Some(Command::Context { task_id }) => {
            let id = parse_task_id(&task_id).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            let context = database.planning_context(id).map_err(ApplicationError::from_command)?;
            write_output(renderer.render_planning_context(&context)?)
        }
        Some(Command::Check) => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            let report = database.check().map_err(ApplicationError::from_command)?;
            let valid = report.valid;
            let details = report.errors.join("; ");
            write_output(renderer.render_check(&report)?)?;
            if valid {
                Ok(())
            } else {
                Err(ApplicationError::from_command(CommandError::Integrity { message: details }).into())
            }
        }
        Some(Command::Snapshot { command }) => {
            exec_snapshot(command).map_err(ApplicationError::from_command)?;
            write_output(None)
        }
        Some(Command::Ready { filters }) => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            let filter = resolve_ready_filter(filters).map_err(ApplicationError::from_command)?;
            let tasks = database
                .ready_planning_tasks_filtered(&filter)
                .map_err(ApplicationError::from_command)?;
            write_output(renderer.render_planning_ready_tasks(&tasks)?)
        }
        Some(Command::Next { filters }) => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            let filter = resolve_ready_filter(filters).map_err(ApplicationError::from_command)?;
            let task = database
                .ready_planning_tasks_filtered(&filter)
                .map_err(ApplicationError::from_command)?
                .into_iter()
                .next();
            write_output(renderer.render_next_planning_task(task.as_ref())?)
        }
        None => write_output(renderer.render_startup()?),
    }
}

fn write_output(message: Option<String>) -> anyhow::Result<()> {
    if let Some(message) = message {
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        writeln!(stdout, "{message}").context("writing CLI output")?;
    }
    Ok(())
}

fn exec_capture(command: CaptureCommand) -> CResult<CaptureResult> {
    match command {
        CaptureCommand::Create { title, body } => {
            validate_title(&title)?;
            let body = resolve_markdown(body)?.unwrap_or_default();
            let mut database = open_database()?;
            Ok(CaptureResult::Mutation { action: "created", capture: database.create_capture(title, body)? })
        }
        CaptureCommand::Show { id } => {
            let id = parse_capture_id(&id)?;
            let database = open_database()?;
            let capture = database
                .capture(id)?
                .ok_or_else(|| StorageError::CaptureNotFound { id: id.to_string() })?;
            Ok(CaptureResult::Mutation { action: "shown", capture })
        }
        CaptureCommand::List => Ok(CaptureResult::List(open_database()?.captures()?)),
        CaptureCommand::Update { id, title, body } => {
            let id = parse_capture_id(&id)?;
            if let Some(title) = &title {
                validate_title(title)?;
            }
            let body = resolve_markdown(body)?;
            let mut database = open_database()?;
            Ok(CaptureResult::Mutation {
                action: "updated",
                capture: database.update_capture(id, CaptureUpdate { title, body })?,
            })
        }
        CaptureCommand::Discard { id } => {
            let id = parse_capture_id(&id)?;
            let mut database = open_database()?;
            Ok(CaptureResult::Mutation { action: "discarded", capture: database.discard_capture(id)? })
        }
        CaptureCommand::Promote {
            id,
            target,
            to,
            title,
            body,
            acceptance_criteria,
            acceptance_criteria_file,
            spec,
            plan,
            phase,
            parent,
            priority,
            position,
        } => {
            let id = parse_capture_id(&id)?;
            let target = target.or(to).ok_or_else(|| CommandError::InvalidFilter {
                message: "capture promotion requires a target: spec, task, or note".to_owned(),
            })?;
            let target = target.to_ascii_lowercase();
            let mut database = open_database()?;
            let capture = database
                .capture(id)?
                .ok_or_else(|| StorageError::CaptureNotFound { id: id.to_string() })?;
            let title = title.unwrap_or_else(|| capture.title.clone());
            let body = resolve_markdown(body)?.unwrap_or_else(|| capture.body.clone());
            let acceptance_criteria =
                resolve_optional_value(acceptance_criteria, acceptance_criteria_file)?.unwrap_or_default();
            let result = match target.as_str() {
                "spec" => {
                    reject_promotion_task_fields(spec, plan, phase, parent)?;
                    database.promote_capture_to_spec(id, title, body, acceptance_criteria)?
                }
                "task" => {
                    let priority = TaskPriority::parse(&priority)?;
                    let input = CaptureTaskPromotion {
                        spec_id: parse_optional_id(spec, SpecId::parse)?,
                        plan_id: parse_optional_id(plan, PlanId::parse)?,
                        phase_id: parse_optional_id(phase, PhaseId::parse)?,
                        parent_id: parse_optional_id(parent, TaskId::parse)?,
                        title,
                        body,
                        priority,
                        position,
                    };
                    database.promote_capture_to_task(id, input)?
                }
                "note" => {
                    reject_promotion_task_fields(spec, plan, phase, parent)?;
                    database.promote_capture_to_note(id, title, body)?
                }
                _ => {
                    return Err(CommandError::InvalidFilter {
                        message: format!("unknown capture promotion target `{target}`; use spec, task, or note"),
                    });
                }
            };
            Ok(CaptureResult::Promotion(Box::new(result)))
        }
    }
}

fn reject_promotion_task_fields(
    spec: Option<String>, plan: Option<String>, phase: Option<String>, parent: Option<String>,
) -> CResult<()> {
    if spec.is_some() || plan.is_some() || phase.is_some() || parent.is_some() {
        return Err(CommandError::InvalidFilter {
            message: "task placement options apply only when promoting to a task".to_owned(),
        });
    }
    Ok(())
}

fn exec_release(command: ReleaseCommand, renderer: &Renderer) -> anyhow::Result<()> {
    let result: Result<Option<String>, ApplicationError> = (|| match command {
        ReleaseCommand::Create { title, body } => {
            validate_title(&title).map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            let body = resolve_markdown(body)
                .map_err(ApplicationError::from_command)?
                .unwrap_or_default();
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            let release = database
                .create_release(title, body)
                .map_err(ApplicationError::from_command)?;
            Ok(renderer
                .render_connected_release("created", &release)
                .map_err(|error| {
                    ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
                })?)
        }
        ReleaseCommand::Show { id } => {
            let id = parse_release_id(&id).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            let release = database
                .release(id)
                .map_err(ApplicationError::from_command)?
                .ok_or_else(|| {
                    ApplicationError::from_command(CommandError::Storage(StorageError::ReleaseNotFound {
                        id: id.to_string(),
                    }))
                })?;
            Ok(renderer.render_connected_release("shown", &release).map_err(|error| {
                ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
            })?)
        }
        ReleaseCommand::List => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            Ok(renderer
                .render_releases(&database.releases().map_err(ApplicationError::from_command)?)
                .map_err(|error| {
                    ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
                })?)
        }
        ReleaseCommand::Update { id, title, body } => {
            let id = parse_release_id(&id).map_err(ApplicationError::from_command)?;
            if let Some(title) = &title {
                validate_title(title).map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            }
            let body = resolve_markdown(body).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            let release = database
                .update_release(id, title, body)
                .map_err(ApplicationError::from_command)?;
            Ok(renderer
                .render_connected_release("updated", &release)
                .map_err(|error| {
                    ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
                })?)
        }
        ReleaseCommand::Complete { id, allow_open_children } => {
            transition_release(id, ContainerAction::Complete, allow_open_children, renderer)
        }
        ReleaseCommand::Cancel { id, allow_open_children } => {
            transition_release(id, ContainerAction::Cancel, allow_open_children, renderer)
        }
        ReleaseCommand::Member { command } => exec_membership(command, renderer),
    })();
    result.map(write_output).unwrap_or_else(|error| Err(error.into()))
}

fn transition_release(
    id: String, action: ContainerAction, allow_open_children: bool, renderer: &Renderer,
) -> Result<Option<String>, ApplicationError> {
    let id = parse_release_id(&id).map_err(ApplicationError::from_command)?;
    let mut database = open_database().map_err(ApplicationError::from_command)?;
    let release = database
        .transition_release(id, action, allow_open_children)
        .map_err(ApplicationError::from_command)?;
    let action = match action {
        ContainerAction::Complete => "completed",
        ContainerAction::Cancel => "cancelled",
    };
    renderer
        .render_connected_release(action, &release)
        .map_err(|error| ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() }))
}

fn exec_membership(command: MembershipCommand, renderer: &Renderer) -> Result<Option<String>, ApplicationError> {
    match command {
        MembershipCommand::Add { release_id, kind, record_id } => {
            let release_id = parse_release_id(&release_id).map_err(ApplicationError::from_command)?;
            let kind = parse_member_kind(&kind).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            let membership = database
                .add_release_membership(release_id, kind, record_id)
                .map_err(ApplicationError::from_command)?;
            renderer
                .render_membership("added", &membership, &membership.record_id)
                .map_err(|error| {
                    ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
                })
        }
        MembershipCommand::Remove { release_id, kind, record_id } => {
            let release_id = parse_release_id(&release_id).map_err(ApplicationError::from_command)?;
            let kind = parse_member_kind(&kind).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            let membership = database
                .remove_release_membership(release_id, kind, &record_id)
                .map_err(ApplicationError::from_command)?;
            renderer
                .render_membership("removed", &membership, &membership.record_id)
                .map_err(|error| {
                    ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
                })
        }
        MembershipCommand::List { release_id } => {
            let release_id = parse_release_id(&release_id).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            if database
                .release(release_id)
                .map_err(ApplicationError::from_command)?
                .is_none()
            {
                return Err(ApplicationError::from_command(CommandError::Storage(
                    StorageError::ReleaseNotFound { id: release_id.to_string() },
                )));
            }
            let values = database
                .release_memberships()
                .map_err(ApplicationError::from_command)?
                .into_iter()
                .filter(|item| item.release_id == release_id)
                .collect::<Vec<_>>();
            let lines = values
                .iter()
                .map(|item| format!("{}\t{}\t{}", item.record_kind.as_str(), item.release_id, item.record_id))
                .collect::<Vec<_>>();
            renderer
                .render_relationships("memberships", &values, &lines, "No release members.")
                .map_err(|error| {
                    ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
                })
        }
    }
}

fn exec_spec(command: SpecCommand, renderer: &Renderer) -> anyhow::Result<()> {
    let message = match command {
        SpecCommand::Create { title, body, acceptance } => {
            validate_title(&title).map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            let body = resolve_markdown(body)
                .map_err(ApplicationError::from_command)?
                .unwrap_or_default();
            let acceptance = resolve_acceptance(acceptance)
                .map_err(ApplicationError::from_command)?
                .unwrap_or_default();
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            let spec = database
                .create_spec(title, body, acceptance)
                .map_err(ApplicationError::from_command)?;
            renderer.render_spec("created", &spec).map_err(|error| {
                ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
            })?
        }
        SpecCommand::Show { id } => {
            let id = parse_spec_id(&id).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            let spec = database
                .spec(id)
                .map_err(ApplicationError::from_command)?
                .ok_or_else(|| {
                    ApplicationError::from_command(CommandError::Storage(StorageError::SpecNotFound {
                        id: id.to_string(),
                    }))
                })?;
            renderer.render_spec("shown", &spec).map_err(|error| {
                ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
            })?
        }
        SpecCommand::List => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            renderer
                .render_specs(&database.specs().map_err(ApplicationError::from_command)?)
                .map_err(|error| {
                    ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
                })?
        }
        SpecCommand::Update { id, title, body, acceptance } => {
            let id = parse_spec_id(&id).map_err(ApplicationError::from_command)?;
            if let Some(title) = &title {
                validate_title(title).map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            }
            let body = resolve_markdown(body).map_err(ApplicationError::from_command)?;
            let acceptance = resolve_acceptance(acceptance).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            let spec = database
                .update_spec(id, SpecUpdate { title, body, acceptance_criteria: acceptance })
                .map_err(ApplicationError::from_command)?;
            renderer.render_spec("updated", &spec).map_err(|error| {
                ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
            })?
        }
        SpecCommand::Complete { id, allow_open_children } => {
            transition_spec(id, ContainerAction::Complete, allow_open_children, renderer)?
        }
        SpecCommand::Cancel { id, allow_open_children } => {
            transition_spec(id, ContainerAction::Cancel, allow_open_children, renderer)?
        }
    };
    write_output(message)
}

fn transition_spec(
    id: String, action: ContainerAction, allow_open_children: bool, renderer: &Renderer,
) -> Result<Option<String>, ApplicationError> {
    let id = parse_spec_id(&id).map_err(ApplicationError::from_command)?;
    let mut database = open_database().map_err(ApplicationError::from_command)?;
    let spec = database
        .transition_spec(id, action, allow_open_children)
        .map_err(ApplicationError::from_command)?;
    let action = match action {
        ContainerAction::Complete => "completed",
        ContainerAction::Cancel => "cancelled",
    };
    renderer
        .render_spec(action, &spec)
        .map_err(|error| ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() }))
}

fn exec_plan(command: PlanCommand, renderer: &Renderer) -> anyhow::Result<()> {
    let result = match command {
        PlanCommand::Create { title, spec, body, input, no_input: _ } => {
            validate_title(&title).map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            let spec_id = parse_spec_id(&spec).map_err(ApplicationError::from_command)?;
            let body = resolve_markdown(body)
                .map_err(ApplicationError::from_command)?
                .unwrap_or_default();
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            match input {
                Some(path) => {
                    let document = read_plan_document(&path).map_err(ApplicationError::from_command)?;
                    PlanResult::Applied(
                        database
                            .create_and_apply_plan(spec_id, title, body, &document)
                            .map_err(ApplicationError::from_command)?,
                    )
                }
                None => PlanResult::Mutation {
                    action: "created",
                    plan: database
                        .create_plan(spec_id, title, body)
                        .map_err(ApplicationError::from_command)?,
                },
            }
        }
        PlanCommand::Show { id } => {
            let id = parse_plan_id(&id).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            let plan = database
                .plan(id)
                .map_err(ApplicationError::from_command)?
                .ok_or_else(|| {
                    ApplicationError::from_command(CommandError::Storage(StorageError::PlanNotFound {
                        id: id.to_string(),
                    }))
                })?;
            let graph = database.connected_graph().map_err(ApplicationError::from_command)?;
            let tasks = graph
                .tasks
                .iter()
                .map(|task| {
                    task_ancestry(&graph, task.id).map(|(_, plan_id, _)| (plan_id == Some(id)).then(|| task.clone()))
                })
                .collect::<CResult<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
            let task_ids = tasks
                .iter()
                .map(|task| task.id)
                .collect::<std::collections::HashSet<_>>();
            PlanResult::Detail(PlanDetail {
                plan,
                phases: graph.phases.into_iter().filter(|phase| phase.plan_id == id).collect(),
                tasks,
                dependencies: graph
                    .dependencies
                    .into_iter()
                    .filter(|dependency| task_ids.contains(&dependency.task_id))
                    .collect(),
            })
        }
        PlanCommand::List => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            PlanResult::List(database.plans().map_err(ApplicationError::from_command)?)
        }
        PlanCommand::Update { id, title, body } => {
            let id = parse_plan_id(&id).map_err(ApplicationError::from_command)?;
            if let Some(title) = &title {
                validate_title(title).map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            }
            let body = resolve_markdown(body).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            PlanResult::Mutation {
                action: "updated",
                plan: database
                    .update_plan(id, PlanUpdate { title, body })
                    .map_err(ApplicationError::from_command)?,
            }
        }
        PlanCommand::Check { id, file } => {
            let id = parse_plan_id(&id).map_err(ApplicationError::from_command)?;
            let document = read_plan_document(&file).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            PlanResult::Diff(
                database
                    .check_plan(id, &document)
                    .map_err(ApplicationError::from_command)?,
            )
        }
        PlanCommand::Diff { id, file } => {
            let id = parse_plan_id(&id).map_err(ApplicationError::from_command)?;
            let document = read_plan_document(&file).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            PlanResult::Diff(
                database
                    .diff_plan(id, &document)
                    .map_err(ApplicationError::from_command)?,
            )
        }
        PlanCommand::Apply { id, file } => {
            let id = parse_plan_id(&id).map_err(ApplicationError::from_command)?;
            let document = read_plan_document(&file).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            PlanResult::Applied(
                database
                    .apply_plan(id, &document)
                    .map_err(ApplicationError::from_command)?,
            )
        }
        PlanCommand::Complete { id, allow_open_children } => {
            let id = parse_plan_id(&id).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            PlanResult::Mutation {
                action: "completed",
                plan: database
                    .transition_plan(id, ContainerAction::Complete, allow_open_children)
                    .map_err(ApplicationError::from_command)?,
            }
        }
        PlanCommand::Cancel { id, allow_open_children } => {
            let id = parse_plan_id(&id).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            PlanResult::Mutation {
                action: "cancelled",
                plan: database
                    .transition_plan(id, ContainerAction::Cancel, allow_open_children)
                    .map_err(ApplicationError::from_command)?,
            }
        }
        PlanCommand::Phase { command } => return exec_phase(command, renderer),
    };
    let message = match result {
        PlanResult::Mutation { action, plan } => renderer.render_plan(action, &plan),
        PlanResult::Detail(detail) => renderer.render_plan_detail(&detail),
        PlanResult::Diff(diff) => renderer.render_plan_diff(&diff),
        PlanResult::Applied(result) => renderer.render_plan_apply(&result),
        PlanResult::List(plans) => renderer.render_plans(&plans),
    }
    .map_err(|error| ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() }))?;
    write_output(message)
}

fn exec_phase(command: PhaseCommand, renderer: &Renderer) -> anyhow::Result<()> {
    let message = match command {
        PhaseCommand::Create { title, plan, position, body } => {
            validate_title(&title).map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            let plan_id = parse_plan_id(&plan).map_err(ApplicationError::from_command)?;
            let body = resolve_markdown(body)
                .map_err(ApplicationError::from_command)?
                .unwrap_or_default();
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            let phase = database
                .create_phase(plan_id, title, body, position)
                .map_err(ApplicationError::from_command)?;
            render_output(renderer.render_phase("created", &phase))
        }
        PhaseCommand::Show { id } => {
            let id = parse_phase_id(&id).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            let phase = database
                .phase(id)
                .map_err(ApplicationError::from_command)?
                .ok_or_else(|| {
                    ApplicationError::from_command(CommandError::Storage(StorageError::PhaseNotFound {
                        id: id.to_string(),
                    }))
                })?;
            render_output(renderer.render_phase("shown", &phase))
        }
        PhaseCommand::List => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            render_output(renderer.render_phases(&database.phases().map_err(ApplicationError::from_command)?))
        }
        PhaseCommand::Update { id, title, position, body } => {
            let id = parse_phase_id(&id).map_err(ApplicationError::from_command)?;
            if let Some(title) = &title {
                validate_title(title).map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            }
            let body = resolve_markdown(body).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            render_output(
                renderer.render_phase(
                    "updated",
                    &database
                        .update_phase(id, PhaseUpdate { title, body, position })
                        .map_err(ApplicationError::from_command)?,
                ),
            )
        }
        PhaseCommand::Complete { id, allow_open_children } => {
            transition_phase(id, ContainerAction::Complete, allow_open_children, renderer)
        }
        PhaseCommand::Cancel { id, allow_open_children } => {
            transition_phase(id, ContainerAction::Cancel, allow_open_children, renderer)
        }
    }?;
    write_output(message)
}

fn transition_phase(
    id: String, action: ContainerAction, allow_open_children: bool, renderer: &Renderer,
) -> Result<Option<String>, ApplicationError> {
    let id = parse_phase_id(&id).map_err(ApplicationError::from_command)?;
    let mut database = open_database().map_err(ApplicationError::from_command)?;
    let phase = database
        .transition_phase(id, action, allow_open_children)
        .map_err(ApplicationError::from_command)?;
    let action = match action {
        ContainerAction::Complete => "completed",
        ContainerAction::Cancel => "cancelled",
    };
    render_output(renderer.render_phase(action, &phase))
}

fn exec_task(command: TaskCommand, renderer: &Renderer) -> anyhow::Result<()> {
    let message = match command {
        TaskCommand::Create { title, spec, plan, phase, parent, priority, position, blocked_by, body } => {
            validate_title(&title).map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            let priority = TaskPriority::parse(&priority)
                .map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            let project_id = database.project().map_err(ApplicationError::from_command)?.id;
            let input = PlanningTaskCreate {
                project_id,
                spec_id: parse_optional_id(spec, SpecId::parse).map_err(ApplicationError::from_command)?,
                plan_id: parse_optional_id(plan, PlanId::parse).map_err(ApplicationError::from_command)?,
                phase_id: parse_optional_id(phase, PhaseId::parse).map_err(ApplicationError::from_command)?,
                parent_id: parse_optional_id(parent, TaskId::parse).map_err(ApplicationError::from_command)?,
                title,
                body: resolve_markdown(body)
                    .map_err(ApplicationError::from_command)?
                    .unwrap_or_default(),
                priority,
                position,
            };
            let blockers = blocked_by
                .iter()
                .map(|blocker| parse_task_id(blocker))
                .collect::<CResult<Vec<_>>>()
                .map_err(ApplicationError::from_command)?;
            let task = database
                .create_planning_task_with_dependencies(input, &blockers)
                .map_err(ApplicationError::from_command)?;
            render_output(renderer.render_planning_task("created", &task))
        }
        TaskCommand::Show { id } => {
            let id = parse_task_id(&id).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            render_output(
                renderer.render_planning_task_view(
                    &database
                        .planning_task_view(id)
                        .map_err(ApplicationError::from_command)?,
                ),
            )
        }
        TaskCommand::List => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            render_output(
                renderer.render_planning_tasks(&database.planning_tasks().map_err(ApplicationError::from_command)?),
            )
        }
        TaskCommand::Update {
            id,
            title,
            body,
            priority,
            position,
            spec,
            no_spec,
            plan,
            no_plan,
            phase,
            no_phase,
            parent,
            no_parent,
        } => {
            let id = parse_task_id(&id).map_err(ApplicationError::from_command)?;
            if let Some(title) = &title {
                validate_title(title).map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            }
            let priority = priority
                .as_deref()
                .map(TaskPriority::parse)
                .transpose()
                .map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            let update = PlanningTaskUpdate {
                title,
                body: resolve_markdown(body).map_err(ApplicationError::from_command)?,
                priority,
                position,
                spec_id: relation_change(spec, no_spec, SpecId::parse).map_err(ApplicationError::from_command)?,
                plan_id: relation_change(plan, no_plan, PlanId::parse).map_err(ApplicationError::from_command)?,
                phase_id: relation_change(phase, no_phase, PhaseId::parse).map_err(ApplicationError::from_command)?,
                parent_id: relation_change(parent, no_parent, TaskId::parse).map_err(ApplicationError::from_command)?,
            };
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            render_output(
                renderer.render_planning_task(
                    "updated",
                    &database
                        .update_planning_task(id, update)
                        .map_err(ApplicationError::from_command)?,
                ),
            )
        }
        TaskCommand::Start { id } => transition_task(id, TaskAction::Start, false, renderer),
        TaskCommand::Park { id } => transition_task(id, TaskAction::Park, false, renderer),
        TaskCommand::Unpark { id } => transition_task(id, TaskAction::Unpark, false, renderer),
        TaskCommand::Handoff { id, note, note_file } => {
            let note = resolve_optional_value(note, note_file)
                .map_err(ApplicationError::from_command)?
                .ok_or_else(|| {
                    ApplicationError::from_command(CommandError::Domain(DomainError::NoFieldsToUpdate {
                        entity: "handoff",
                    }))
                })?;
            let id = parse_task_id(&id).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            render_output(
                renderer.render_planning_task(
                    "handed_off",
                    &database
                        .handoff_planning_task(id, note)
                        .map_err(ApplicationError::from_command)?,
                ),
            )
        }
        TaskCommand::Complete { id, allow_open_children, evidence, evidence_file } => {
            let evidence = resolve_optional_value(evidence, evidence_file).map_err(ApplicationError::from_command)?;
            let id = parse_task_id(&id).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            render_output(
                renderer.render_planning_task(
                    "completed",
                    &database
                        .complete_planning_task(id, allow_open_children, evidence)
                        .map_err(ApplicationError::from_command)?,
                ),
            )
        }
        TaskCommand::Cancel { id, allow_open_children } => {
            transition_task(id, TaskAction::Cancel, allow_open_children, renderer)
        }
        TaskCommand::Explain { id } => {
            let id = parse_task_id(&id).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            render_output(
                renderer.render_planning_task_view(
                    &database
                        .planning_task_view(id)
                        .map_err(ApplicationError::from_command)?,
                ),
            )
        }
        TaskCommand::Context { id } => {
            let id = parse_task_id(&id).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            render_output(
                renderer
                    .render_planning_context(&database.planning_context(id).map_err(ApplicationError::from_command)?),
            )
        }
    }?;
    write_output(message)
}

fn transition_task(
    id: String, action: TaskAction, allow_open_children: bool, renderer: &Renderer,
) -> Result<Option<String>, ApplicationError> {
    let id = parse_task_id(&id).map_err(ApplicationError::from_command)?;
    let mut database = open_database().map_err(ApplicationError::from_command)?;
    let task = database
        .transition_planning_task(id, action, allow_open_children)
        .map_err(ApplicationError::from_command)?;
    let action = match action {
        TaskAction::Start => "started",
        TaskAction::Park => "parked",
        TaskAction::Unpark => "unparked",
        TaskAction::Complete => "completed",
        TaskAction::Cancel => "cancelled",
    };
    render_output(renderer.render_planning_task(action, &task))
}

fn render_output(result: Result<Option<String>, OutputError>) -> Result<Option<String>, ApplicationError> {
    result.map_err(|error| ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() }))
}

fn exec_dependency(command: DependencyCommand, renderer: &Renderer) -> anyhow::Result<()> {
    let message = match command {
        DependencyCommand::Add { task_id, blocker_id } => {
            let task_id = parse_task_id(&task_id).map_err(ApplicationError::from_command)?;
            let blocker_id = parse_task_id(&blocker_id).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            let dependency = database
                .add_planning_dependency(task_id, blocker_id)
                .map_err(ApplicationError::from_command)?;
            renderer
                .render_dependency(crate::output::DependencyMutation::Added, &dependency)
                .map_err(|error| {
                    ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
                })?
        }
        DependencyCommand::Remove { task_id, blocker_id } => {
            let task_id = parse_task_id(&task_id).map_err(ApplicationError::from_command)?;
            let blocker_id = parse_task_id(&blocker_id).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            let dependency = database
                .remove_planning_dependency(task_id, blocker_id)
                .map_err(ApplicationError::from_command)?;
            renderer
                .render_dependency(crate::output::DependencyMutation::Removed, &dependency)
                .map_err(|error| {
                    ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
                })?
        }
        DependencyCommand::List => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            let dependencies = database
                .planning_dependencies()
                .map_err(ApplicationError::from_command)?;
            let lines = dependencies
                .iter()
                .map(|item| format!("{}\t{}", item.task_id, item.blocker_id))
                .collect::<Vec<_>>();
            renderer
                .render_relationships("dependencies", &dependencies, &lines, "No dependencies.")
                .map_err(|error| {
                    ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
                })?
        }
    };
    write_output(message)
}

fn exec_note(command: NoteCommand, renderer: &Renderer) -> anyhow::Result<()> {
    let message = match command {
        NoteCommand::Create { title, body } => {
            validate_title(&title).map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            let body = resolve_markdown(body)
                .map_err(ApplicationError::from_command)?
                .unwrap_or_default();
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            renderer.render_note(
                "created",
                &database
                    .create_note(title, body)
                    .map_err(ApplicationError::from_command)?,
            )
        }
        NoteCommand::Show { id } => {
            let id = parse_note_id(&id).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            let note = database
                .note(id)
                .map_err(ApplicationError::from_command)?
                .ok_or_else(|| {
                    ApplicationError::from_command(CommandError::Storage(StorageError::NoteNotFound {
                        id: id.to_string(),
                    }))
                })?;
            renderer.render_note("shown", &note)
        }
        NoteCommand::List => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            renderer.render_notes(&database.notes().map_err(ApplicationError::from_command)?)
        }
        NoteCommand::Update { id, title, body } => {
            let id = parse_note_id(&id).map_err(ApplicationError::from_command)?;
            if let Some(title) = &title {
                validate_title(title).map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            }
            let body = resolve_markdown(body).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            renderer.render_note(
                "updated",
                &database
                    .update_note(id, NoteUpdate { title, body })
                    .map_err(ApplicationError::from_command)?,
            )
        }
        NoteCommand::Link { command } => return exec_note_link(command, renderer),
    }
    .map_err(|error| ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() }))?;
    write_output(message)
}

fn exec_note_link(command: NoteLinkCommand, renderer: &Renderer) -> anyhow::Result<()> {
    let message = match command {
        NoteLinkCommand::Add { note_id, kind, record_id } => {
            let note_id = parse_note_id(&note_id).map_err(ApplicationError::from_command)?;
            let kind = parse_linked_kind(&kind).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            let link = database
                .add_note_link(note_id, kind, record_id)
                .map_err(ApplicationError::from_command)?;
            renderer
                .render_note_link("added", &link, &link.record_id)
                .map_err(|error| {
                    ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
                })?
        }
        NoteLinkCommand::Remove { note_id, kind, record_id } => {
            let note_id = parse_note_id(&note_id).map_err(ApplicationError::from_command)?;
            let kind = parse_linked_kind(&kind).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            let link = database
                .remove_note_link(note_id, kind, &record_id)
                .map_err(ApplicationError::from_command)?;
            renderer
                .render_note_link("removed", &link, &link.record_id)
                .map_err(|error| {
                    ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
                })?
        }
        NoteLinkCommand::List { note_id } => {
            let note_id = parse_note_id(&note_id).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            if database
                .note(note_id)
                .map_err(ApplicationError::from_command)?
                .is_none()
            {
                return Err(
                    ApplicationError::from_command(CommandError::Storage(StorageError::NoteNotFound {
                        id: note_id.to_string(),
                    }))
                    .into(),
                );
            }
            let links = database
                .note_links()
                .map_err(ApplicationError::from_command)?
                .into_iter()
                .filter(|link| link.note_id == note_id)
                .collect::<Vec<_>>();
            let lines = links
                .iter()
                .map(|link| format!("{}\t{}", link.record_kind.as_str(), link.record_id))
                .collect::<Vec<_>>();
            renderer
                .render_relationships("links", &links, &lines, "No note links.")
                .map_err(|error| {
                    ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
                })?
        }
    };
    write_output(message)
}

fn show_record(database: &Database, id: &str, renderer: &Renderer) -> CResult<Option<String>> {
    if id.starts_with(CaptureId::PREFIX) {
        let value = parse_capture_id(id)?;
        let record = database
            .capture(value)?
            .ok_or_else(|| StorageError::CaptureNotFound { id: id.to_owned() })?;
        return renderer
            .render_capture("shown", &record)
            .map_err(|error| CommandError::InvalidFilter { message: error.to_string() });
    }
    if id.starts_with(SpecId::PREFIX) {
        let value = parse_spec_id(id)?;
        let record = database
            .spec(value)?
            .ok_or_else(|| StorageError::SpecNotFound { id: id.to_owned() })?;
        return renderer
            .render_spec("shown", &record)
            .map_err(|error| CommandError::InvalidFilter { message: error.to_string() });
    }
    if id.starts_with(PlanId::PREFIX) {
        let value = parse_plan_id(id)?;
        let record = database
            .plan(value)?
            .ok_or_else(|| StorageError::PlanNotFound { id: id.to_owned() })?;
        return renderer
            .render_plan("shown", &record)
            .map_err(|error| CommandError::InvalidFilter { message: error.to_string() });
    }
    if id.starts_with(PhaseId::PREFIX) {
        let value = parse_phase_id(id)?;
        let record = database
            .phase(value)?
            .ok_or_else(|| StorageError::PhaseNotFound { id: id.to_owned() })?;
        return renderer
            .render_phase("shown", &record)
            .map_err(|error| CommandError::InvalidFilter { message: error.to_string() });
    }
    if id.starts_with(TaskId::PREFIX) {
        let value = parse_task_id(id)?;
        let record = database.planning_task_view(value)?;
        return renderer
            .render_planning_task_view(&record)
            .map_err(|error| CommandError::InvalidFilter { message: error.to_string() });
    }
    if id.starts_with(NoteId::PREFIX) {
        let value = parse_note_id(id)?;
        let record = database
            .note(value)?
            .ok_or_else(|| StorageError::NoteNotFound { id: id.to_owned() })?;
        return renderer
            .render_note("shown", &record)
            .map_err(|error| CommandError::InvalidFilter { message: error.to_string() });
    }
    if id.starts_with(ReleaseId::PREFIX) {
        let value = parse_release_id(id)?;
        let record = database
            .release(value)?
            .ok_or_else(|| StorageError::ReleaseNotFound { id: id.to_owned() })?;
        return renderer
            .render_connected_release("shown", &record)
            .map_err(|error| CommandError::InvalidFilter { message: error.to_string() });
    }
    Err(CommandError::InvalidFilter { message: format!("unknown record ID `{id}`") })
}

fn connected_tree(graph: &ConnectedGraph, root: Option<&str>) -> CResult<Vec<ConnectedTreeNode>> {
    let mut nodes = Vec::new();
    for release in &graph.releases {
        nodes.push(ConnectedTreeNode {
            kind: "release",
            id: release.id.to_string(),
            title: release.title.clone(),
            status: release.status.as_str().to_owned(),
            children: Vec::new(),
        });
    }
    for capture in &graph.captures {
        nodes.push(ConnectedTreeNode {
            kind: "capture",
            id: capture.id.to_string(),
            title: capture.title.clone(),
            status: capture.status.as_str().to_owned(),
            children: Vec::new(),
        });
    }
    for spec in &graph.specs {
        let mut children = graph
            .plans
            .iter()
            .filter(|plan| plan.spec_id == spec.id)
            .map(|plan| plan_tree_node(graph, plan))
            .collect::<CResult<Vec<_>>>()?;
        children.extend(
            graph
                .tasks
                .iter()
                .filter(|task| {
                    task.spec_id == Some(spec.id)
                        && task.plan_id.is_none()
                        && task.phase_id.is_none()
                        && task.parent_id.is_none()
                })
                .map(|task| task_tree_node(graph, task, &mut std::collections::HashSet::new()))
                .collect::<CResult<Vec<_>>>()?,
        );
        nodes.push(ConnectedTreeNode {
            kind: "spec",
            id: spec.id.to_string(),
            title: spec.title.clone(),
            status: spec.status.as_str().to_owned(),
            children,
        });
    }
    nodes.extend(
        graph
            .tasks
            .iter()
            .filter(|task| {
                task.spec_id.is_none() && task.plan_id.is_none() && task.phase_id.is_none() && task.parent_id.is_none()
            })
            .map(|task| task_tree_node(graph, task, &mut std::collections::HashSet::new()))
            .collect::<CResult<Vec<_>>>()?,
    );
    for note in &graph.notes {
        nodes.push(ConnectedTreeNode {
            kind: "note",
            id: note.id.to_string(),
            title: note.title.clone(),
            status: "available".to_owned(),
            children: Vec::new(),
        });
    }
    nodes.sort_by(|left, right| left.kind.cmp(right.kind).then_with(|| left.id.cmp(&right.id)));
    let Some(root) = root else { return Ok(nodes) };
    if let Some(node) = find_tree_node(&nodes, root) {
        return Ok(vec![node.clone()]);
    }
    Err(tree_root_error(root))
}

fn plan_tree_node(graph: &ConnectedGraph, plan: &Plan) -> CResult<ConnectedTreeNode> {
    let mut children = graph
        .phases
        .iter()
        .filter(|phase| phase.plan_id == plan.id)
        .map(|phase| {
            let tasks = graph
                .tasks
                .iter()
                .filter(|task| task.phase_id == Some(phase.id) && task.parent_id.is_none())
                .map(|task| task_tree_node(graph, task, &mut std::collections::HashSet::new()))
                .collect::<CResult<Vec<_>>>()?;
            Ok(ConnectedTreeNode {
                kind: "phase",
                id: phase.id.to_string(),
                title: phase.title.clone(),
                status: phase.status.as_str().to_owned(),
                children: tasks,
            })
        })
        .collect::<CResult<Vec<_>>>()?;
    children.extend(
        graph
            .tasks
            .iter()
            .filter(|task| task.plan_id == Some(plan.id) && task.phase_id.is_none() && task.parent_id.is_none())
            .map(|task| task_tree_node(graph, task, &mut std::collections::HashSet::new()))
            .collect::<CResult<Vec<_>>>()?,
    );
    children.sort_by(|left, right| left.kind.cmp(right.kind).then_with(|| left.id.cmp(&right.id)));
    Ok(ConnectedTreeNode {
        kind: "plan",
        id: plan.id.to_string(),
        title: plan.title.clone(),
        status: plan.status.as_str().to_owned(),
        children,
    })
}

fn task_tree_node(
    graph: &ConnectedGraph, task: &PlanningTask, path: &mut std::collections::HashSet<TaskId>,
) -> CResult<ConnectedTreeNode> {
    if !path.insert(task.id) {
        return Err(CommandError::Storage(StorageError::InvalidPlanningTask(
            DomainError::ParentCycle { task: task.id.to_string(), parent: task.id.to_string() },
        )));
    }
    let mut children = graph
        .tasks
        .iter()
        .filter(|child| child.parent_id == Some(task.id))
        .map(|child| task_tree_node(graph, child, path))
        .collect::<CResult<Vec<_>>>()?;
    path.remove(&task.id);
    children.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(ConnectedTreeNode {
        kind: "task",
        id: task.id.to_string(),
        title: task.title.clone(),
        status: task.status.as_str().to_owned(),
        children,
    })
}

fn find_tree_node<'a>(nodes: &'a [ConnectedTreeNode], id: &str) -> Option<&'a ConnectedTreeNode> {
    nodes.iter().find_map(
        |node| {
            if node.id == id { Some(node) } else { find_tree_node(&node.children, id) }
        },
    )
}

fn tree_root_error(id: &str) -> CommandError {
    if id.starts_with(CaptureId::PREFIX) {
        return CommandError::Storage(StorageError::CaptureNotFound { id: id.to_owned() });
    }
    if id.starts_with(SpecId::PREFIX) {
        return CommandError::Storage(StorageError::SpecNotFound { id: id.to_owned() });
    }
    if id.starts_with(PlanId::PREFIX) {
        return CommandError::Storage(StorageError::PlanNotFound { id: id.to_owned() });
    }
    if id.starts_with(PhaseId::PREFIX) {
        return CommandError::Storage(StorageError::PhaseNotFound { id: id.to_owned() });
    }
    if id.starts_with(TaskId::PREFIX) {
        return CommandError::Storage(StorageError::PlanningTaskNotFound { id: id.to_owned() });
    }
    if id.starts_with(NoteId::PREFIX) {
        return CommandError::Storage(StorageError::NoteNotFound { id: id.to_owned() });
    }
    if id.starts_with(ReleaseId::PREFIX) {
        return CommandError::Storage(StorageError::ReleaseNotFound { id: id.to_owned() });
    }
    CommandError::InvalidFilter { message: format!("unknown record ID `{id}`") }
}

fn list_connected(database: &Database, args: ListArgs) -> CResult<Vec<ConnectedSummary>> {
    let graph = database.connected_graph()?;
    let filter = resolve_list_filter(args)?;
    validate_list_targets(&graph, &filter)?;
    let mut records = Vec::new();
    for capture in &graph.captures {
        if filter.kind.as_deref().is_none_or(|kind| kind == "capture")
            && filter.release_id.is_none()
            && filter.spec_id.is_none()
            && filter.plan_id.is_none()
            && filter.phase_id.is_none()
            && filter.parent_id.is_none()
            && filter.priorities.is_empty()
            && status_matches(&filter.statuses, capture.status.as_str())
        {
            records.push(ConnectedSummary::new(
                "capture",
                capture.id.to_string(),
                capture.title.clone(),
                capture.status.as_str(),
            ));
        }
    }
    for release in &graph.releases {
        if filter.kind.as_deref().is_none_or(|kind| kind == "release")
            && filter.spec_id.is_none()
            && filter.plan_id.is_none()
            && filter.phase_id.is_none()
            && filter.parent_id.is_none()
            && filter.priorities.is_empty()
            && status_matches(&filter.statuses, release.status.as_str())
            && filter.release_id.is_none_or(|id| id == release.id)
        {
            records.push(ConnectedSummary::new(
                "release",
                release.id.to_string(),
                release.title.clone(),
                release.status.as_str(),
            ));
        }
    }
    for spec in &graph.specs {
        if filter.kind.as_deref().is_none_or(|kind| kind == "spec")
            && status_matches(&filter.statuses, spec.status.as_str())
            && filter.spec_id.is_none_or(|id| id == spec.id)
            && filter.plan_id.is_none()
            && filter.phase_id.is_none()
            && filter.parent_id.is_none()
            && filter.priorities.is_empty()
            && release_member_matches(&graph, "spec", &spec.id.to_string(), filter.release_id)
        {
            records.push(ConnectedSummary::new(
                "spec",
                spec.id.to_string(),
                spec.title.clone(),
                spec.status.as_str(),
            ));
        }
    }
    for plan in &graph.plans {
        if filter.kind.as_deref().is_none_or(|kind| kind == "plan")
            && status_matches(&filter.statuses, plan.status.as_str())
            && filter.plan_id.is_none_or(|id| id == plan.id)
            && filter.spec_id.is_none_or(|id| id == plan.spec_id)
            && filter.phase_id.is_none()
            && filter.parent_id.is_none()
            && filter.priorities.is_empty()
            && release_member_matches(&graph, "plan", &plan.id.to_string(), filter.release_id)
        {
            records.push(ConnectedSummary::new(
                "plan",
                plan.id.to_string(),
                plan.title.clone(),
                plan.status.as_str(),
            ));
        }
    }
    for phase in &graph.phases {
        if filter.kind.as_deref().is_none_or(|kind| kind == "phase")
            && status_matches(&filter.statuses, phase.status.as_str())
            && filter.phase_id.is_none_or(|id| id == phase.id)
            && filter.plan_id.is_none_or(|id| id == phase.plan_id)
            && filter.release_id.is_none()
            && filter.spec_id.is_none()
            && filter.parent_id.is_none()
            && filter.priorities.is_empty()
        {
            records.push(ConnectedSummary::new(
                "phase",
                phase.id.to_string(),
                phase.title.clone(),
                phase.status.as_str(),
            ));
        }
    }
    for task in &graph.tasks {
        let ancestry = task_ancestry(&graph, task.id)?;
        let task_matches = filter.kind.as_deref().is_none_or(|kind| kind == "task")
            && status_matches(&filter.statuses, task.status.as_str())
            && (filter.priorities.is_empty() || filter.priorities.contains(&task.priority))
            && filter.spec_id.is_none_or(|id| ancestry.0 == Some(id))
            && filter.plan_id.is_none_or(|id| ancestry.1 == Some(id))
            && filter.phase_id.is_none_or(|id| ancestry.2 == Some(id))
            && filter.parent_id.is_none_or(|id| task.parent_id == Some(id))
            && release_member_matches(&graph, "task", &task.id.to_string(), filter.release_id);
        if task_matches {
            records.push(ConnectedSummary::new(
                "task",
                task.id.to_string(),
                task.title.clone(),
                task.status.as_str(),
            ));
        }
    }
    for note in &graph.notes {
        if filter.kind.as_deref().is_none_or(|kind| kind == "note")
            && filter.spec_id.is_none()
            && filter.plan_id.is_none()
            && filter.phase_id.is_none()
            && filter.parent_id.is_none()
            && filter.priorities.is_empty()
            && filter
                .release_id
                .is_none_or(|id| release_member_matches(&graph, "note", &note.id.to_string(), Some(id)))
        {
            records.push(ConnectedSummary::new(
                "note",
                note.id.to_string(),
                note.title.clone(),
                "available",
            ));
        }
    }
    records.sort_by(|left, right| left.kind.cmp(right.kind).then_with(|| left.id.cmp(&right.id)));
    Ok(records)
}

fn release_member_matches(graph: &ConnectedGraph, kind: &str, id: &str, release: Option<ReleaseId>) -> bool {
    release.is_none_or(|release| {
        graph
            .release_memberships
            .iter()
            .any(|member| member.release_id == release && member.record_kind.as_str() == kind && member.record_id == id)
    })
}

fn task_ancestry(graph: &ConnectedGraph, id: TaskId) -> CResult<(Option<SpecId>, Option<PlanId>, Option<PhaseId>)> {
    task_ancestry_with_seen(graph, id, &mut std::collections::HashSet::new())
}

fn task_ancestry_with_seen(
    graph: &ConnectedGraph, id: TaskId, seen: &mut std::collections::HashSet<TaskId>,
) -> CResult<(Option<SpecId>, Option<PlanId>, Option<PhaseId>)> {
    if !seen.insert(id) {
        return Err(CommandError::Storage(StorageError::InvalidPlanningTask(
            DomainError::ParentCycle { task: id.to_string(), parent: id.to_string() },
        )));
    }
    let task = graph
        .tasks
        .iter()
        .find(|task| task.id == id)
        .ok_or_else(|| StorageError::PlanningTaskNotFound { id: id.to_string() })?;
    let mut ancestry = (task.spec_id, task.plan_id, task.phase_id);
    if let Some(phase_id) = ancestry.2 {
        let phase = graph
            .phases
            .iter()
            .find(|phase| phase.id == phase_id)
            .ok_or_else(|| StorageError::PhaseNotFound { id: phase_id.to_string() })?;
        ancestry.1.get_or_insert(phase.plan_id);
    }
    if let Some(plan_id) = ancestry.1 {
        let plan = graph
            .plans
            .iter()
            .find(|plan| plan.id == plan_id)
            .ok_or_else(|| StorageError::PlanNotFound { id: plan_id.to_string() })?;
        ancestry.0.get_or_insert(plan.spec_id);
    }
    if let Some(parent_id) = task.parent_id {
        let parent = task_ancestry_with_seen(graph, parent_id, seen)?;
        if ancestry.0.is_none() {
            ancestry.0 = parent.0;
        }
        if ancestry.1.is_none() {
            ancestry.1 = parent.1;
        }
        if ancestry.2.is_none() {
            ancestry.2 = parent.2;
        }
    }
    seen.remove(&id);
    Ok(ancestry)
}

fn resolve_list_filter(args: ListArgs) -> CResult<ListFilter> {
    let known = ["capture", "release", "spec", "plan", "phase", "task", "note"];
    if let Some(kind) = &args.kind
        && !known.contains(&kind.as_str())
    {
        return Err(CommandError::InvalidFilter {
            message: format!("unknown kind `{kind}`; use capture, release, spec, plan, phase, task, or note"),
        });
    }
    let priorities = args
        .priority
        .iter()
        .map(|value| TaskPriority::parse(value))
        .collect::<Result<Vec<_>, _>>()?;
    if !priorities.is_empty() && args.kind.as_deref().is_some_and(|kind| kind != "task") {
        return Err(CommandError::InvalidFilter { message: "priority filters apply only to tasks".to_owned() });
    }
    let known_statuses = [
        "captured",
        "promoted",
        "discarded",
        "open",
        "completed",
        "cancelled",
        "pending",
        "in_progress",
        "parked",
    ];
    if let Some(status) = args
        .status
        .iter()
        .find(|status| !known_statuses.contains(&status.as_str()))
    {
        return Err(CommandError::InvalidFilter { message: format!("unknown status `{status}`") });
    }
    if let Some(kind) = args.kind.as_deref() {
        let allowed = match kind {
            "capture" => &["captured", "promoted", "discarded"][..],
            "release" | "spec" | "plan" | "phase" => &["open", "completed", "cancelled"][..],
            "task" => &["pending", "in_progress", "parked", "completed", "cancelled"][..],
            "note" => &[][..],
            _ => &[][..],
        };
        if let Some(status) = args.status.iter().find(|status| !allowed.contains(&status.as_str())) {
            return Err(CommandError::InvalidFilter {
                message: format!("status `{status}` does not apply to {kind} records"),
            });
        }
    }
    let release_id = parse_optional_id(args.release, ReleaseId::parse)?;
    let spec_id = parse_optional_id(args.spec, SpecId::parse)?;
    let plan_id = parse_optional_id(args.plan, PlanId::parse)?;
    let phase_id = parse_optional_id(args.phase, PhaseId::parse)?;
    let parent_id = parse_optional_id(args.parent, TaskId::parse)?;
    Ok(ListFilter {
        kind: args.kind,
        statuses: args.status,
        priorities,
        release_id,
        epic_id: None,
        milestone_id: None,
        parent_id,
        spec_id,
        plan_id,
        phase_id,
    })
}

fn validate_list_targets(graph: &ConnectedGraph, filter: &ListFilter) -> CResult<()> {
    if let Some(id) = filter.release_id
        && !graph.releases.iter().any(|release| release.id == id)
    {
        return Err(CommandError::Storage(StorageError::ReleaseNotFound {
            id: id.to_string(),
        }));
    }
    if let Some(id) = filter.spec_id
        && !graph.specs.iter().any(|spec| spec.id == id)
    {
        return Err(CommandError::Storage(StorageError::SpecNotFound { id: id.to_string() }));
    }
    if let Some(id) = filter.plan_id
        && !graph.plans.iter().any(|plan| plan.id == id)
    {
        return Err(CommandError::Storage(StorageError::PlanNotFound { id: id.to_string() }));
    }
    if let Some(id) = filter.phase_id
        && !graph.phases.iter().any(|phase| phase.id == id)
    {
        return Err(CommandError::Storage(StorageError::PhaseNotFound {
            id: id.to_string(),
        }));
    }
    if let Some(id) = filter.parent_id
        && !graph.tasks.iter().any(|task| task.id == id)
    {
        return Err(CommandError::Storage(StorageError::PlanningTaskNotFound {
            id: id.to_string(),
        }));
    }
    Ok(())
}

fn resolve_ready_filter(args: ReadyArgs) -> CResult<PlanningReadyFilter> {
    Ok(PlanningReadyFilter {
        priorities: args
            .priority
            .iter()
            .map(|value| TaskPriority::parse(value))
            .collect::<Result<Vec<_>, _>>()?,
        spec_id: parse_optional_id(args.spec, SpecId::parse)?,
        plan_id: parse_optional_id(args.plan, PlanId::parse)?,
        phase_id: parse_optional_id(args.phase, PhaseId::parse)?,
        parent_id: parse_optional_id(args.parent, TaskId::parse)?,
    })
}

fn status_matches(statuses: &[String], status: &str) -> bool {
    statuses.is_empty() || statuses.iter().any(|candidate| candidate == status)
}

fn parse_member_kind(value: &str) -> CResult<ReleaseMemberKind> {
    match value {
        "spec" => Ok(ReleaseMemberKind::Spec),
        "plan" => Ok(ReleaseMemberKind::Plan),
        "task" => Ok(ReleaseMemberKind::Task),
        "note" => Ok(ReleaseMemberKind::Note),
        _ => Err(CommandError::InvalidFilter { message: format!("unknown release member kind `{value}`") }),
    }
}

fn parse_linked_kind(value: &str) -> CResult<LinkedRecordKind> {
    match value {
        "capture" => Ok(LinkedRecordKind::Capture),
        "spec" => Ok(LinkedRecordKind::Spec),
        "plan" => Ok(LinkedRecordKind::Plan),
        "phase" => Ok(LinkedRecordKind::Phase),
        "task" => Ok(LinkedRecordKind::Task),
        "note" => Ok(LinkedRecordKind::Note),
        "release" => Ok(LinkedRecordKind::Release),
        _ => Err(CommandError::InvalidFilter { message: format!("unknown linked record kind `{value}`") }),
    }
}

fn parse_id<T>(value: &str, parser: impl Fn(&str) -> Result<T, IdError>) -> CResult<T> {
    parser(value).map_err(|error| CommandError::Domain(DomainError::InvalidId(error)))
}

fn parse_optional_id<T>(value: Option<String>, parser: impl Fn(&str) -> Result<T, IdError>) -> CResult<Option<T>> {
    value.as_deref().map(|value| parse_id(value, &parser)).transpose()
}

fn relation_change<T>(
    value: Option<String>, clear: bool, parser: impl Fn(&str) -> Result<T, IdError>,
) -> CResult<Option<Option<T>>> {
    if clear { Ok(Some(None)) } else { parse_optional_id(value, parser).map(|value| value.map(Some)) }
}

fn parse_capture_id(value: &str) -> CResult<CaptureId> {
    parse_id(value, CaptureId::parse)
}
fn parse_release_id(value: &str) -> CResult<ReleaseId> {
    parse_id(value, ReleaseId::parse)
}
fn parse_spec_id(value: &str) -> CResult<SpecId> {
    parse_id(value, SpecId::parse)
}
fn parse_plan_id(value: &str) -> CResult<PlanId> {
    parse_id(value, PlanId::parse)
}
fn parse_phase_id(value: &str) -> CResult<PhaseId> {
    parse_id(value, PhaseId::parse)
}
fn parse_task_id(value: &str) -> CResult<TaskId> {
    parse_id(value, TaskId::parse)
}
fn parse_note_id(value: &str) -> CResult<NoteId> {
    parse_id(value, NoteId::parse)
}

fn resolve_markdown(args: MarkdownArgs) -> CResult<Option<String>> {
    resolve_optional_value(args.body, args.body_file)
}

fn resolve_acceptance(args: AcceptanceArgs) -> CResult<Option<String>> {
    resolve_optional_value(args.acceptance_criteria, args.acceptance_criteria_file)
}

fn resolve_optional_value(value: Option<String>, path: Option<PathBuf>) -> CResult<Option<String>> {
    if let Some(value) = value {
        return Ok(Some(value));
    }
    let Some(path) = path else { return Ok(None) };
    if path == Path::new("-") {
        return read_stdin();
    }
    let bytes = fs::read(&path).map_err(|source| CommandError::ReadMarkdown { path: path.clone(), source })?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| CommandError::InvalidMarkdown { path })
}

fn read_stdin() -> CResult<Option<String>> {
    let stdin = io::stdin();
    if stdin.is_terminal() {
        return Err(CommandError::StdinIsTerminal);
    }
    let mut bytes = Vec::new();
    stdin
        .lock()
        .read_to_end(&mut bytes)
        .map_err(|source| CommandError::ReadStdin { source })?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| CommandError::InvalidStdin)
}

fn read_plan_document(path: &Path) -> CResult<crate::plan::PlanDocument> {
    let input = resolve_optional_value(None, Some(path.to_owned()))?.unwrap_or_default();
    crate::plan::parse(&input).map_err(|error| CommandError::Storage(StorageError::InvalidPlanInput(error)))
}

fn initialize(start: &Path, snapshot: bool) -> IResult<Initialization> {
    let root = initialization_root(start)?;
    let arcl_directory = root.join(ARCL_DIRECTORY);
    fs::create_dir_all(&arcl_directory)
        .map_err(|source| InitError::CreateDirectory { path: arcl_directory.clone(), source })?;
    let config_path = arcl_directory.join(CONFIG_FILE);
    let config = load_or_create_config(&config_path, snapshot)?;
    ensure_gitignore(&arcl_directory.join(GITIGNORE_FILE))?;
    let database_path = arcl_directory.join(DATABASE_FILE);
    Database::open(&database_path).map_err(|source| InitError::OpenDatabase { path: database_path, source })?;
    if config.snapshot.enabled {
        ensure_snapshot_layout(&root, &config)?;
    }
    Ok(Initialization { root, snapshot_enabled: config.snapshot.enabled })
}

fn initialization_root(start: &Path) -> IResult<PathBuf> {
    if let Some(root) = nearest_project_root(start) {
        return Ok(root);
    }
    match GixVcs::discover(start) {
        Ok(vcs) => Ok(vcs.worktree_root()?.to_owned()),
        Err(VcsError::Discovery { .. }) => Ok(start.to_owned()),
        Err(error) => Err(InitError::Vcs(error)),
    }
}

fn load_or_create_config(path: &Path, snapshot: bool) -> IResult<ProjectConfig> {
    match fs::read_to_string(path) {
        Ok(input) => {
            let mut config = ProjectConfig::parse(&input)
                .map_err(|source| InitError::InvalidConfig { path: path.to_owned(), source })?;
            if snapshot && !config.snapshot.enabled {
                config.snapshot.enabled = true;
                let rendered = config
                    .render()
                    .map_err(|source| InitError::RenderConfig { path: path.to_owned(), source })?;
                fs::write(path, rendered).map_err(|source| InitError::WriteConfig { path: path.to_owned(), source })?;
            }
            Ok(config)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let mut config = ProjectConfig::default();
            config.snapshot.enabled = snapshot;
            let rendered = config
                .render()
                .map_err(|source| InitError::RenderConfig { path: path.to_owned(), source })?;
            create_file(path, rendered.as_bytes())
                .map_err(|source| InitError::WriteConfig { path: path.to_owned(), source })?;
            Ok(config)
        }
        Err(source) => Err(InitError::ReadConfig { path: path.to_owned(), source }),
    }
}

fn ensure_snapshot_layout(root: &Path, config: &ProjectConfig) -> IResult<()> {
    let snapshot_root = resolve_snapshot_root(root, &config.snapshot.path)
        .map_err(|source| InitError::InvalidConfig { path: root.join(ARCL_DIRECTORY).join(CONFIG_FILE), source })?;
    fs::create_dir_all(&snapshot_root)
        .map_err(|source| InitError::CreateSnapshotDirectory { path: snapshot_root.clone(), source })?;
    for directory in SNAPSHOT_DIRECTORIES {
        let path = snapshot_root.join(directory);
        fs::create_dir_all(&path).map_err(|source| InitError::CreateSnapshotDirectory { path, source })?;
    }
    let manifest_path = snapshot_root.join("manifest.toml");
    if !manifest_path.is_file() {
        let manifest = encode_manifest(&SnapshotManifest::default())
            .map_err(|source| InitError::RenderSnapshotManifest { path: manifest_path.clone(), source })?;
        create_file(&manifest_path, manifest.as_bytes())
            .map_err(|source| InitError::WriteSnapshotManifest { path: manifest_path, source })?;
    }
    Ok(())
}

fn ensure_gitignore(path: &Path) -> IResult<()> {
    let existing = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let content = required_gitignore();
            create_file(path, content.as_bytes())
                .map_err(|source| InitError::WriteGitignore { path: path.to_owned(), source })?;
            return Ok(());
        }
        Err(source) => return Err(InitError::ReadGitignore { path: path.to_owned(), source }),
    };
    let mut updated = existing.clone();
    for entry in REQUIRED_GITIGNORE_ENTRIES {
        if updated.lines().any(|line| line.trim() == *entry) {
            continue;
        }
        if !updated.is_empty() && !updated.ends_with('\n') {
            updated.push('\n');
        }
        updated.push_str(entry);
        updated.push('\n');
    }
    if updated != existing {
        fs::write(path, updated).map_err(|source| InitError::WriteGitignore { path: path.to_owned(), source })?;
    }
    Ok(())
}

fn required_gitignore() -> String {
    let mut content = REQUIRED_GITIGNORE_ENTRIES.join("\n");
    content.push('\n');
    content
}

fn create_file(path: &Path, content: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(content)?;
    file.flush()
}

fn open_database() -> CResult<Database> {
    let start = std::env::current_dir().map_err(|source| CommandError::CurrentDirectory { source })?;
    let Some(root) = nearest_project_root(&start) else {
        return Err(CommandError::NotInitialized { root: start });
    };
    let arcl_directory = root.join(ARCL_DIRECTORY);
    let config_path = arcl_directory.join(CONFIG_FILE);
    let database_path = arcl_directory.join(DATABASE_FILE);
    let input = fs::read_to_string(&config_path)
        .map_err(|source| CommandError::ReadConfig { path: config_path.clone(), source })?;
    ProjectConfig::parse(&input).map_err(|source| CommandError::InvalidConfig { path: config_path, source })?;
    if !database_path.is_file() {
        return Err(CommandError::NotInitialized { root });
    }
    Database::open(&database_path).map_err(|source| CommandError::OpenDatabase { path: database_path, source })
}

fn locate_project() -> CResult<ProjectLocation> {
    let start = std::env::current_dir().map_err(|source| CommandError::CurrentDirectory { source })?;
    let Some(root) = nearest_project_root(&start) else {
        return Err(CommandError::NotInitialized { root: start });
    };
    let arcl_directory = root.join(ARCL_DIRECTORY);
    let config_path = arcl_directory.join(CONFIG_FILE);
    let database_path = arcl_directory.join(DATABASE_FILE);
    let input = fs::read_to_string(&config_path)
        .map_err(|source| CommandError::ReadConfig { path: config_path.clone(), source })?;
    let config =
        ProjectConfig::parse(&input).map_err(|source| CommandError::InvalidConfig { path: config_path, source })?;
    Ok(ProjectLocation { root, config, database_path })
}

fn nearest_project_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|candidate| candidate.join(ARCL_DIRECTORY).join(CONFIG_FILE).is_file())
        .map(Path::to_owned)
}

fn open_project() -> CResult<OpenProject> {
    let location = locate_project()?;
    let database = Database::open(&location.database_path)
        .map_err(|source| CommandError::OpenDatabase { path: location.database_path.clone(), source })?;
    Ok(OpenProject { root: location.root, config: location.config, database })
}

fn exec_snapshot(command: SnapshotCommand) -> CResult<()> {
    match command {
        SnapshotCommand::Export => {
            let mut project = open_project()?;
            if !project.config.snapshot.enabled {
                return Err(CommandError::SnapshotDisabled { root: project.root });
            }
            let config_path = project.root.join(ARCL_DIRECTORY).join(CONFIG_FILE);
            let snapshot_root = resolve_snapshot_root(&project.root, &project.config.snapshot.path)
                .map_err(|source| CommandError::InvalidConfig { path: config_path, source })?;
            let graph = project.database.graph()?;
            let files = export_graph(&snapshot_root, &graph)?;
            let base = files
                .iter()
                .map(|file| SnapshotBaseFile {
                    path: file.path.to_string_lossy().into_owned(),
                    content: file.content.clone(),
                })
                .collect::<Vec<_>>();
            if project.database.snapshot_base()? != base {
                project.database.replace_snapshot_base(&base)?;
            }
            Ok(())
        }
        SnapshotCommand::Import => {
            let project = locate_project()?;
            if !project.config.snapshot.enabled {
                return Err(CommandError::SnapshotDisabled { root: project.root });
            }
            let config_path = project.root.join(ARCL_DIRECTORY).join(CONFIG_FILE);
            let snapshot_root = resolve_snapshot_root(&project.root, &project.config.snapshot.path)
                .map_err(|source| CommandError::InvalidConfig { path: config_path, source })?;
            import_snapshot(&snapshot_root, &project.root, &project.database_path)?;
            Ok(())
        }
    }
}
