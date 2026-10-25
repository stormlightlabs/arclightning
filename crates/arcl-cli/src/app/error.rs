use super::*;

pub(super) type CResult<T> = Result<T, CommandError>;
pub(super) type IResult<T> = Result<T, InitError>;

/// A typed application failure carrying the process exit category.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct ApplicationError {
    message: String,
    exit_code: u8,
    /// Whether another process held the project's sync lock past the wait.
    lock_busy: bool,
}

impl ApplicationError {
    /// Return the stable process exit category for this failure.
    pub const fn exit_code(&self) -> u8 {
        self.exit_code
    }

    /// Whether the command gave up waiting for the project's sync lock, which
    /// another `arcl` command held. Trying again later may succeed.
    pub const fn lock_busy(&self) -> bool {
        self.lock_busy
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

    pub(super) fn from_init(error: InitError) -> Self {
        let exit_code = error.exit_code();
        let message = if exit_code == 3 { format!("invalid project: {error}") } else { error.to_string() };
        Self { message, exit_code, lock_busy: false }
    }

    pub(super) fn from_command<E>(error: E) -> Self
    where
        E: Into<CommandError>,
    {
        let error: CommandError = error.into();
        let exit_code = error.exit_code();
        let lock_busy = matches!(error, CommandError::SyncLockBusy { .. });
        let message = if exit_code == 3 { format!("invalid project or record: {error}") } else { error.to_string() };
        Self { message, exit_code, lock_busy }
    }
}

#[derive(Debug, Error)]
pub(super) enum InitError {
    #[error(transparent)]
    Vcs(#[from] VcsError),
    #[error("could not create Arc Lightning directory `{path}`: {source}")]
    CreateDirectory { path: PathBuf, source: io::Error },
    #[error("could not read project configuration `{path}`: {source}")]
    ReadConfig { path: PathBuf, source: io::Error },
    #[error("project configuration `{path}` is invalid: {source}")]
    InvalidConfig { path: PathBuf, source: WorkspaceError },
    #[error("could not write project configuration `{path}`: {source}")]
    WriteConfig { path: PathBuf, source: io::Error },
    #[error("could not render project configuration `{path}`: {source}")]
    RenderConfig { path: PathBuf, source: WorkspaceError },
    #[error("could not create workspace directory `{path}`: {source}")]
    CreateWorkspaceDirectory { path: PathBuf, source: io::Error },
    #[error("could not render workspace manifest `{path}`: {source}")]
    RenderWorkspaceManifest { path: PathBuf, source: WorkspaceError },
    #[error("could not write workspace manifest `{path}`: {source}")]
    WriteWorkspaceManifest { path: PathBuf, source: io::Error },
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
            Self::Vcs(error) => error.exit_code(),
            Self::InvalidConfig { .. } => 3,
            _ => 1,
        }
    }
}

