//! Find a project, open its database, and run the workspace sync that the CLI
//! and the MCP server run around an operation.
//!
//! With the workspace on, an operation takes the project's sync lock and
//! syncs first ([`sync_before`]). A write that cannot sync refuses before it
//! changes anything ([`ProjectError::before_write`]); a read warns and reads
//! the database as it is ([`Warning::unsynced_read`]). After a write, a
//! second sync exports it ([`LockedWorkspace::export_after_write`]), and a
//! failure there is a warning, since the change is saved.
//!
//! The lock file is `.arcl/arcl.db-sync.lock`, so `arcl` commands and the MCP
//! server in one project sync one at a time. Each adapter decides what a
//! held lock or a failed sync means for its own calls.

use std::fmt::Display;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use thiserror::Error;

use arcl_store::prelude::*;

use crate::vcs::{GixVcs, Vcs, VcsError};
use crate::workspace::{
    ProjectConfig, WorkspaceError, WorkspaceSyncError, importable_workspace, not_initialized_message,
    resolve_workspace_root, sync_workspace,
};

/// The project directory under the project root.
pub const ARCL_DIRECTORY: &str = ".arcl";
/// The project configuration file under [`ARCL_DIRECTORY`].
pub const CONFIG_FILE: &str = "config.toml";
/// The database file under [`ARCL_DIRECTORY`].
pub const DATABASE_FILE: &str = "arcl.db";
/// The lock file under [`ARCL_DIRECTORY`] that serializes syncs in one
/// project. The `/arcl.db-*` entry in `.arcl/.gitignore` keeps it out of Git.
pub const SYNC_LOCK_FILE: &str = "arcl.db-sync.lock";
/// The warning code for a sync or export that did not run while the
/// operation went on.
pub const WORKSPACE_SYNC_WARNING: &str = "workspace_sync";
/// The warning code for a blocker rule violation a workspace import loaded
/// because it did not introduce it.
pub const BLOCKER_EDGE_WARNING: &str = "blocker_edge";

/// How often a bounded lock wait tries the lock again and checks for
/// cancellation.
const LOCK_POLL: Duration = Duration::from_millis(25);

/// A failure to find, open or sync a project.
#[derive(Debug, Error)]
pub enum ProjectError {
    /// No project or no database. `workspace` is the workspace directory when
    /// the project has one that the next command after `arcl init` imports.
    #[error("{}", not_initialized_message(root, workspace.as_deref()))]
    NotInitialized { root: PathBuf, workspace: Option<PathBuf> },
    /// The current directory, where the search for a project starts, could
    /// not be read.
    #[error("could not determine the current directory: {source}")]
    CurrentDirectory { source: io::Error },
    /// The project configuration could not be read.
    #[error("could not read project configuration `{path}`: {source}")]
    ReadConfig { path: PathBuf, source: io::Error },
    /// The project configuration, or the workspace path it names, is invalid.
    #[error("project configuration `{path}` is invalid: {source}")]
    InvalidConfig { path: PathBuf, source: Box<WorkspaceError> },
    /// The database file could not be checked.
    #[error("could not open Arc Lightning database `{path}`: {source}")]
    ProbeDatabase { path: PathBuf, source: io::Error },
    /// The database could not be opened, including one a newer arcl wrote.
    #[error("could not open Arc Lightning database `{path}`: {source}")]
    OpenDatabase { path: PathBuf, source: Box<StorageError> },
    /// The operation needs the workspace and the project has it off.
    #[error("workspace support is disabled; run `arcl init --workspace` first")]
    WorkspaceDisabled,
    /// The sync lock file could not be opened or locked.
    #[error("could not lock `{path}`: {source}")]
    SyncLock { path: PathBuf, source: io::Error },
    /// Another process held the sync lock past a bounded wait.
    #[error(
        "another `arcl` command held the sync lock `{path}` for more than {} s; try again when it finishes",
        waited.as_secs_f64()
    )]
    SyncLockBusy { path: PathBuf, waited: Duration },
    /// The caller cancelled the operation before it started.
    #[error("the call was cancelled before it changed anything")]
    Cancelled,
    /// The Git worktree could not be read.
    #[error(transparent)]
    Vcs(#[from] VcsError),
    /// The sync stopped.
    #[error(transparent)]
    Sync(Box<WorkspaceSyncError>),
    /// The sync before a write stopped, so the write did not run.
    #[error(
        "the workspace must sync before this change, and the sync stopped: {0}\nRun `arcl workspace status` to see why{hint}",
        hint = .0.resolve_hint()
    )]
    AutoSync(Box<WorkspaceSyncError>),
}

