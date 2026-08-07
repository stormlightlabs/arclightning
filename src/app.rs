use std::ffi::OsStr;
use std::io::{IsTerminal, Read};
use std::path::{Component, Path, PathBuf};
use std::{fs, io};
use std::{fs::OpenOptions, io::Write};

use anyhow::Context;
use thiserror::Error;

use crate::{cli::*, domain::*, output::*};

use crate::snapshot::{ProjectConfig, SnapshotError};
use crate::storage::{Database, Graph, ListFilter, Promotion, ReadyFilter, StorageError, TaskCreate, TaskUpdate};
use crate::vcs::{GixVcs, Vcs, VcsError};

const ARCL_DIRECTORY: &str = ".arcl";
const CONFIG_FILE: &str = "config.toml";
const DATABASE_FILE: &str = "arcl.db";
const GITIGNORE_FILE: &str = ".gitignore";
const REQUIRED_GITIGNORE_ENTRIES: &[&str] = &["/arcl.db", "/arcl.db-*", "/*.tmp", "/conflicts/"];

type CResult<T> = std::result::Result<T, CommandError>;
type SResult<T> = std::result::Result<T, SpecPathError>;
type IResult<T> = std::result::Result<T, InitError>;

/// A typed application failure carrying the roadmap's process exit category.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct ApplicationError {
    message: String,
    exit_code: u8,
}

impl ApplicationError {
    pub const fn exit_code(&self) -> u8 {
        self.exit_code
    }

    fn from_init(error: InitError) -> Self {
        let exit_code = error.exit_code();
        let message = if exit_code == 3 { format!("invalid project: {error}") } else { error.to_string() };
        Self { message, exit_code }
    }

