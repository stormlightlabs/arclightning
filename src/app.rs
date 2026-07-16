use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::{fs, io};
use std::{fs::OpenOptions, io::Write};

use anyhow::{Context, Result};
use thiserror::Error;

use crate::domain::{DomainError, Idea, IdeaId, validate_title};
use crate::output::{IdeaMutation, OutputMode, Renderer};
use crate::{
    cli::{Cli, Command, DescriptionArgs, IdeaCommand},
    snapshot::{ProjectConfig, SnapshotError},
    storage::{Database, StorageError},
    vcs::{GixVcs, Vcs, VcsError},
};

const ARCL_DIRECTORY: &str = ".arcl";
const CONFIG_FILE: &str = "config.toml";
const DATABASE_FILE: &str = "arcl.db";
const GITIGNORE_FILE: &str = ".gitignore";
const REQUIRED_GITIGNORE_ENTRIES: &[&str] = &["/arcl.db", "/arcl.db-*", "/*.tmp", "/conflicts/"];

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
            Self::OpenDatabase { source, .. } => storage_exit_code(source),
            Self::Storage(error) => storage_exit_code(error),
            Self::InvalidDescription { .. } | Self::InvalidStdin => 3,
            Self::StdinIsTerminal => 3,
        }
    }
}

fn storage_exit_code(error: &StorageError) -> u8 {
    match error {
        StorageError::IdeaNotFound { .. } => 5,
        StorageError::InvalidIdea(_) | StorageError::NewerDatabase { .. } | StorageError::MigrationGap { .. } => 3,
        StorageError::Sqlite(_) => 1,
    }
}

enum IdeaCommandResult {
    Mutation { action: IdeaMutation, idea: Idea },
    List(Vec<Idea>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Initialization {
    root: PathBuf,
    snapshot_enabled: bool,
}

/// Run one parsed CLI invocation at the application boundary.
pub fn run(cli: Cli) -> Result<()> {
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
            let result = execute_idea(command).map_err(ApplicationError::from_command)?;
            let message = match result {
                IdeaCommandResult::Mutation { action, idea } => renderer.render_idea(action, &idea),
                IdeaCommandResult::List(ideas) => renderer.render_ideas(&ideas),
            }
            .context("rendering idea output")?;
            write_output(message)
        }
        None => write_output(renderer.render_startup().context("rendering CLI output")?),
    }
}

fn write_output(message: Option<String>) -> Result<()> {
    if let Some(message) = message {
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        writeln!(stdout, "{message}").context("writing CLI output")?;
    }
    Ok(())
}

fn execute_idea(command: IdeaCommand) -> std::result::Result<IdeaCommandResult, CommandError> {
    match command {
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

fn open_database() -> std::result::Result<Database, CommandError> {
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
    Database::open(&database_path).map_err(|source| CommandError::OpenDatabase { path: database_path, source })
}

fn resolve_description(args: DescriptionArgs) -> std::result::Result<Option<String>, CommandError> {
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

fn read_stdin_description() -> std::result::Result<Option<String>, CommandError> {
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

fn initialize(start: &Path, snapshot: bool) -> std::result::Result<Initialization, InitError> {
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

fn load_or_create_config(path: &Path, snapshot: bool) -> std::result::Result<ProjectConfig, InitError> {
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

fn ensure_gitignore(path: &Path) -> std::result::Result<(), InitError> {
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