impl ProjectError {
    /// The CLI exit status for this failure.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::NotInitialized { .. } | Self::InvalidConfig { .. } | Self::WorkspaceDisabled => 3,
            Self::CurrentDirectory { .. }
            | Self::ReadConfig { .. }
            | Self::ProbeDatabase { .. }
            | Self::SyncLock { .. }
            | Self::SyncLockBusy { .. }
            | Self::Cancelled => 1,
            Self::OpenDatabase { source, .. } => source.exit_code(),
            Self::Vcs(error) => error.exit_code(),
            Self::Sync(error) | Self::AutoSync(error) => error.exit_code(),
        }
    }

    /// The error a write returns when the sync before it fails. A sync that
    /// stopped says the write did not run, except when the database changed
    /// while it read, which keeps its own error since running again settles
    /// it. Any other failure is returned as it is.
    pub fn before_write(self) -> Self {
        match self {
            Self::Sync(error) if !matches!(*error, WorkspaceSyncError::Storage(StorageError::ChangedSinceRead)) => {
                Self::AutoSync(error)
            }
            other => other,
        }
    }

    /// Whether an operation cannot go on even as a read: the lock wait ran
    /// out or the caller cancelled.
    pub const fn stops_reads(&self) -> bool {
        matches!(self, Self::SyncLockBusy { .. } | Self::Cancelled)
    }
}

impl From<WorkspaceSyncError> for ProjectError {
    fn from(error: WorkspaceSyncError) -> Self {
        Self::Sync(Box::new(error))
    }
}

/// How long to wait for the sync lock.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LockWait {
    /// Wait as long as it takes, as an `arcl` command does.
    Unbounded,
    /// Wait at most this long, then fail with [`ProjectError::SyncLockBusy`].
    Within(Duration),
}

/// A project: its root, the nearest directory holding `.arcl/config.toml`,
/// and its configuration.
#[derive(Clone, Debug)]
pub struct ProjectDir {
    root: PathBuf,
    config: ProjectConfig,
}

impl ProjectDir {
    /// Find the project that holds `start` and read its configuration. With
    /// no project, fails with [`ProjectError::NotInitialized`] naming `start`.
    pub fn discover(start: &Path) -> Result<Self, ProjectError> {
        let Some(root) =
            nearest_project_root(start).map_err(|(path, source)| ProjectError::ReadConfig { path, source })?
        else {
            return Err(ProjectError::NotInitialized { root: start.to_owned(), workspace: None });
        };
        let config_path = root.join(ARCL_DIRECTORY).join(CONFIG_FILE);
        let input = fs::read_to_string(&config_path)
            .map_err(|source| ProjectError::ReadConfig { path: config_path.clone(), source })?;
        let config = ProjectConfig::parse(&input)
            .map_err(|source| ProjectError::InvalidConfig { path: config_path, source: Box::new(source) })?;
        Ok(Self { root, config })
    }

    /// Find the project that holds the current directory. See
    /// [`ProjectDir::discover`].
    pub fn discover_current() -> Result<Self, ProjectError> {
        let start = std::env::current_dir().map_err(|source| ProjectError::CurrentDirectory { source })?;
        Self::discover(&start)
    }

    /// The project root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The project configuration.
    pub fn config(&self) -> &ProjectConfig {
        &self.config
    }

    /// The path of the project's database file.
    pub fn database_path(&self) -> PathBuf {
        self.root.join(ARCL_DIRECTORY).join(DATABASE_FILE)
    }

    /// Check that the database file exists and return its path. A missing
    /// database fails with [`ProjectError::NotInitialized`], naming the
    /// workspace `arcl init` would import when there is one.
    pub fn require_database(&self) -> Result<PathBuf, ProjectError> {
        let path = self.database_path();
        match file_present(fs::metadata(&path)) {
            Ok(true) => Ok(path),
            Ok(false) => Err(ProjectError::NotInitialized {
                root: self.root.clone(),
                workspace: importable_workspace(&self.root, &self.config),
            }),
            Err(source) => Err(ProjectError::ProbeDatabase { path, source }),
        }
    }