    fn from_command(error: CommandError) -> Self {
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
    #[error("could not read scoped ignore file `{path}`: {source}")]
    ReadGitignore { path: PathBuf, source: io::Error },
    #[error("could not write scoped ignore file `{path}`: {source}")]
    WriteGitignore { path: PathBuf, source: io::Error },
    #[error("could not initialize database `{path}`: {source}")]
    OpenDatabase { path: PathBuf, source: StorageError },
}

impl From<&InitError> for u8 {
    fn from(value: &InitError) -> Self {
        value.exit_code()
    }
}

impl InitError {
    fn exit_code(&self) -> u8 {
        match self {
            Self::Vcs(
                VcsError::Discovery { .. } | VcsError::BareRepository { .. } | VcsError::MissingWorktree { .. },
            )
            | Self::InvalidConfig { .. } => 3,
            Self::Vcs(_)
            | Self::CreateDirectory { .. }
            | Self::ReadConfig { .. }
            | Self::WriteConfig { .. }
            | Self::RenderConfig { .. }
            | Self::ReadGitignore { .. }
            | Self::WriteGitignore { .. }
            | Self::OpenDatabase { .. } => 1,
        }
    }
}

#[derive(Debug, Error)]
enum CommandError {
    #[error(transparent)]
    Vcs(#[from] VcsError),
    #[error("Arc Lightning is not initialized in Git worktree `{root}`; run `arcl init` first")]
    NotInitialized { root: PathBuf },
    #[error("could not read project configuration `{path}`: {source}")]
    ReadConfig { path: PathBuf, source: io::Error },
    #[error("could not determine the current directory: {source}")]
    CurrentDirectory { source: io::Error },
    #[error("project configuration `{path}` is invalid: {source}")]
    InvalidConfig { path: PathBuf, source: SnapshotError },
    #[error("could not open Arc Lightning database `{path}`: {source}")]
    OpenDatabase { path: PathBuf, source: StorageError },
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    InvalidSpecPath(#[from] SpecPathError),
    #[error("description file `{path}` could not be read: {source}")]
    ReadDescription { path: PathBuf, source: io::Error },
    #[error("description file `{path}` is not valid UTF-8")]
    InvalidDescription { path: PathBuf },
    #[error("standard input could not be read: {source}")]
    ReadStdin { source: io::Error },
    #[error("standard input is a terminal; pipe UTF-8 Markdown to `--description-file -` or use `--description`")]
    StdinIsTerminal,
    #[error("standard input is not valid UTF-8")]
    InvalidStdin,
    #[error("invalid list filter: {message}")]
    InvalidFilter { message: String },
    #[error("database or graph integrity check failed: {message}")]
    Integrity { message: String },
}

impl CommandError {
    fn exit_code(&self) -> u8 {
        match self {
            Self::Vcs(error) => match error {
                VcsError::Discovery { .. } | VcsError::BareRepository { .. } | VcsError::MissingWorktree { .. } => 3,
                VcsError::PathOutsideWorktree { .. } | VcsError::Operation { .. } => 1,
            },
            Self::NotInitialized { .. } | Self::InvalidConfig { .. } | Self::Domain(_) => 3,
            Self::ReadConfig { .. }
            | Self::CurrentDirectory { .. }
            | Self::ReadDescription { .. }
            | Self::ReadStdin { .. }
            | Self::OpenDatabase { source: StorageError::Sqlite(_), .. } => 1,
            Self::OpenDatabase { source, .. } => u8::from(source),
            Self::Storage(error) => u8::from(error),
            Self::InvalidDescription { .. }
            | Self::InvalidStdin
            | Self::InvalidSpecPath(_)
            | Self::InvalidFilter { .. }
            | Self::Integrity { .. }
            | Self::StdinIsTerminal => 3,
        }
    }
}

impl From<&StorageError> for u8 {
    fn from(value: &StorageError) -> Self {
        match value {
            StorageError::IdeaNotFound { .. } => 5,
            StorageError::ReleaseNotFound { .. }
            | StorageError::EpicNotFound { .. }
            | StorageError::MilestoneNotFound { .. }
            | StorageError::TaskNotFound { .. } => 5,
            StorageError::InvalidIdea(_)
            | StorageError::InvalidRelease(_)
            | StorageError::InvalidEpic(_)
            | StorageError::InvalidMilestone(_)
            | StorageError::InvalidTask(_)
            | StorageError::InvalidDependency(_)
            | StorageError::DuplicateSpec { .. }
            | StorageError::IdeaNotPromotable { .. }
            | StorageError::InconsistentPromotion { .. }
            | StorageError::NewerDatabase { .. }
            | StorageError::MigrationGap { .. } => 3,
            StorageError::DependencyNotFound { .. } => 5,
            StorageError::Sqlite(_) => 1,
        }
    }
}

enum IdeaCommandResult {
    Mutation { action: IdeaMutation, idea: Idea },
    Promotion(Promotion),
    List(Vec<Idea>),
}

enum ReleaseCommandResult {
    Mutation { action: ReleaseMutation, release: Release },
}

enum EpicCommandResult {
    Mutation { action: EpicMutation, epic: Epic },
}

enum MilestoneCommandResult {
    Mutation {
        action: MilestoneMutation,
        milestone: Milestone,
    },
}

enum TaskCommandResult {
    Mutation { action: TaskMutation, task: Task },
}

enum DependencyCommandResult {
    Mutation {
        action: DependencyMutation,
        dependency: TaskDependency,
    },
}

#[derive(Debug, Error)]
enum SpecPathError {
    #[error("spec path cannot be empty")]
    Empty,
    #[error("spec path `{path}` must be relative to the current directory")]
    Absolute { path: PathBuf },
    #[error("spec path `{path}` cannot contain `..` path components")]
    Traversal { path: PathBuf },
    #[error("could not resolve the Git worktree root `{path}`: {source}")]
    CanonicalizeRoot { path: PathBuf, source: io::Error },
    #[error("could not resolve current directory `{path}`: {source}")]
    CanonicalizeCurrentDirectory { path: PathBuf, source: io::Error },
    #[error("could not resolve spec path `{path}`: {source}")]
    Resolve { path: PathBuf, source: io::Error },
    #[error("spec path `{path}` resolves outside the Git worktree")]
    OutsideWorktree { path: PathBuf },
    #[error("spec path `{path}` is not a regular file")]
    NotRegularFile { path: PathBuf },
    #[error("spec path `{path}` is not a Markdown file ending in `.md`")]
    NotMarkdown { path: PathBuf },
    #[error("spec path `{path}` is not valid UTF-8")]
    NonUtf8 { path: PathBuf },
}

struct OpenProject {
    root: PathBuf,
    current_dir: PathBuf,
    database: Database,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Initialization {
    root: PathBuf,
    snapshot_enabled: bool,
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
            let message = renderer
                .render_init(&initialization.root, initialization.snapshot_enabled)
                .context("rendering initialization output")?;
            write_output(message)
        }
        Some(Command::Idea { command }) => {
            let result = exec_idea(command).map_err(ApplicationError::from_command)?;
            let message = match result {
                IdeaCommandResult::Mutation { action, idea } => renderer.render_idea(action, &idea),
                IdeaCommandResult::Promotion(promotion) => renderer.render_promotion(&promotion),
                IdeaCommandResult::List(ideas) => renderer.render_ideas(&ideas),
            }
            .context("rendering idea output")?;
            write_output(message)
        }
        Some(Command::Release { command }) => {
            let result = exec_release(command).map_err(ApplicationError::from_command)?;
            let ReleaseCommandResult::Mutation { action, release } = result;
            let message = renderer
                .render_release(action, &release)
                .context("rendering release output")?;
            write_output(message)
        }
        Some(Command::Epic { command }) => {
            let result = exec_epic(command).map_err(ApplicationError::from_command)?;
            let EpicCommandResult::Mutation { action, epic } = result;
            let message = renderer.render_epic(action, &epic).context("rendering epic output")?;
            write_output(message)
        }
        Some(Command::Milestone { command }) => {
            let result = exec_milestone(command).map_err(ApplicationError::from_command)?;
            let MilestoneCommandResult::Mutation { action, milestone } = result;
            let message = renderer
                .render_milestone(action, &milestone)
                .context("rendering milestone output")?;
            write_output(message)
        }
        Some(Command::Task { command }) => {
            let result = exec_task(command).map_err(ApplicationError::from_command)?;
            let TaskCommandResult::Mutation { action, task } = result;
            let message = renderer.render_task(action, &task).context("rendering task output")?;
            write_output(message)
        }
        Some(Command::Dependency { command }) => {
            let result = exec_dependency(command).map_err(ApplicationError::from_command)?;
            let DependencyCommandResult::Mutation { action, dependency } = result;
            let message = renderer
                .render_dependency(action, &dependency)
                .context("rendering dependency output")?;
            write_output(message)
        }
        Some(Command::Ready { filters }) => {
            let tasks = exec_ready(filters).map_err(ApplicationError::from_command)?;
            let message = renderer
                .render_ready_tasks(&tasks)
                .context("rendering ready-work output")?;
            write_output(message)
        }
        Some(Command::Next { filters }) => {
            let task = exec_ready(filters)
                .map_err(ApplicationError::from_command)?
                .into_iter()
                .next();
            let message = renderer
                .render_next_task(task.as_ref())
                .context("rendering next-work output")?;
            write_output(message)
        }
        Some(Command::Show { id }) => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            let record = database
                .show(&id)
                .map_err(|error| ApplicationError::from_command(CommandError::Storage(error)))?;
            write_output(renderer.render_show(&record).context("rendering show output")?)
        }
        Some(Command::List { filters }) => {
            let filter = resolve_list_filter(filters).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            let graph = database
                .graph()
                .map_err(|error| ApplicationError::from_command(CommandError::Storage(error)))?;
            validate_list_targets(&graph, &filter).map_err(ApplicationError::from_command)?;
            let records = graph.list_items(&filter);
            write_output(renderer.render_list(&records).context("rendering list output")?)
        }
        Some(Command::Tree { id }) => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            let tree = database
                .graph()
                .map_err(|error| ApplicationError::from_command(CommandError::Storage(error)))?
                .tree(id.as_deref())
                .map_err(|error| ApplicationError::from_command(CommandError::Storage(error)))?;
            write_output(renderer.render_tree(&tree).context("rendering tree output")?)
        }
        Some(Command::Explain { task_id }) => {
            let task_id = TaskId::parse(&task_id)
                .map_err(DomainError::from)
                .map_err(CommandError::Domain)
                .map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            let view = database
                .task_view(task_id)
                .map_err(|error| ApplicationError::from_command(CommandError::Storage(error)))?;
            write_output(renderer.render_explain(&view).context("rendering explain output")?)
        }
        Some(Command::Context { task_id }) => {
            let task_id = TaskId::parse(&task_id)
                .map_err(DomainError::from)
                .map_err(CommandError::Domain)
                .map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            let view = database
                .context(task_id)
                .map_err(|error| ApplicationError::from_command(CommandError::Storage(error)))?;
            write_output(renderer.render_context(&view).context("rendering context output")?)
        }
        Some(Command::Check) => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            let report = database
                .check()
                .map_err(|error| ApplicationError::from_command(CommandError::Storage(error)))?;
            let valid = report.valid;
            let details = report.errors.join("; ");
            write_output(renderer.render_check(&report).context("rendering check output")?)?;
            if valid {
                Ok(())
            } else {
                Err(ApplicationError::from_command(CommandError::Integrity { message: details }).into())
            }
        }
        None => write_output(renderer.render_startup().context("rendering CLI output")?),
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

fn exec_idea(cmd: IdeaCommand) -> CResult<IdeaCommandResult> {
    match cmd {
        IdeaCommand::Create { title, description } => {
            validate_title(&title)?;
            let description = resolve_description(description)?.unwrap_or_default();
            let mut database = open_database()?;
            let idea = database.create_idea(title, description)?;
            Ok(IdeaCommandResult::Mutation { action: IdeaMutation::Created, idea })
        }
        IdeaCommand::Update { id, title, description } => {
            let id = IdeaId::parse(&id).map_err(DomainError::from)?;
            if let Some(title) = &title {
                validate_title(title)?;
            }
            let description = resolve_description(description)?;
            if title.is_none() && description.is_none() {
                return Err(CommandError::Domain(DomainError::NoFieldsToUpdate { entity: "idea" }));
            }
            let mut database = open_database()?;
            let idea = database.update_idea(id, title, description)?;
            Ok(IdeaCommandResult::Mutation { action: IdeaMutation::Updated, idea })
        }
        IdeaCommand::Discard { id } => {
            let id = IdeaId::parse(&id).map_err(DomainError::from)?;
            let mut database = open_database()?;
            let idea = database.discard_idea(id)?;
            Ok(IdeaCommandResult::Mutation { action: IdeaMutation::Discarded, idea })
        }
        IdeaCommand::List => {
            let database = open_database()?;
            Ok(IdeaCommandResult::List(database.ideas()?))
        }
        IdeaCommand::Promote { id, spec, release } => {
            let id = IdeaId::parse(&id).map_err(DomainError::from)?;
            let release_id = release
                .as_deref()
                .map(ReleaseId::parse)
                .transpose()
                .map_err(DomainError::from)?;
            let mut project = open_project()?;
            let idea = project
                .database
                .idea(id)?
                .ok_or_else(|| StorageError::IdeaNotFound { id: id.to_string() })?;
            let spec_path = resolve_spec_path(&project.root, &project.current_dir, &spec)?;
            let promotion = project
                .database
                .promote_idea(id, idea.title, idea.description, spec_path, release_id)?;
            Ok(IdeaCommandResult::Promotion(promotion))
        }
    }
}

fn exec_release(cmd: ReleaseCommand) -> CResult<ReleaseCommandResult> {
    match cmd {
        ReleaseCommand::Create { title, description } => {
            validate_title(&title)?;
            let description = resolve_description(description)?.unwrap_or_default();
            let mut database = open_database()?;
            let release = database.create_release(title, description)?;
            Ok(ReleaseCommandResult::Mutation { action: ReleaseMutation::Created, release })
        }
        ReleaseCommand::Update { id, title, description } => {
            let id = ReleaseId::parse(&id).map_err(DomainError::from)?;
            if let Some(title) = &title {
                validate_title(title)?;
            }
            let description = resolve_description(description)?;
            if title.is_none() && description.is_none() {
                return Err(CommandError::Domain(DomainError::NoFieldsToUpdate {
                    entity: "release",
                }));
            }
            let mut database = open_database()?;
            let release = database.update_release(id, title, description)?;
            Ok(ReleaseCommandResult::Mutation { action: ReleaseMutation::Updated, release })
        }
        ReleaseCommand::Complete { id, allow_open_children } => {
            transition_release(id, ContainerAction::Complete, allow_open_children)
        }
        ReleaseCommand::Cancel { id, allow_open_children } => {
            transition_release(id, ContainerAction::Cancel, allow_open_children)
        }
    }
}

fn transition_release(id: String, action: ContainerAction, allow_open_children: bool) -> CResult<ReleaseCommandResult> {
    let id = ReleaseId::parse(&id).map_err(DomainError::from)?;
    let mut database = open_database()?;
    let release = database.transition_release(id, action, allow_open_children)?;
    let action = match action {
        ContainerAction::Complete => ReleaseMutation::Completed,
        ContainerAction::Cancel => ReleaseMutation::Cancelled,
    };
    Ok(ReleaseCommandResult::Mutation { action, release })
}

fn exec_epic(command: EpicCommand) -> CResult<EpicCommandResult> {
    match command {
        EpicCommand::Create { title, spec, release, description } => {
            validate_title(&title)?;
            let release_id = release
                .as_deref()
                .map(ReleaseId::parse)
                .transpose()
                .map_err(DomainError::from)?;
            let description = resolve_description(description)?.unwrap_or_default();
            let mut project = open_project()?;
            let spec_path = resolve_spec_path(&project.root, &project.current_dir, &spec)?;
            let epic = project
                .database
                .create_epic(title, description, spec_path, release_id)?;
            Ok(EpicCommandResult::Mutation { action: EpicMutation::Created, epic })
        }
        EpicCommand::Update { id, title, spec, release, no_release, description } => {
            let id = EpicId::parse(&id).map_err(DomainError::from)?;
            if let Some(title) = &title {
                validate_title(title)?;
            }
            let description = resolve_description(description)?;
            let release_change = if no_release {
                Some(None)
            } else {
                release
                    .as_deref()
                    .map(ReleaseId::parse)
                    .transpose()
                    .map_err(DomainError::from)?
                    .map(Some)
            };
            if title.is_none() && spec.is_none() && description.is_none() && release_change.is_none() {
                return Err(CommandError::Domain(DomainError::NoFieldsToUpdate { entity: "epic" }));
            }
            let mut project = open_project()?;
            let spec_path = spec
                .as_ref()
                .map(|spec| resolve_spec_path(&project.root, &project.current_dir, spec))
                .transpose()?;
            let epic = project
                .database
                .update_epic(id, title, description, spec_path, release_change)?;
            Ok(EpicCommandResult::Mutation { action: EpicMutation::Updated, epic })
        }
        EpicCommand::Complete { id, allow_open_children } => {
            transition_epic(id, ContainerAction::Complete, allow_open_children)
        }
        EpicCommand::Cancel { id, allow_open_children } => {
            transition_epic(id, ContainerAction::Cancel, allow_open_children)
        }
    }
}

fn transition_epic(id: String, action: ContainerAction, allow_open_children: bool) -> CResult<EpicCommandResult> {
    let id = EpicId::parse(&id).map_err(DomainError::from)?;
    let mut database = open_database()?;
    let epic = database.transition_epic(id, action, allow_open_children)?;
    let action = match action {
        ContainerAction::Complete => EpicMutation::Completed,
        ContainerAction::Cancel => EpicMutation::Cancelled,
    };
    Ok(EpicCommandResult::Mutation { action, epic })
}

fn exec_milestone(command: MilestoneCommand) -> CResult<MilestoneCommandResult> {
    match command {
        MilestoneCommand::Create { title, epic, position, description } => {
            validate_title(&title)?;
            let epic_id = EpicId::parse(&epic).map_err(DomainError::from)?;
            let description = resolve_description(description)?.unwrap_or_default();
            let mut database = open_database()?;
            let milestone = database.create_milestone(epic_id, title, description, position)?;
            Ok(MilestoneCommandResult::Mutation { action: MilestoneMutation::Created, milestone })
        }
        MilestoneCommand::Update { id, title, position, description } => {
            let id = MilestoneId::parse(&id).map_err(DomainError::from)?;
            if let Some(title) = &title {
                validate_title(title)?;
            }
            let description = resolve_description(description)?;
            if title.is_none() && position.is_none() && description.is_none() {
                return Err(CommandError::Domain(DomainError::NoFieldsToUpdate {
                    entity: "milestone",
                }));
            }
            let mut database = open_database()?;
            let milestone = database.update_milestone(id, title, description, position)?;
            Ok(MilestoneCommandResult::Mutation { action: MilestoneMutation::Updated, milestone })
        }
        MilestoneCommand::Complete { id, allow_open_children } => {
            transition_milestone(id, ContainerAction::Complete, allow_open_children)
        }
        MilestoneCommand::Cancel { id, allow_open_children } => {
            transition_milestone(id, ContainerAction::Cancel, allow_open_children)
        }
    }
}

fn transition_milestone(
    id: String, action: ContainerAction, allow_open_children: bool,
) -> CResult<MilestoneCommandResult> {
    let id = MilestoneId::parse(&id).map_err(DomainError::from)?;
    let mut database = open_database()?;
    let milestone = database.transition_milestone(id, action, allow_open_children)?;
    let action = match action {
        ContainerAction::Complete => MilestoneMutation::Completed,
        ContainerAction::Cancel => MilestoneMutation::Cancelled,
    };
    Ok(MilestoneCommandResult::Mutation { action, milestone })
}

fn exec_task(command: TaskCommand) -> CResult<TaskCommandResult> {
    match command {
        TaskCommand::Create { title, milestone, parent, priority, position, blocked_by, description } => {
            validate_title(&title)?;
            let milestone_id = MilestoneId::parse(&milestone).map_err(DomainError::from)?;
            let parent_id = parent
                .as_deref()
                .map(TaskId::parse)
                .transpose()
                .map_err(DomainError::from)?;
            let blockers = blocked_by
                .iter()
                .map(|id| TaskId::parse(id).map_err(DomainError::from))
                .collect::<Result<Vec<_>, _>>()?;
            let priority = TaskPriority::parse(&priority)?;
            let description = resolve_description(description)?.unwrap_or_default();
            let mut database = open_database()?;
            let task = database.create_task_with_dependencies(TaskCreate {
                milestone_id,
                parent_id,
                title,
                description,
                priority,
                position,
                blockers,
            })?;
            Ok(TaskCommandResult::Mutation { action: TaskMutation::Created, task })
        }
        TaskCommand::Update { id, title, priority, position, milestone, parent, no_parent, description } => {
            let id = TaskId::parse(&id).map_err(DomainError::from)?;
            if let Some(title) = &title {
                validate_title(title)?;
            }
            let priority = priority.as_deref().map(TaskPriority::parse).transpose()?;
            let milestone_id = milestone
                .as_deref()
                .map(MilestoneId::parse)
                .transpose()
                .map_err(DomainError::from)?;
            let parent_change = if no_parent {
                Some(None)
            } else {
                parent
                    .as_deref()
                    .map(TaskId::parse)
                    .transpose()
                    .map_err(DomainError::from)?
                    .map(Some)
            };
            let description = resolve_description(description)?;
            if title.is_none()
                && priority.is_none()
                && position.is_none()
                && milestone_id.is_none()
                && parent_change.is_none()
                && description.is_none()
            {
                return Err(CommandError::Domain(DomainError::NoFieldsToUpdate { entity: "task" }));
            }
            let mut database = open_database()?;
            let task = database.update_task(
                id,
                TaskUpdate { title, description, priority, position, milestone_id, parent_change },
            )?;
            Ok(TaskCommandResult::Mutation { action: TaskMutation::Updated, task })
        }
        TaskCommand::Start { id } => transition_task(id, TaskAction::Start, false),
        TaskCommand::Park { id } => transition_task(id, TaskAction::Park, false),
        TaskCommand::Unpark { id } => transition_task(id, TaskAction::Unpark, false),
        TaskCommand::Handoff { id, note, note_file } => {
            let note = resolve_description(DescriptionArgs { description: note, description_file: note_file })?
                .ok_or_else(|| CommandError::Domain(DomainError::NoFieldsToUpdate { entity: "handoff" }))?;
            let id = TaskId::parse(&id).map_err(DomainError::from)?;
            let mut database = open_database()?;
            let task = database.handoff_task(id, note)?;
            Ok(TaskCommandResult::Mutation { action: TaskMutation::HandedOff, task })
        }
        TaskCommand::Complete { id, allow_open_children, evidence, evidence_file } => {
            let evidence =
                resolve_description(DescriptionArgs { description: evidence, description_file: evidence_file })?;
            let id = TaskId::parse(&id).map_err(DomainError::from)?;
            let mut database = open_database()?;
            let task = database.complete_task(id, allow_open_children, evidence)?;
            Ok(TaskCommandResult::Mutation { action: TaskMutation::Completed, task })
        }
        TaskCommand::Cancel { id, allow_open_children } => transition_task(id, TaskAction::Cancel, allow_open_children),
    }
}

fn exec_dependency(command: DependencyCommand) -> CResult<DependencyCommandResult> {
    let (task_id, blocker_id, action) = match command {
        DependencyCommand::Add { task_id, blocker_id } => (task_id, blocker_id, DependencyMutation::Added),
        DependencyCommand::Remove { task_id, blocker_id } => (task_id, blocker_id, DependencyMutation::Removed),
    };
    let task_id = TaskId::parse(&task_id).map_err(DomainError::from)?;
    let blocker_id = TaskId::parse(&blocker_id).map_err(DomainError::from)?;
    let mut database = open_database()?;
    let dependency = match action {
        DependencyMutation::Added => database.add_dependency(task_id, blocker_id)?,
        DependencyMutation::Removed => database.remove_dependency(task_id, blocker_id)?,
    };
    Ok(DependencyCommandResult::Mutation { action, dependency })
}

fn exec_ready(args: ReadyArgs) -> CResult<Vec<Task>> {
    let filter = resolve_ready_filter(args)?;
    let database = open_database()?;
    validate_ready_targets(&database, &filter)?;
    Ok(database.ready_tasks_filtered(&filter)?)
}

fn validate_ready_targets(database: &Database, filter: &ReadyFilter) -> CResult<()> {
    if let Some(id) = filter.release_id
        && database.release(id)?.is_none()
    {
        return Err(CommandError::Storage(StorageError::ReleaseNotFound {
            id: id.to_string(),
        }));
    }
    if let Some(id) = filter.epic_id
        && database.epic(id)?.is_none()
    {
        return Err(CommandError::Storage(StorageError::EpicNotFound { id: id.to_string() }));
    }
    if let Some(id) = filter.milestone_id
        && database.milestone(id)?.is_none()
    {
        return Err(CommandError::Storage(StorageError::MilestoneNotFound {
            id: id.to_string(),
        }));
    }
    if let Some(id) = filter.parent_id
        && database.task(id)?.is_none()
    {
        return Err(CommandError::Storage(StorageError::TaskNotFound { id: id.to_string() }));
    }
    Ok(())
}

fn resolve_list_filter(args: ListArgs) -> CResult<ListFilter> {
    let mut kind = args.kind;
    if let Some(value) = &kind {
        if !matches!(value.as_str(), "idea" | "release" | "epic" | "milestone" | "task") {
            return Err(CommandError::InvalidFilter {
                message: format!("unknown kind `{value}`; use idea, release, epic, milestone, or task"),
            });
        }
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
    if let Some(value) = args
        .status
        .iter()
        .find(|value| !known_statuses.contains(&value.as_str()))
    {
        return Err(CommandError::InvalidFilter { message: format!("unknown status `{value}`") });
    }
    let priorities = args
        .priority
        .iter()
        .map(|priority| TaskPriority::parse(priority))
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(kind_name) = kind.as_deref() {
        let allowed = match kind_name {
            "idea" => ["captured", "promoted", "discarded"].as_slice(),
            "release" | "epic" | "milestone" => ["open", "completed", "cancelled"].as_slice(),
            "task" => ["pending", "in_progress", "parked", "completed", "cancelled"].as_slice(),
            _ => &[][..],
        };
        if let Some(status) = args.status.iter().find(|status| !allowed.contains(&status.as_str())) {
            return Err(CommandError::InvalidFilter {
                message: format!("status `{status}` does not apply to {kind_name} records"),
            });
        }
    }
    if (!priorities.is_empty() || args.parent.is_some()) && kind.as_deref().is_some_and(|kind| kind != "task") {
        return Err(CommandError::InvalidFilter {
            message: "priority and parent filters apply only to tasks".to_owned(),
        });
    }
    if let Some(kind_name) = kind.as_deref() {
        let invalid = match kind_name {
            "idea" => args.release.is_some() || args.epic.is_some() || args.milestone.is_some(),
            "release" => args.epic.is_some() || args.milestone.is_some(),
            "epic" => args.milestone.is_some(),
            "milestone" => args.parent.is_some() || !priorities.is_empty(),
            "task" => false,
            _ => false,
        };
        if invalid {
            return Err(CommandError::InvalidFilter {
                message: format!("the supplied filters do not apply to {kind_name} records"),
            });
        }
    }
    if kind.is_none() && (!priorities.is_empty() || args.parent.is_some()) {
        kind = Some("task".to_owned());
    }
    let release_id = args
        .release
        .as_deref()
        .map(ReleaseId::parse)
        .transpose()
        .map_err(DomainError::from)?;
    let epic_id = args
        .epic
        .as_deref()
        .map(EpicId::parse)
        .transpose()
        .map_err(DomainError::from)?;
    let milestone_id = args
        .milestone
        .as_deref()
        .map(MilestoneId::parse)
        .transpose()
        .map_err(DomainError::from)?;
    let parent_id = args
        .parent
        .as_deref()
        .map(TaskId::parse)
        .transpose()
        .map_err(DomainError::from)?;
    Ok(ListFilter { kind, statuses: args.status, priorities, release_id, epic_id, milestone_id, parent_id })
}

fn validate_list_targets(graph: &Graph, filter: &ListFilter) -> CResult<()> {
    if let Some(id) = filter.release_id {
        if !graph.releases.iter().any(|release| release.id == id) {
            return Err(CommandError::Storage(StorageError::ReleaseNotFound {
                id: id.to_string(),
            }));
        }
    }
    if let Some(id) = filter.epic_id {
        if !graph.epics.iter().any(|epic| epic.id == id) {
            return Err(CommandError::Storage(StorageError::EpicNotFound { id: id.to_string() }));
        }
    }
    if let Some(id) = filter.milestone_id {
        if !graph.milestones.iter().any(|milestone| milestone.id == id) {
            return Err(CommandError::Storage(StorageError::MilestoneNotFound {
                id: id.to_string(),
            }));
        }
    }
    if let Some(id) = filter.parent_id {
        if !graph.tasks.iter().any(|task| task.id == id) {
            return Err(CommandError::Storage(StorageError::TaskNotFound { id: id.to_string() }));
        }
    }
    Ok(())
}

fn resolve_ready_filter(args: ReadyArgs) -> CResult<ReadyFilter> {
    let priorities = args
        .priority
        .iter()
        .map(|priority| TaskPriority::parse(priority))
        .collect::<Result<Vec<_>, _>>()?;
    let release_id = args
        .release
        .as_deref()
        .map(ReleaseId::parse)
        .transpose()
        .map_err(DomainError::from)?;
    let epic_id = args
        .epic
        .as_deref()
        .map(EpicId::parse)
        .transpose()
        .map_err(DomainError::from)?;
    let milestone_id = args
        .milestone
        .as_deref()
        .map(MilestoneId::parse)
        .transpose()
        .map_err(DomainError::from)?;
    let parent_id = args
        .parent
        .as_deref()
        .map(TaskId::parse)
        .transpose()
        .map_err(DomainError::from)?;
    Ok(ReadyFilter { priorities, release_id, epic_id, milestone_id, parent_id })
}

fn transition_task(id: String, action: TaskAction, allow_open_children: bool) -> CResult<TaskCommandResult> {
    let id = TaskId::parse(&id).map_err(DomainError::from)?;
    let mut database = open_database()?;
    let task = database.transition_task(id, action, allow_open_children)?;
    let action = match action {
        TaskAction::Start => TaskMutation::Started,
        TaskAction::Park => TaskMutation::Parked,
        TaskAction::Unpark => TaskMutation::Unparked,
        TaskAction::Complete => TaskMutation::Completed,
        TaskAction::Cancel => TaskMutation::Cancelled,
    };
    Ok(TaskCommandResult::Mutation { action, task })
}

fn open_database() -> CResult<Database> {
    Ok(open_project()?.database)
}

fn open_project() -> CResult<OpenProject> {
    let start = std::env::current_dir().map_err(|source| CommandError::CurrentDirectory { source })?;
    let vcs = GixVcs::discover(&start)?;
    let root = vcs.worktree_root()?.to_owned();
    let arcl_directory = root.join(ARCL_DIRECTORY);
    let config_path = arcl_directory.join(CONFIG_FILE);
    let database_path = arcl_directory.join(DATABASE_FILE);

    if !config_path.is_file() || !database_path.is_file() {
        return Err(CommandError::NotInitialized { root });
    }

    let config = fs::read_to_string(&config_path)
        .map_err(|source| CommandError::ReadConfig { path: config_path.clone(), source })?;
    ProjectConfig::parse(&config).map_err(|source| CommandError::InvalidConfig { path: config_path, source })?;
    let database =
        Database::open(&database_path).map_err(|source| CommandError::OpenDatabase { path: database_path, source })?;
    Ok(OpenProject { root, current_dir: start, database })
}

fn resolve_spec_path(root: &Path, current_dir: &Path, input: &Path) -> SResult<String> {
    if input.as_os_str().is_empty() {
        return Err(SpecPathError::Empty);
    }

    let relative = normalize_relative_path(input)?;
    let canonical_root =
        fs::canonicalize(root).map_err(|source| SpecPathError::CanonicalizeRoot { path: root.to_owned(), source })?;
    let canonical_current = fs::canonicalize(current_dir)
        .map_err(|source| SpecPathError::CanonicalizeCurrentDirectory { path: current_dir.to_owned(), source })?;
    if !canonical_current.starts_with(&canonical_root) {
        return Err(SpecPathError::OutsideWorktree { path: current_dir.to_owned() });
    }

    let candidate = canonical_current.join(&relative);
    let resolved =
        fs::canonicalize(&candidate).map_err(|source| SpecPathError::Resolve { path: input.to_owned(), source })?;
    if !resolved.starts_with(&canonical_root) {
        return Err(SpecPathError::OutsideWorktree { path: input.to_owned() });
    }

    let metadata =
        fs::metadata(&resolved).map_err(|source| SpecPathError::Resolve { path: input.to_owned(), source })?;
    if !metadata.is_file() {
        return Err(SpecPathError::NotRegularFile { path: input.to_owned() });
    }
    if resolved.extension() != Some(OsStr::new("md")) {
        return Err(SpecPathError::NotMarkdown { path: input.to_owned() });
    }

    let root_relative = resolved
        .strip_prefix(&canonical_root)
        .map_err(|_| SpecPathError::OutsideWorktree { path: input.to_owned() })?;
    path_to_slash_string(root_relative).ok_or_else(|| SpecPathError::NonUtf8 { path: input.to_owned() })
}

fn normalize_relative_path(input: &Path) -> SResult<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in input.components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::ParentDir => return Err(SpecPathError::Traversal { path: input.to_owned() }),
            Component::RootDir | Component::Prefix(_) => {
                return Err(SpecPathError::Absolute { path: input.to_owned() });
            }
        }
    }
    if normalized.as_os_str().is_empty() { Err(SpecPathError::Empty) } else { Ok(normalized) }
}