#[derive(Debug, Error)]
pub(super) enum CommandError {
    #[error(transparent)]
    Vcs(#[from] VcsError),
    #[error("Arc Lightning is not initialized in project directory `{root}`; run `arcl init` first")]
    NotInitialized { root: PathBuf },
    #[error("could not read project configuration `{path}`: {source}")]
    ReadConfig { path: PathBuf, source: io::Error },
    #[error("could not determine the current directory: {source}")]
    CurrentDirectory { source: io::Error },
    #[error("project configuration `{path}` is invalid: {source}")]
    InvalidConfig { path: PathBuf, source: WorkspaceError },
    #[error("could not open Arc Lightning database `{path}`: {source}")]
    OpenDatabase { path: PathBuf, source: StorageError },
    #[error("workspace support is disabled; run `arcl init --workspace` first")]
    WorkspaceDisabled,
    #[error(transparent)]
    WorkspaceExport(Box<WorkspaceExportError>),
    #[error(transparent)]
    WorkspaceImport(Box<WorkspaceImportError>),
    #[error(transparent)]
    WorkspaceSync(Box<WorkspaceSyncError>),
    #[error(transparent)]
    WorkspaceStatus(Box<WorkspaceStatusError>),
    #[error(transparent)]
    RecordEdit(Box<RecordEditError>),
    #[error(
        "the workspace must sync before this change, and the sync stopped: {0}\nRun `arcl workspace status` to see why{hint}",
        hint = .0.resolve_hint()
    )]
    AutoSync(Box<WorkspaceSyncError>),
    #[error("could not lock `{path}`: {source}")]
    SyncLock { path: PathBuf, source: io::Error },
    #[error(
        "another `arcl` command held the sync lock `{path}` for more than {} s; try again when it finishes",
        waited.as_secs_f64()
    )]
    SyncLockBusy { path: PathBuf, waited: Duration },
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
    #[error("could not read skill file `{path}`: {source}")]
    ReadSkill { path: PathBuf, source: io::Error },
    #[error("could not write skill file `{path}`: {source}")]
    WriteSkill { path: PathBuf, source: io::Error },
    #[error("skill file `{path}` is a symbolic link; replace it with a regular file or remove it")]
    SkillSymlink { path: PathBuf },
    #[error("skill file `{path}` is not a regular file")]
    SkillNotFile { path: PathBuf },
    #[error("skill path `{path}` is not a directory")]
    SkillNotDirectory { path: PathBuf },
    #[error("skill file `{path}` resolves outside the project root `{root}`")]
    SkillOutsideProject { path: PathBuf, root: PathBuf },
    #[error("{count} skill file(s) are missing or differ from this arcl; run `arcl skill install`")]
    SkillDrift { count: usize },
    #[error("could not read `{path}`: {source}")]
    ReadWebFile { path: PathBuf, source: io::Error },
    #[error("could not write `{path}`: {source}")]
    WriteWebFile { path: PathBuf, source: io::Error },
    #[error("could not export `{document}`: {message}")]
    WebExport { document: String, message: String },
    #[error("the queued edits are not valid: {message}")]
    InvalidQueue { message: String },
    #[error("the workspace did not sync, so `web export` wrote nothing: {0}")]
    ExportSync(Box<CommandError>),
}

impl From<WorkspaceExportError> for CommandError {
    fn from(error: WorkspaceExportError) -> Self {
        Self::WorkspaceExport(Box::new(error))
    }
}

impl From<WorkspaceImportError> for CommandError {
    fn from(error: WorkspaceImportError) -> Self {
        Self::WorkspaceImport(Box::new(error))
    }
}

impl From<RecordEditError> for CommandError {
    fn from(error: RecordEditError) -> Self {
        Self::RecordEdit(Box::new(error))
    }
}

impl From<WorkspaceStatusError> for CommandError {
    fn from(error: WorkspaceStatusError) -> Self {
        Self::WorkspaceStatus(Box::new(error))
    }
}

impl From<WorkspaceSyncError> for CommandError {
    fn from(error: WorkspaceSyncError) -> Self {
        Self::WorkspaceSync(Box::new(error))
    }
}

impl CommandError {
    fn exit_code(&self) -> u8 {
        match self {
            Self::Vcs(error) => error.exit_code(),
            Self::NotInitialized { .. } | Self::InvalidConfig { .. } | Self::WorkspaceDisabled | Self::Domain(_) => 3,
            Self::WorkspaceExport(error) => match error.as_ref() {
                WorkspaceExportError::Conflict { .. } => 4,
                _ => 1,
            },
            Self::WorkspaceImport(error) => error.exit_code(),
            Self::WorkspaceSync(error) => error.exit_code(),
            Self::WorkspaceStatus(_) => 1,
            Self::RecordEdit(error) => error.exit_code(),
            Self::AutoSync(error) => error.exit_code(),
            Self::ExportSync(error) => error.exit_code(),
            Self::ReadConfig { .. }
            | Self::ReadMarkdown { .. }
            | Self::ReadStdin { .. }
            | Self::OpenDatabase { source: StorageError::Sqlite(_), .. }
            | Self::CurrentDirectory { .. }
            | Self::SyncLock { .. }
            | Self::SyncLockBusy { .. }
            | Self::ReadSkill { .. }
            | Self::WriteSkill { .. }
            | Self::ReadWebFile { .. }
            | Self::WriteWebFile { .. }
            | Self::WebExport { .. } => 1,
            Self::SkillDrift { .. } => 4,
            Self::OpenDatabase { source, .. } => source.exit_code(),
            Self::Storage(error) => error.exit_code(),
            Self::InvalidMarkdown { .. }
            | Self::InvalidStdin
            | Self::InvalidFilter { .. }
            | Self::Integrity { .. }
            | Self::StdinIsTerminal
            | Self::SkillSymlink { .. }
            | Self::SkillNotFile { .. }
            | Self::SkillNotDirectory { .. }
            | Self::SkillOutsideProject { .. }
            | Self::InvalidQueue { .. } => 3,
        }
    }
}