    /// Open the project's database. See [`ProjectDir::require_database`].
    pub fn open_database(&self) -> Result<Database, ProjectError> {
        let path = self.require_database()?;
        Database::open(&path).map_err(|source| ProjectError::OpenDatabase { path, source: Box::new(source) })
    }

    /// The project's workspace, or `None` when it is off. A workspace path
    /// that does not resolve, such as one through a symbolic link, fails with
    /// [`ProjectError::InvalidConfig`].
    pub fn workspace(&self) -> Result<Option<Workspace>, ProjectError> {
        if !self.config.workspace.enabled {
            return Ok(None);
        }
        let workspace_root = resolve_workspace_root(&self.root, &self.config.workspace.path).map_err(|source| {
            ProjectError::InvalidConfig {
                path: self.root.join(ARCL_DIRECTORY).join(CONFIG_FILE),
                source: Box::new(source),
            }
        })?;
        Ok(Some(Workspace { project: self.clone(), workspace_root }))
    }

    /// The project's workspace, failing with
    /// [`ProjectError::WorkspaceDisabled`] when it is off.
    pub fn require_workspace(&self) -> Result<Workspace, ProjectError> {
        self.workspace()?.ok_or(ProjectError::WorkspaceDisabled)
    }
}

/// A project with the workspace on.
#[derive(Clone, Debug)]
pub struct Workspace {
    project: ProjectDir,
    workspace_root: PathBuf,
}

impl Workspace {
    /// The project the workspace belongs to.
    pub fn project(&self) -> &ProjectDir {
        &self.project
    }

    /// The project root, which is the Git worktree root the sync compares
    /// against.
    pub fn worktree_root(&self) -> &Path {
        self.project.root()
    }

    /// The workspace directory.
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Take the project's sync lock, creating the lock file if it is
    /// missing. [`LockWait::Unbounded`] blocks until no other process holds
    /// it. [`LockWait::Within`] tries again every 25 ms and fails with
    /// [`ProjectError::SyncLockBusy`] once the wait passes, or with
    /// [`ProjectError::Cancelled`] once `cancelled` returns `true`. The lock
    /// lasts until the returned [`SyncLock`] drops.
    pub fn lock(&self, wait: LockWait, cancelled: &dyn Fn() -> bool) -> Result<SyncLock, ProjectError> {
        let path = self.worktree_root().join(ARCL_DIRECTORY).join(SYNC_LOCK_FILE);
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|source| ProjectError::SyncLock { path: path.clone(), source })?;
        let wait = match wait {
            LockWait::Unbounded => {
                file.lock().map_err(|source| ProjectError::SyncLock { path, source })?;
                return Ok(SyncLock { _file: file });
            }
            LockWait::Within(wait) => wait,
        };
        let deadline = Instant::now() + wait;
        loop {
            if cancelled() {
                return Err(ProjectError::Cancelled);
            }
            match file.try_lock() {
                Ok(()) => return Ok(SyncLock { _file: file }),
                Err(TryLockError::Error(source)) => return Err(ProjectError::SyncLock { path, source }),
                Err(TryLockError::WouldBlock) if Instant::now() >= deadline => {
                    return Err(ProjectError::SyncLockBusy { path, waited: wait });
                }
                Err(TryLockError::WouldBlock) => thread::sleep(LOCK_POLL),
            }
        }
    }

    /// Sync the database and the workspace once, and return the blocker rule
    /// warnings the sync gave. Call it under the sync lock.
    pub fn sync(&self) -> Result<Vec<String>, ProjectError> {
        let mut database = self.project.open_database()?;
        let vcs = discover_vcs(self.worktree_root())?;
        let outcome = sync_workspace(&self.workspace_root, self.worktree_root(), &mut database, as_dyn(&vcs))?;
        Ok(outcome.warnings)
    }
}

/// A held sync lock. Dropping it releases the lock.
#[derive(Debug)]
pub struct SyncLock {
    _file: File,
}

/// A workspace whose sync lock this process holds.
#[derive(Debug)]
pub struct LockedWorkspace {
    workspace: Workspace,
    _lock: SyncLock,
}