fn path_to_slash_string(path: &Path) -> Option<String> {
    let mut components = Vec::new();
    for component in path.components() {
        let Component::Normal(part) = component else { return None };
        components.push(part.to_str()?);
    }
    (!components.is_empty()).then(|| components.join("/"))
}

fn resolve_description(args: DescriptionArgs) -> CResult<Option<String>> {
    if let Some(description) = args.description {
        return Ok(Some(description));
    }

    let Some(path) = args.description_file else {
        return Ok(None);
    };
    if path == Path::new("-") {
        return read_stdin_description();
    }

    let bytes = fs::read(&path).map_err(|source| CommandError::ReadDescription { path: path.clone(), source })?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| CommandError::InvalidDescription { path })
}

fn read_stdin_description() -> CResult<Option<String>> {
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

fn initialize(start: &Path, snapshot: bool) -> IResult<Initialization> {
    let vcs = GixVcs::discover(start)?;
    let root = vcs.worktree_root()?.to_owned();
    let arcl_directory = root.join(ARCL_DIRECTORY);
    fs::create_dir_all(&arcl_directory)
        .map_err(|source| InitError::CreateDirectory { path: arcl_directory.clone(), source })?;

    let config_path = arcl_directory.join(CONFIG_FILE);
    let config = load_or_create_config(&config_path, snapshot)?;

    let gitignore_path = arcl_directory.join(GITIGNORE_FILE);
    ensure_gitignore(&gitignore_path)?;

    let database_path = arcl_directory.join(DATABASE_FILE);
    Database::open(&database_path).map_err(|source| InitError::OpenDatabase { path: database_path, source })?;

    Ok(Initialization { root, snapshot_enabled: config.snapshot.enabled })
}

fn load_or_create_config(path: &Path, snapshot: bool) -> IResult<ProjectConfig> {
    match fs::read_to_string(path) {
        Ok(input) => {
            ProjectConfig::parse(&input).map_err(|source| InitError::InvalidConfig { path: path.to_owned(), source })
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
