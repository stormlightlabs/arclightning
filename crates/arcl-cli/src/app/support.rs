use std::cell::RefCell;
use std::rc::Rc;

use super::*;

pub(super) fn write_output(message: Option<String>) -> anyhow::Result<()> {
    if let Some(message) = message {
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        writeln!(stdout, "{message}").context("writing CLI output")?;
    }
    Ok(())
}

pub(super) fn parse_member_kind(value: &str) -> CResult<ReleaseMemberKind> {
    match value {
        "spec" => Ok(ReleaseMemberKind::Spec),
        "plan" => Ok(ReleaseMemberKind::Plan),
        "task" => Ok(ReleaseMemberKind::Task),
        "note" => Ok(ReleaseMemberKind::Note),
        _ => Err(CommandError::InvalidFilter { message: format!("unknown release member kind `{value}`") }),
    }
}

pub(super) fn parse_linked_kind(value: &str) -> CResult<LinkedRecordKind> {
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

thread_local! {
    /// The records short ID arguments resolve against: `None` outside a
    /// [`RecordScope`], and read on first use inside one.
    static RECORDS: RefCell<Option<Option<Rc<[RecordRef]>>>> = const { RefCell::new(None) };
}

/// One command's scope, inside which every short ID argument resolves against
/// a single read of the records. Dropping it forgets them, so the next command
/// reads them again.
pub(super) struct RecordScope {
    previous: Option<Option<Rc<[RecordRef]>>>,
}

impl RecordScope {
    /// Start a scope. Scopes nest; dropping one restores the outer scope.
    pub(super) fn enter() -> Self {
        Self { previous: RECORDS.with(|records| records.replace(Some(None))) }
    }
}

impl Drop for RecordScope {
    fn drop(&mut self) {
        let previous = self.previous.take();
        RECORDS.with(|records| records.replace(previous));
    }
}

/// Every record's full ID and title: read once per [`RecordScope`], or on
/// each call outside one.
fn record_refs() -> CResult<Rc<[RecordRef]>> {
    let cached = RECORDS.with(|records| records.borrow().clone());
    if let Some(Some(records)) = cached {
        return Ok(records);
    }
    let records: Rc<[RecordRef]> = open_database()?.record_refs()?.into();
    if cached.is_some() {
        RECORDS.with(|scoped| scoped.replace(Some(Some(Rc::clone(&records)))));
    }
    Ok(records)
}

/// The full record ID `value` names. A full ID comes back as it is; a short
/// ID, such as `535B` or `t-535B`, must end one record's ID. `expected`, a
/// full-ID prefix such as `arcl-t-`, narrows a short ID that names no kind.
pub(super) fn resolve_record_id(value: &str, expected: Option<&str>) -> CResult<String> {
    if is_full_id(value) {
        return Ok(value.to_owned());
    }
    let records = record_refs()?;
    Ok(resolve_short_id(value, &records, expected)
        .map_err(StorageError::from)?
        .id
        .clone())
}

/// Parse a record ID argument of kind `T`. A full ID parses as it is; a short
/// ID resolves among records of kind `T` first, unless it names a kind.
pub(super) fn parse_id<T: PrefixedId>(value: &str) -> CResult<T> {
    let value = resolve_record_id(value, Some(T::PREFIX))?;
    T::parse(&value).map_err(|error| CommandError::Domain(DomainError::InvalidId(error)))
}

/// The short IDs human output shows, or none for plain, JSON and quiet
/// output, which keep full IDs.
pub(super) fn display_ids(database: &Database, renderer: &Renderer) -> CResult<ShortIds> {
    match renderer.mode() {
        OutputMode::Human => Ok(database.short_ids()?),
        OutputMode::Plain | OutputMode::Json | OutputMode::Quiet => Ok(ShortIds::default()),
    }
}

/// The full-ID prefix of a release member kind.
pub(super) const fn member_prefix(kind: ReleaseMemberKind) -> &'static str {
    match kind {
        ReleaseMemberKind::Spec => SpecId::PREFIX,
        ReleaseMemberKind::Plan => PlanId::PREFIX,
        ReleaseMemberKind::Task => TaskId::PREFIX,
        ReleaseMemberKind::Note => NoteId::PREFIX,
    }
}

/// The full-ID prefix of a record kind a note links to.
pub(super) const fn linked_prefix(kind: LinkedRecordKind) -> &'static str {
    match kind {
        LinkedRecordKind::Capture => CaptureId::PREFIX,
        LinkedRecordKind::Spec => SpecId::PREFIX,
        LinkedRecordKind::Plan => PlanId::PREFIX,
        LinkedRecordKind::Phase => PhaseId::PREFIX,
        LinkedRecordKind::Task => TaskId::PREFIX,
        LinkedRecordKind::Note => NoteId::PREFIX,
        LinkedRecordKind::Release => ReleaseId::PREFIX,
    }
}