impl LockedWorkspace {
    /// The workspace.
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// Sync after a committed write to export it, adding a warning when the
    /// export fails, since the change is saved either way.
    pub fn export_after_write(&self, warnings: &mut Vec<Warning>) {
        match self.workspace.sync() {
            Ok(found) => warnings.extend(Warning::blocker_edges(found)),
            Err(error) => warnings.push(Warning::unexported_write(&error)),
        }
    }
}

/// What [`sync_before`] did: the workspace whose lock it holds, if it took
/// the lock, and the sync's blocker rule warnings or the error that stopped
/// it.
#[derive(Debug)]
pub struct SyncBefore {
    /// The workspace and its lock, held until this drops. `None` when the
    /// workspace is off or the lock was not taken.
    pub locked: Option<LockedWorkspace>,
    /// The sync's blocker rule warnings, or why it did not run. A workspace
    /// that is off gives no warnings.
    pub result: Result<Vec<String>, ProjectError>,
}

/// A condition an operation went on despite. `code` is
/// `workspace_sync` for a sync that did not run, or `blocker_edge` for a
/// blocker rule violation a workspace import loaded, as in the CLI's warnings
/// envelope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Warning {
    /// The kind of warning.
    pub code: &'static str,
    /// What happened.
    pub message: String,
}

impl Warning {
    /// A warning of kind `code`.
    pub const fn new(code: &'static str, message: String) -> Self {
        Self { code, message }
    }

    /// A sync or export that did not run, while the operation went on.
    pub const fn workspace_sync(message: String) -> Self {
        Self::new(WORKSPACE_SYNC_WARNING, message)
    }

    /// A read that goes on with the database as it is because the sync
    /// before it failed with `error`.
    pub fn unsynced_read(error: &dyn Display) -> Self {
        Self::workspace_sync(format!(
            "the workspace did not sync, so this shows the database as it is: {error}"
        ))
    }

    /// A committed write whose export failed with `error`.
    pub fn unexported_write(error: &dyn Display) -> Self {
        Self::workspace_sync(format!(
            "the change is saved, but the workspace was not updated: {error}; the next command updates it"
        ))
    }

    /// The blocker rule violations a workspace import loaded because it did
    /// not introduce them.
    pub fn blocker_edges(messages: Vec<String>) -> impl Iterator<Item = Self> {
        messages
            .into_iter()
            .map(|message| Self::new(BLOCKER_EDGE_WARNING, message))
    }
}

/// Take the sync lock of `workspace` and sync once, before an operation.
///
/// `workspace` is what [`ProjectDir::workspace`] returned, or the error that
/// kept the caller from finding it. `Ok(None)` syncs nothing. The lock stays
/// held in [`SyncBefore::locked`] whether or not the sync succeeds, so the
/// operation and the export after it run under it.
pub fn sync_before(
    workspace: Result<Option<Workspace>, ProjectError>, wait: LockWait, cancelled: &dyn Fn() -> bool,
) -> SyncBefore {
    let workspace = match workspace {
        Ok(Some(workspace)) => workspace,
        Ok(None) => return SyncBefore { locked: None, result: Ok(Vec::new()) },
        Err(error) => return SyncBefore { locked: None, result: Err(error) },
    };
    match workspace.lock(wait, cancelled) {
        Ok(lock) => {
            let result = workspace.sync();
            SyncBefore { locked: Some(LockedWorkspace { workspace, _lock: lock }), result }
        }
        Err(error) => SyncBefore { locked: None, result: Err(error) },
    }
}

/// The nearest of `start` and its ancestors with a `.arcl/config.toml` file,
/// or `None` when there is none. A config path that cannot be checked for a
/// reason other than its absence, such as a `.arcl` directory without read
/// permission, fails with that path and error.
pub fn nearest_project_root(start: &Path) -> Result<Option<PathBuf>, (PathBuf, io::Error)> {
    for candidate in start.ancestors() {
        let config_path = candidate.join(ARCL_DIRECTORY).join(CONFIG_FILE);
        match file_present(fs::metadata(&config_path)) {
            Ok(true) => return Ok(Some(candidate.to_owned())),
            Ok(false) => {}
            Err(source) => return Err((config_path, source)),
        }
    }
    Ok(None)
}

/// Open the Git worktree at `worktree_root`, or return `None` when the
/// project is not in one.
pub fn discover_vcs(worktree_root: &Path) -> Result<Option<GixVcs>, VcsError> {
    match GixVcs::discover(worktree_root) {
        Ok(vcs) => Ok(Some(vcs)),
        Err(VcsError::Discovery { .. }) => Ok(None),
        Err(error) => Err(error),
    }
}

