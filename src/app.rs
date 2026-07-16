use std::ffi::OsStr;
use std::io::{IsTerminal, Read};
use std::path::{Component, Path, PathBuf};
use std::{fs, io};
use std::{fs::OpenOptions, io::Write};

use anyhow::Context;
use thiserror::Error;

use crate::domain::{
    DomainError, Epic, EpicId, Idea, IdeaId, Milestone, MilestoneId, Release, ReleaseId, Task, TaskId, TaskPriority,
    validate_title,
};
use crate::output::{
    EpicMutation, IdeaMutation, MilestoneMutation, OutputMode, ReleaseMutation, Renderer, TaskMutation,
};
use crate::{
    cli::{Cli, Command, DescriptionArgs, EpicCommand, IdeaCommand, MilestoneCommand, ReleaseCommand, TaskCommand},
    snapshot::{ProjectConfig, SnapshotError},
    storage::{Database, StorageError, TaskUpdate},
    vcs::{GixVcs, Vcs, VcsError},
};

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
            Self::InvalidDescription { .. } | Self::InvalidStdin | Self::InvalidSpecPath(_) => 3,
            Self::StdinIsTerminal => 3,
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
            | StorageError::DuplicateSpec { .. }
            | StorageError::NewerDatabase { .. }
            | StorageError::MigrationGap { .. } => 3,
            StorageError::Sqlite(_) => 1,
        }
    }
}

enum IdeaCommandResult {
    Mutation { action: IdeaMutation, idea: Idea },
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
    }
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
    }
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
    }
}

fn exec_task(command: TaskCommand) -> CResult<TaskCommandResult> {
    match command {
        TaskCommand::Create { title, milestone, parent, priority, position, description } => {
            validate_title(&title)?;
            let milestone_id = MilestoneId::parse(&milestone).map_err(DomainError::from)?;
            let parent_id = parent
                .as_deref()
                .map(TaskId::parse)
                .transpose()
                .map_err(DomainError::from)?;
            let priority = TaskPriority::parse(&priority)?;
            let description = resolve_description(description)?.unwrap_or_default();
            let mut database = open_database()?;
            let task = database.create_task(milestone_id, parent_id, title, description, priority, position)?;
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
    }
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
