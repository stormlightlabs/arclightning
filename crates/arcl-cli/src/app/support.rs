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

pub(super) fn parse_id<T>(value: &str, parser: impl Fn(&str) -> Result<T, IdError>) -> CResult<T> {
    parser(value).map_err(|error| CommandError::Domain(DomainError::InvalidId(error)))
}

pub(super) fn parse_optional_id<T>(
    value: Option<String>, parser: impl Fn(&str) -> Result<T, IdError>,
) -> CResult<Option<T>> {
    value.as_deref().map(|value| parse_id(value, &parser)).transpose()
}

pub(super) fn relation_change<T>(
    value: Option<String>, clear: bool, parser: impl Fn(&str) -> Result<T, IdError>,
) -> CResult<Option<Option<T>>> {
    if clear { Ok(Some(None)) } else { parse_optional_id(value, parser).map(|value| value.map(Some)) }
}

pub(super) fn parse_capture_id(value: &str) -> CResult<CaptureId> {
    parse_id(value, CaptureId::parse)
}
pub(super) fn parse_release_id(value: &str) -> CResult<ReleaseId> {
    parse_id(value, ReleaseId::parse)
}
pub(super) fn parse_spec_id(value: &str) -> CResult<SpecId> {
    parse_id(value, SpecId::parse)
}
pub(super) fn parse_plan_id(value: &str) -> CResult<PlanId> {
    parse_id(value, PlanId::parse)
}
pub(super) fn parse_phase_id(value: &str) -> CResult<PhaseId> {
    parse_id(value, PhaseId::parse)
}
pub(super) fn parse_task_id(value: &str) -> CResult<TaskId> {
    parse_id(value, TaskId::parse)
}
pub(super) fn parse_note_id(value: &str) -> CResult<NoteId> {
    parse_id(value, NoteId::parse)
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
    arcl_core::plan::parse(&input).map_err(|error| CommandError::Storage(StorageError::InvalidPlanInput(error)))
}

pub(super) fn open_database() -> CResult<Database> {
    let (root, _) = project_root_config()?;
    let database_path = root.join(ARCL_DIRECTORY).join(DATABASE_FILE);
    if !database_path.is_file() {
        return Err(CommandError::NotInitialized { root });
    }
    Database::open(&database_path).map_err(|source| CommandError::OpenDatabase { path: database_path, source })
}

pub(super) fn project_root_config() -> CResult<(PathBuf, ProjectConfig)> {
    let start = std::env::current_dir().map_err(|source| CommandError::CurrentDirectory { source })?;
    let Some(root) = nearest_project_root(&start) else {
        return Err(CommandError::NotInitialized { root: start });
    };
    let config_path = root.join(ARCL_DIRECTORY).join(CONFIG_FILE);
    let input = fs::read_to_string(&config_path)
        .map_err(|source| CommandError::ReadConfig { path: config_path.clone(), source })?;
    let config =
        ProjectConfig::parse(&input).map_err(|source| CommandError::InvalidConfig { path: config_path, source })?;
    Ok((root, config))
}

pub(super) fn snapshot_paths() -> CResult<(PathBuf, PathBuf)> {
    let (root, config) = project_root_config()?;
    if !config.snapshot.enabled {
        return Err(CommandError::SnapshotDisabled);
    }
    let snapshot_root = resolve_snapshot_root(&root, &config.snapshot.path)
        .map_err(|source| CommandError::InvalidConfig { path: root.join(ARCL_DIRECTORY).join(CONFIG_FILE), source })?;
    Ok((root, snapshot_root))
}

pub(super) fn nearest_project_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|candidate| candidate.join(ARCL_DIRECTORY).join(CONFIG_FILE).is_file())
        .map(Path::to_owned)
}