/// Borrow a discovered worktree as the `Vcs` trait object the workspace
/// functions take.
pub fn as_dyn(vcs: &Option<GixVcs>) -> Option<&dyn Vcs> {
    vcs.as_ref().map(|vcs| vcs as &dyn Vcs)
}

/// Whether the path `metadata` was read from is a file. Only a missing path,
/// or one under a non-directory, counts as absent; any other error, such as
/// a permission error, is returned.
fn file_present(metadata: io::Result<fs::Metadata>) -> io::Result<bool> {
    match metadata {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if matches!(error.kind(), io::ErrorKind::NotFound | io::ErrorKind::NotADirectory) => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A project in a temporary directory with a database, and with the
    /// workspace exported when `workspace` is set.
    fn project(workspace: bool) -> tempfile::TempDir {
        let directory = tempfile::tempdir().expect("temporary directory can be created");
        let arcl = directory.path().join(ARCL_DIRECTORY);
        fs::create_dir_all(&arcl).expect("project directory is created");
        let mut config = ProjectConfig::default();
        config.workspace.enabled = workspace;
        fs::write(arcl.join(CONFIG_FILE), config.render().expect("config renders")).expect("config is written");
        Database::open(arcl.join(DATABASE_FILE)).expect("database opens");
        if workspace {
            let workspace_root = directory.path().join(&config.workspace.path);
            fs::create_dir_all(&workspace_root).expect("workspace is created");
            fs::write(workspace_root.join("manifest.toml"), "format-version = 1\n").expect("manifest is written");
        }
        directory
    }

    fn never() -> bool {
        false
    }

    #[test]
    fn file_present_treats_only_a_missing_path_as_absent() {
        let missing = |kind: io::ErrorKind| file_present(Err(io::Error::from(kind)));
        assert!(!missing(io::ErrorKind::NotFound).expect("a missing path is absent"));
        assert!(!missing(io::ErrorKind::NotADirectory).expect("a path under a file is absent"));
        let denied = missing(io::ErrorKind::PermissionDenied).expect_err("a permission error fails");
        assert_eq!(denied.kind(), io::ErrorKind::PermissionDenied);

        let directory = tempfile::tempdir().expect("temporary directory can be created");
        let file = directory.path().join("config.toml");
        fs::write(&file, "").expect("file is written");
        assert!(file_present(fs::metadata(&file)).expect("a file is present"));
        assert!(!file_present(fs::metadata(directory.path())).expect("a directory is not a file"));
    }

    #[test]
    fn discover_finds_the_root_from_a_subdirectory() {
        let directory = project(false);
        let nested = directory.path().join("src/deep");
        fs::create_dir_all(&nested).expect("subdirectory is created");

        let project = ProjectDir::discover(&nested).expect("the project is found");
        assert_eq!(project.root(), directory.path());
        assert_eq!(project.database_path(), directory.path().join(".arcl/arcl.db"));
        project.open_database().expect("the database opens");
        assert!(project.workspace().expect("workspace").is_none());
        assert!(matches!(
            project.require_workspace(),
            Err(ProjectError::WorkspaceDisabled)
        ));
    }

    #[test]
    fn a_missing_project_or_database_is_not_initialized() {
        let directory = tempfile::tempdir().expect("temporary directory can be created");
        let error = ProjectDir::discover(directory.path()).expect_err("no project");
        assert!(matches!(&error, ProjectError::NotInitialized { root, workspace: None } if root == directory.path()));
        assert_eq!(error.exit_code(), 3);

        let directory = project(true);
        fs::remove_file(directory.path().join(".arcl/arcl.db")).expect("database is removed");
        let error = ProjectDir::discover(directory.path())
            .expect("the project is found")
            .open_database()
            .expect_err("no database");
        let ProjectError::NotInitialized { workspace, .. } = &error else { panic!("{error}") };
        assert_eq!(
            workspace.as_deref(),
            Some(directory.path().join(".arcl/workspace").as_path())
        );
        assert!(error.to_string().contains("the next command imports"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn a_config_path_that_cannot_be_checked_fails() {
        let directory = tempfile::tempdir().expect("temporary directory can be created");
        let arcl = directory.path().join(ARCL_DIRECTORY);
        fs::create_dir_all(&arcl).expect("project directory is created");
        // A link to itself fails to resolve with an error other than absence.
        std::os::unix::fs::symlink(arcl.join(CONFIG_FILE), arcl.join(CONFIG_FILE)).expect("symlink");

        let error = ProjectDir::discover(directory.path()).expect_err("the config cannot be checked");
        assert!(matches!(error, ProjectError::ReadConfig { .. }), "{error}");
        assert_eq!(error.exit_code(), 1);
    }

    #[test]
    fn a_held_sync_lock_fails_a_bounded_wait() {
        let directory = project(true);
        let workspace = ProjectDir::discover(directory.path())
            .expect("project")
            .require_workspace()
            .expect("workspace");
        let held = workspace.lock(LockWait::Unbounded, &never).expect("the lock is taken");

        let synced = sync_before(
            Ok(Some(workspace.clone())),
            LockWait::Within(Duration::from_millis(50)),
            &never,
        );
        assert!(synced.locked.is_none());
        let error = synced.result.expect_err("the lock is busy");
        assert!(matches!(error, ProjectError::SyncLockBusy { .. }), "{error}");
        assert!(error.stops_reads());
        assert_eq!(error.exit_code(), 1);
        assert!(error.to_string().contains(SYNC_LOCK_FILE), "{error}");

        let cancelled = workspace
            .lock(LockWait::Within(Duration::from_secs(5)), &|| true)
            .expect_err("a cancelled wait stops");
        assert!(matches!(cancelled, ProjectError::Cancelled));

        drop(held);
        let synced = sync_before(Ok(Some(workspace)), LockWait::Within(Duration::from_millis(50)), &never);
        assert!(synced.locked.is_some());
        assert_eq!(synced.result.expect("the sync runs"), Vec::<String>::new());
    }

    #[test]
    fn a_failed_sync_refuses_writes_and_warns_reads() {
        let directory = project(true);
        fs::write(directory.path().join(".arcl/workspace/manifest.toml"), "not = [valid").expect("break the manifest");
        let workspace = ProjectDir::discover(directory.path()).expect("project").workspace();

        let synced = sync_before(workspace, LockWait::Unbounded, &never);
        let locked = synced.locked.expect("the lock is held after a failed sync");
        let error = synced.result.expect_err("the sync stops");
        assert!(matches!(error, ProjectError::Sync(_)), "{error}");
        assert!(!error.stops_reads());

        let warning = Warning::unsynced_read(&error);
        assert_eq!(warning.code, WORKSPACE_SYNC_WARNING);
        assert!(
            warning
                .message
                .starts_with("the workspace did not sync, so this shows the database as it is: ")
        );

        let mut warnings = Vec::new();
        locked.export_after_write(&mut warnings);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].code, WORKSPACE_SYNC_WARNING);
        assert!(
            warnings[0]
                .message
                .starts_with("the change is saved, but the workspace was not updated: ")
        );

        let refused = error.before_write();
        assert!(matches!(refused, ProjectError::AutoSync(_)), "{refused}");
        assert!(
            refused
                .to_string()
                .contains("the workspace must sync before this change, and the sync stopped: "),
            "{refused}"
        );
        assert!(
            refused.to_string().contains("\nRun `arcl workspace status` to see why"),
            "{refused}"
        );
    }

    #[test]
    fn a_changed_database_keeps_its_own_error_before_a_write() {
        let error = ProjectError::from(WorkspaceSyncError::Storage(StorageError::ChangedSinceRead)).before_write();
        assert!(matches!(error, ProjectError::Sync(_)), "{error}");
        let lock = ProjectError::SyncLockBusy { path: PathBuf::from("lock"), waited: Duration::from_secs(1) };
        assert!(matches!(lock.before_write(), ProjectError::SyncLockBusy { .. }));
    }

    #[test]
    fn an_invalid_workspace_path_is_a_config_error() {
        let directory = project(false);
        let config = "format-version = 1\n\n[workspace]\nenabled = true\npath = \"../outside\"\n";
        fs::write(directory.path().join(".arcl/config.toml"), config).expect("config is written");
        let error = ProjectDir::discover(directory.path()).expect_err("the config is invalid");
        assert!(matches!(error, ProjectError::InvalidConfig { .. }), "{error}");
        assert_eq!(error.exit_code(), 3);
    }
}
