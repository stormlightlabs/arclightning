use super::*;

pub(super) type CResult<T> = Result<T, CommandError>;
pub(super) type IResult<T> = Result<T, InitError>;

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

    pub(super) fn from_init(error: InitError) -> Self {
        let exit_code = error.exit_code();
        let message = if exit_code == 3 { format!("invalid project: {error}") } else { error.to_string() };
        Self { message, exit_code }
    }

    pub(super) fn from_command<E>(error: E) -> Self
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
pub(super) enum InitError {
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
    InvalidConfig { path: PathBuf, source: SnapshotError },
    #[error("could not open Arc Lightning database `{path}`: {source}")]
    OpenDatabase { path: PathBuf, source: StorageError },
    #[error("snapshot support is disabled; run `arcl init --snapshot` first")]
    SnapshotDisabled,
    #[error(transparent)]
    SnapshotExport(Box<SnapshotExportError>),
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

impl From<SnapshotExportError> for CommandError {
    fn from(error: SnapshotExportError) -> Self {
        Self::SnapshotExport(Box::new(error))
    }
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
            Self::NotInitialized { .. } | Self::InvalidConfig { .. } | Self::SnapshotDisabled | Self::Domain(_) => 3,
            Self::SnapshotExport(error) => match error.as_ref() {
                SnapshotExportError::Conflict { .. } => 4,
                _ => 1,
            },
            Self::SnapshotImport(error) => error.exit_code(),
            Self::ReadConfig { .. }
            | Self::ReadMarkdown { .. }
            | Self::ReadStdin { .. }
            | Self::OpenDatabase { source: StorageError::Sqlite(_), .. }
            | Self::CurrentDirectory { .. } => 1,
            Self::OpenDatabase { source, .. } => source.storage_exit_code(),
            Self::Storage(error) => error.storage_exit_code(),
            Self::InvalidMarkdown { .. }
            | Self::InvalidStdin
            | Self::InvalidFilter { .. }
            | Self::Integrity { .. }
            | Self::StdinIsTerminal => 3,
        }
    }
}

trait StorageErrorExitCode {
    fn storage_exit_code(&self) -> u8;
}

impl StorageErrorExitCode for StorageError {
    fn storage_exit_code(&self) -> u8 {
        match self {
            StorageError::ProjectNotFound => 3,
            StorageError::ReleaseNotFound { .. }
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
            | StorageError::InvalidRelease(_)
            | StorageError::CaptureNotPromotable { .. }
            | StorageError::AmbiguousCapturePromotion { .. }
            | StorageError::InconsistentCapturePromotion { .. }
            | StorageError::InvalidPlanInput(_)
            | StorageError::NewerDatabase { .. }
            | StorageError::MigrationGap { .. } => 3,
            StorageError::Sqlite(_) => 1,
        }
    }
}