pub(super) fn parse_optional_id<T: PrefixedId>(value: Option<String>) -> CResult<Option<T>> {
    value.as_deref().map(parse_id).transpose()
}

pub(super) fn relation_change<T: PrefixedId>(value: Option<String>, clear: bool) -> CResult<Option<Option<T>>> {
    if clear { Ok(Some(None)) } else { parse_optional_id(value).map(|value| value.map(Some)) }
}

pub(super) fn parse_capture_id(value: &str) -> CResult<CaptureId> {
    parse_id(value)
}
pub(super) fn parse_release_id(value: &str) -> CResult<ReleaseId> {
    parse_id(value)
}
pub(super) fn parse_spec_id(value: &str) -> CResult<SpecId> {
    parse_id(value)
}
pub(super) fn parse_plan_id(value: &str) -> CResult<PlanId> {
    parse_id(value)
}
pub(super) fn parse_phase_id(value: &str) -> CResult<PhaseId> {
    parse_id(value)
}
pub(super) fn parse_task_id(value: &str) -> CResult<TaskId> {
    parse_id(value)
}
pub(super) fn parse_note_id(value: &str) -> CResult<NoteId> {
    parse_id(value)
}

pub(super) fn resolve_markdown(args: MarkdownArgs) -> CResult<Option<String>> {
    resolve_optional_value(args.body, args.body_file)
}

pub(super) fn resolve_acceptance(args: AcceptanceArgs) -> CResult<Option<String>> {
    resolve_optional_value(args.acceptance_criteria, args.acceptance_criteria_file)
}

pub(super) fn resolve_optional_value(value: Option<String>, path: Option<PathBuf>) -> CResult<Option<String>> {
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

pub(super) fn read_stdin() -> CResult<Option<String>> {
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

pub(super) fn read_plan_document(path: &Path) -> CResult<arcl_core::plan::PlanDocument> {
    let input = resolve_optional_value(None, Some(path.to_owned()))?.unwrap_or_default();
    parse_plan_document(&input)
}

/// Parse and validate a structured plan from its TOML text.
pub(super) fn parse_plan_document(input: &str) -> CResult<arcl_core::plan::PlanDocument> {
    arcl_core::plan::parse(input).map_err(|error| CommandError::Storage(StorageError::InvalidPlanInput(error)))
}

pub(super) fn open_database() -> CResult<Database> {
    let (root, _) = project_root_config()?;
    let database_path = root.join(ARCL_DIRECTORY).join(DATABASE_FILE);
    match file_present(fs::metadata(&database_path)) {
        Ok(true) => {}
        Ok(false) => return Err(CommandError::NotInitialized { root }),
        Err(source) => return Err(CommandError::ProbeDatabase { path: database_path, source }),
    }
    Database::open(&database_path).map_err(|source| CommandError::OpenDatabase { path: database_path, source })
}

pub(super) fn project_root_config() -> CResult<(PathBuf, ProjectConfig)> {
    let start = std::env::current_dir().map_err(|source| CommandError::CurrentDirectory { source })?;
    let Some(root) =
        nearest_project_root(&start).map_err(|(path, source)| CommandError::ReadConfig { path, source })?
    else {
        return Err(CommandError::NotInitialized { root: start });
    };
    let config_path = root.join(ARCL_DIRECTORY).join(CONFIG_FILE);
    let input = fs::read_to_string(&config_path)
        .map_err(|source| CommandError::ReadConfig { path: config_path.clone(), source })?;
    let config =
        ProjectConfig::parse(&input).map_err(|source| CommandError::InvalidConfig { path: config_path, source })?;
    Ok((root, config))
}

pub(super) fn workspace_paths() -> CResult<(PathBuf, PathBuf)> {
    let (root, config) = project_root_config()?;
    if !config.workspace.enabled {
        return Err(CommandError::WorkspaceDisabled);
    }
    let workspace_root = resolve_workspace_root(&root, &config.workspace.path)
        .map_err(|source| CommandError::InvalidConfig { path: root.join(ARCL_DIRECTORY).join(CONFIG_FILE), source })?;
    Ok((root, workspace_root))
}

/// The nearest of `start` and its ancestors with a `.arcl/config.toml` file,
/// or `None` when there is none. A config path that cannot be checked for a
/// reason other than its absence, such as a `.arcl` directory without read
/// permission, fails with that path and error.
pub(super) fn nearest_project_root(start: &Path) -> Result<Option<PathBuf>, (PathBuf, io::Error)> {
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
}
