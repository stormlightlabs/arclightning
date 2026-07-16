use std::path::{Path, PathBuf};
use std::{fs, io};
use std::{fs::OpenOptions, io::Write};

use anyhow::{Context, Result};
use thiserror::Error;

use crate::output::{OutputMode, Renderer};
use crate::{
    cli::{Cli, Command},
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

    if let Some(Command::Init { snapshot }) = cli.command {
        let start = std::env::current_dir().context("could not determine the current directory")?;
        let initialization = initialize(&start, snapshot).map_err(ApplicationError::from_init)?;
        if let Some(message) = renderer
            .render_init(&initialization.root, initialization.snapshot_enabled)
            .context("rendering initialization output")?
        {
            let stdout = io::stdout();
            let mut stdout = stdout.lock();
            writeln!(stdout, "{message}").context("writing CLI output")?;
        }
        return Ok(());
    }

    match renderer.render_startup().context("rendering CLI output")? {
        Some(message) => {
            let stdout = io::stdout();
            let mut stdout = stdout.lock();
            writeln!(stdout, "{message}").context("writing CLI output")?;
            Ok(())
        }
        None => Ok(()),
    }
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
