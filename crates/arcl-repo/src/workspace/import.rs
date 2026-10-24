use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ffi::OsStr,
    fs, io,
    path::{Component, Path, PathBuf},
};

use thiserror::Error;

use arcl_core::domain::*;
use arcl_store::{ConnectedGraph, DataVersion, Database, StorageError, WorkspaceBaseFile};

use super::{
    CaptureRecord, NoteRecord, PhaseRecord, PlanRecord, ReleaseRecord, SpecRecord, TaskRecord, WorkspaceError,
    WorkspaceFile, WorkspaceRecord, WorkspaceReference, decode_manifest, decode_record, list_paths,
};
use super::{export::export_observed, export::project_graph};
use crate::vcs::{PathState, Vcs, VcsError};

const DEFAULT_PROJECT_ID: &str = "arcl-pj-00000000000000000000000000";
pub(super) const RECORD_DIRECTORIES: &[&str] = &["captures", "releases", "specs", "plans", "phases", "tasks", "notes"];

/// A validation or I/O failure raised while importing a complete workspace.
#[derive(Debug, Error)]
pub enum WorkspaceImportError {
    /// The workspace root or one of its record directories could not be read.
    #[error("could not read workspace directory `{path}`: {source}")]
    ReadDirectory { path: PathBuf, source: io::Error },
    /// A directory entry could not be inspected.
    #[error("could not inspect workspace entry `{path}`: {source}")]
    InspectEntry { path: PathBuf, source: io::Error },
    /// Git reports these workspace files as unmerged.
    #[error(
        "workspace files are unmerged in Git: {}; finish the merge, then run the command again",
        list_paths(paths)
    )]
    Unmerged { paths: Vec<PathBuf> },
    /// Git state could not be read.
    #[error(transparent)]
    Vcs(#[from] VcsError),
    /// The required manifest was not present.
    #[error("workspace manifest `{path}` is missing")]
    MissingManifest { path: PathBuf },
    /// A file or directory is outside the workspace layout.
    #[error("workspace entry `{path}` is not allowed in the workspace layout")]
    UnknownEntry { path: PathBuf },
    /// A known workspace path has the wrong file type.
    #[error("workspace entry `{path}` has the wrong type")]
    WrongEntryType { path: PathBuf },
    /// A workspace file contains bytes that are not UTF-8.
    #[error("workspace file `{path}` is not valid UTF-8")]
    NonUtf8 { path: PathBuf },
    /// The manifest could not be decoded or validated.
    #[error("workspace file `{path}`: field `format-version`: {source}")]
    Manifest { path: PathBuf, source: WorkspaceError },
    /// A record could not be decoded or validated by the workspace codec.
    #[error("workspace file `{path}`: field `front matter`: {source}")]
    Record { path: PathBuf, source: WorkspaceError },
    /// A scalar field violates a complete-graph invariant.
    #[error("workspace file `{path}`: field `{field}`: {message}")]
    Field {
        path: PathBuf,
        field: String,
        message: String,
    },
    /// A relationship violates a complete-graph invariant.
    #[error("workspace file `{path}`: relationship `{relationship}`: {message}")]
    Relationship {
        path: PathBuf,
        relationship: String,
        message: String,
    },
    /// A record ID represented in the stored base was removed or renamed.
    #[error(
        "workspace file `{path}`: record ID `{id}` was removed or renamed; the workspace does not support hard deletion"
    )]
    ExistingRecordRemoved { path: PathBuf, id: String },
    /// Both the database projection and workspace changed since the last base.
    #[error("workspace file `{path}` has divergent database and workspace changes")]
    Conflict { path: PathBuf },
    /// A filesystem export used to canonicalize the candidate failed.
    #[error(transparent)]
    Export(#[from] super::WorkspaceExportError),
    /// The complete replacement could not be committed to SQLite.
    #[error(transparent)]
    Storage(#[from] StorageError),
}

impl WorkspaceImportError {
    /// Return the CLI exit category for this import failure.
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Storage(StorageError::ChangedSinceRead) => 4,
            Self::Storage(_) | Self::Vcs(_) | Self::ReadDirectory { .. } | Self::InspectEntry { .. } => 1,
            Self::Export(super::WorkspaceExportError::Conflict { .. }) => 4,
            Self::Export(_) => 1,
            Self::MissingManifest { .. }
            | Self::UnknownEntry { .. }
            | Self::WrongEntryType { .. }
            | Self::NonUtf8 { .. }
            | Self::Manifest { .. }
            | Self::Record { .. }
            | Self::Field { .. }
            | Self::Relationship { .. }
            | Self::ExistingRecordRemoved { .. } => 3,
            Self::Conflict { .. } | Self::Unmerged { .. } => 4,
        }
    }
}

/// What a workspace import wrote and what it loaded despite the rules.
#[derive(Debug, Default)]
pub struct ImportOutcome {
    /// The canonical workspace files, as written back to disk.
    pub files: Vec<WorkspaceFile>,
    /// One message for each blocker rule violation the import loaded because
    /// it did not introduce it. See [`import_graph`].
    pub warnings: Vec<String>,
}

/// A workspace read from disk: every decoded record and the exact bytes of every file.
pub(super) struct CandidateWorkspace {
    pub(super) records: BTreeMap<PathBuf, WorkspaceRecord>,
    pub(super) files: Vec<WorkspaceFile>,
}

struct PreparedImport {
    graph: ConnectedGraph,
    files: Vec<WorkspaceFile>,
    observed: Vec<WorkspaceFile>,
    warnings: Vec<String>,
}

#[derive(Clone, Copy)]
struct Ancestry {
    spec: Option<SpecId>,
    plan: Option<PlanId>,
    phase: Option<PhaseId>,
}

struct RecordIndex {
    captures: HashMap<CaptureId, (Capture, PathBuf)>,
    specs: HashMap<SpecId, (Spec, PathBuf)>,
    plans: HashMap<PlanId, (Plan, PathBuf)>,
    phases: HashMap<PhaseId, (Phase, PathBuf)>,
    tasks: HashMap<TaskId, (PlanningTask, PathBuf)>,
    notes: HashMap<NoteId, (Note, PathBuf)>,
    releases: HashMap<ReleaseId, (Release, PathBuf)>,
}

impl RecordIndex {
    fn from_records(records: &BTreeMap<PathBuf, WorkspaceRecord>, project_id: ProjectId) -> Self {
        let mut index = Self {
            captures: HashMap::new(),
            specs: HashMap::new(),
            plans: HashMap::new(),
            phases: HashMap::new(),
            tasks: HashMap::new(),
            notes: HashMap::new(),
            releases: HashMap::new(),
        };
        for (path, record) in records {
            match record {
                WorkspaceRecord::Capture(record) => {
                    index
                        .captures
                        .insert(record.id, (capture_from_record(record, project_id), path.clone()));
                }
                WorkspaceRecord::Spec(record) => {
                    index
                        .specs
                        .insert(record.id, (spec_from_record(record, project_id), path.clone()));
                }
                WorkspaceRecord::Plan(record) => {
                    index
                        .plans
                        .insert(record.id, (plan_from_record(record, project_id), path.clone()));
                }
                WorkspaceRecord::Phase(record) => {
                    index
                        .phases
                        .insert(record.id, (phase_from_record(record, project_id), path.clone()));
                }
                WorkspaceRecord::Task(record) => {
                    index
                        .tasks
                        .insert(record.id, (task_from_record(record, project_id), path.clone()));
                }
                WorkspaceRecord::Note(record) => {
                    index
                        .notes
                        .insert(record.id, (note_from_record(record, project_id), path.clone()));
                }
                WorkspaceRecord::Release(record) => {
                    index
                        .releases
                        .insert(record.id, (release_from_record(record), path.clone()));
                }
            }
        }
        index
    }

    fn task_path(&self, id: TaskId) -> PathBuf {
        self.tasks
            .get(&id)
            .map(|(_, path)| path.clone())
            .unwrap_or_else(|| PathBuf::from("tasks/<unknown>.md"))
    }

    fn effective_ancestry(&self, id: TaskId, visiting: &mut HashSet<TaskId>) -> Result<Ancestry> {
        if !visiting.insert(id) {
            return Err(relationship_error(
                self.task_path(id),
                "parent",
                format!("parent cycle includes task `{id}`"),
            ));
        }
        let task = &self.tasks[&id].0;
        let mut result = Ancestry { spec: task.spec_id, plan: task.plan_id, phase: task.phase_id };
        if let Some(phase_id) = result.phase {
            let phase = self
                .phases
                .get(&phase_id)
                .expect("phase references are checked before ancestry");
            if result.plan.is_some_and(|plan| plan != phase.0.plan_id) {
                return Err(relationship_error(
                    self.task_path(id),
                    "plan/phase",
                    "task has contradictory plan and phase ancestry",
                ));
            }
            result.plan = Some(phase.0.plan_id);
        }
        if let Some(plan_id) = result.plan {
            let plan = self
                .plans
                .get(&plan_id)
                .expect("plan references are checked before ancestry");
            if result.spec.is_some_and(|spec| spec != plan.0.spec_id) {
                return Err(relationship_error(
                    self.task_path(id),
                    "spec/plan",
                    "task has contradictory spec and plan ancestry",
                ));
            }
            result.spec = Some(plan.0.spec_id);
        }
        if let Some(spec_id) = result.spec
            && !self.specs.contains_key(&spec_id)
        {
            return Err(relationship_error(
                self.task_path(id),
                "spec",
                format!("spec `{spec_id}` does not exist"),
            ));
        }
        if let Some(parent_id) = task.parent_id {
            let parent = self.effective_ancestry(parent_id, visiting)?;
            if result.spec.zip(parent.spec).is_some_and(|(left, right)| left != right)
                || result.plan.zip(parent.plan).is_some_and(|(left, right)| left != right)
                || result
                    .phase
                    .zip(parent.phase)
                    .is_some_and(|(left, right)| left != right)
            {
                return Err(relationship_error(
                    self.task_path(id),
                    "parent",
                    "task and parent have contradictory ancestry",
                ));
            }
            result.spec = result.spec.or(parent.spec);
            result.plan = result.plan.or(parent.plan);
            result.phase = result.phase.or(parent.phase);
        }
        visiting.remove(&id);
        Ok(result)
    }

    fn link_target(&self, path: &Path, reference: &WorkspaceReference) -> Result<LinkedRecordKind> {
        macro_rules! target {
            ($kind:expr, $id:ty, $records:expr) => {{
                let id = parse_reference_id(path, "links", &reference.id, <$id>::parse)?;
                if !$records.contains_key(&id) {
                    return Err(relationship_error(
                        path.to_owned(),
                        "links",
                        format!("{} `{}` does not exist", reference.kind, reference.id),
                    ));
                }
                $kind
            }};
        }
        Ok(match reference.kind.as_str() {
            "capture" => target!(LinkedRecordKind::Capture, CaptureId, self.captures),
            "spec" => target!(LinkedRecordKind::Spec, SpecId, self.specs),
            "plan" => target!(LinkedRecordKind::Plan, PlanId, self.plans),
            "phase" => target!(LinkedRecordKind::Phase, PhaseId, self.phases),
            "task" => target!(LinkedRecordKind::Task, TaskId, self.tasks),
            "note" => target!(LinkedRecordKind::Note, NoteId, self.notes),
            "release" => target!(LinkedRecordKind::Release, ReleaseId, self.releases),
            other => {
                return Err(relationship_error(
                    path.to_owned(),
                    "links",
                    format!("unknown target kind `{other}`"),
                ));
            }
        })
    }
}

type Result<T> = std::result::Result<T, WorkspaceImportError>;

/// Parse, validate, replace, and canonicalize a workspace using an open database.
///
/// Files are parsed and the complete candidate graph is validated before the
/// database replacement transaction starts. Pass `vcs` when the project has a
/// Git worktree; the import refuses files Git reports as unmerged.
///
/// Blocker rules are checked against the database graph the import replaces.
/// The import refuses a violation that a workspace edge or parent link new to
/// the database causes. Violations already in the database load and come back
/// as warnings. A project that has never synced, or whose database holds no
/// records, has nothing to compare against, so every violation loads with a
/// warning.
pub fn import_graph(
    root: &Path, worktree_root: &Path, database: &mut Database, vcs: Option<&dyn Vcs>,
) -> Result<ImportOutcome> {
    // Taken before any read, so a commit from another connection after this
    // point refuses the replacement instead of being overwritten.
    let read = database.data_version()?;
    let project = database.project()?;
    let base = database.workspace_base()?;
    let before = database.connected_graph()?;
    let prepared = prepare_import(root, worktree_root, project, vcs, comparison_graph(&before, &base))?;
    apply_prepared(root, prepared, database, read)
}

/// Import a workspace while opening the database only after validation.
///
/// This entry point supports cloning a project with no local SQLite file. The
/// initial project ID is the same deterministic ID used by `arcl init`. Pass
/// `vcs` as for [`import_graph`]. With no database to compare against, every
/// blocker rule violation loads with a warning.
pub fn import_workspace(
    root: &Path, worktree_root: &Path, database_path: &Path, vcs: Option<&dyn Vcs>,
) -> Result<ImportOutcome> {
    let project = Project::from_parts(
        ProjectId::parse(DEFAULT_PROJECT_ID).expect("default project ID is canonical"),
        "Project".to_owned(),
    )
    .map_err(|error| WorkspaceImportError::Field {
        path: PathBuf::from("manifest.toml"),
        field: "project".to_owned(),
        message: error.to_string(),
    })?;
    let prepared = prepare_import(root, worktree_root, project, vcs, None)?;
    let mut database = Database::open(database_path)?;
    let read = database.data_version()?;
    apply_prepared(root, prepared, &mut database, read)
}

/// The database graph that blocker rule violations are classified against,
/// or `None` when there is nothing to compare: the project has never synced
/// (`base` is empty) or the database holds no records.
pub(super) fn comparison_graph<'a>(
    before: &'a ConnectedGraph, base: &[WorkspaceBaseFile],
) -> Option<&'a ConnectedGraph> {
    let empty = before.captures.is_empty()
        && before.specs.is_empty()
        && before.plans.is_empty()
        && before.phases.is_empty()
        && before.tasks.is_empty()
        && before.notes.is_empty()
        && before.releases.is_empty();
    (!base.is_empty() && !empty).then_some(before)
}

fn prepare_import(
    root: &Path, worktree_root: &Path, project: Project, vcs: Option<&dyn Vcs>, before: Option<&ConnectedGraph>,
) -> Result<PreparedImport> {
    reject_unmerged(root, vcs)?;
    let candidate = read_candidate(root)?;
    let (graph, warnings) = validate_candidate(candidate.records, project, worktree_root, before)?;
    let files = project_graph(&graph).map_err(|source| WorkspaceImportError::Field {
        path: PathBuf::from("manifest.toml"),
        field: "canonical rendering".to_owned(),
        message: source.to_string(),
    })?;
    Ok(PreparedImport { graph, files, observed: candidate.files, warnings })
}

/// Replace the database graph and base with `prepared`. `read` is the data
/// version taken before the caller's first read of the database.
fn apply_prepared(
    root: &Path, prepared: PreparedImport, database: &mut Database, read: DataVersion,
) -> Result<ImportOutcome> {
    let base = database.workspace_base()?;
    reject_removed_base_records(&base, &prepared.files)?;
    let current_files = project_graph(&database.connected_graph()?).map_err(|source| WorkspaceImportError::Field {
        path: PathBuf::from("manifest.toml"),
        field: "current database projection".to_owned(),
        message: source.to_string(),
    })?;
    reject_removed_workspace_records(&current_files, &prepared.files)?;
    reject_divergent_changes(&base, &current_files, &prepared.files)?;

    // Re-check exact bytes before filesystem canonicalization and before the
    // SQLite replacement transaction starts.
    export_observed(root, &prepared.files, &prepared.observed)?;
    let base = prepared
        .files
        .iter()
        .map(|file| WorkspaceBaseFile { path: file.path.to_string_lossy().into_owned(), content: file.content.clone() })
        .collect::<Vec<_>>();
    database.replace_graph_and_workspace_base(&prepared.graph, &base, read)?;
    Ok(ImportOutcome { files: prepared.files, warnings: prepared.warnings })
}

/// Refuse the workspace at `root` when Git reports any of its paths as
/// unmerged. Without `vcs` there is no Git state to check.
pub(super) fn reject_unmerged(root: &Path, vcs: Option<&dyn Vcs>) -> Result<()> {
    let Some(vcs) = vcs else { return Ok(()) };
    let paths = vcs
        .changed_paths(root)?
        .into_iter()
        .filter(|(_, state)| *state == PathState::Conflicted)
        .map(|(path, _)| path)
        .collect::<Vec<_>>();
    if paths.is_empty() { Ok(()) } else { Err(WorkspaceImportError::Unmerged { paths }) }
}

pub(super) fn read_candidate(root: &Path) -> Result<CandidateWorkspace> {
    let entries = read_entries(root)?;
    let Some(manifest_entry) = entries
        .iter()
        .find(|entry| entry.file_name() == OsStr::new("manifest.toml"))
    else {
        return Err(WorkspaceImportError::MissingManifest { path: root.join("manifest.toml") });
    };
    let manifest_path = root.join(manifest_entry.file_name());
    let manifest_type = manifest_entry
        .file_type()
        .map_err(|source| WorkspaceImportError::InspectEntry { path: manifest_path.clone(), source })?;
    if !manifest_type.is_file() {
        return Err(WorkspaceImportError::WrongEntryType { path: PathBuf::from("manifest.toml") });
    }
    let manifest_input = read_utf8_file(&manifest_path, Path::new("manifest.toml"))?;
    decode_manifest(&manifest_input)
        .map_err(|source| WorkspaceImportError::Manifest { path: PathBuf::from("manifest.toml"), source })?;
    read_candidate_records(root, entries, manifest_input)
}

fn read_candidate_records(
    root: &Path, entries: Vec<fs::DirEntry>, manifest_input: String,
) -> Result<CandidateWorkspace> {
    let mut records = BTreeMap::new();
    let mut files = vec![WorkspaceFile { path: PathBuf::from("manifest.toml"), content: manifest_input.into_bytes() }];
    for entry in entries {
        let name = entry.file_name();
        if name == OsStr::new("manifest.toml") {
            continue;
        }
        let relative = PathBuf::from(&name);
        let path = root.join(&name);
        let file_type = entry
            .file_type()
            .map_err(|source| WorkspaceImportError::InspectEntry { path: path.clone(), source })?;
        let Some(directory) = name.to_str() else {
            return Err(WorkspaceImportError::UnknownEntry { path: relative });
        };
        if !RECORD_DIRECTORIES.contains(&directory) {
            return Err(WorkspaceImportError::UnknownEntry { path: relative });
        }
        if !file_type.is_dir() {
            return Err(WorkspaceImportError::WrongEntryType { path: relative });
        }
        read_record_directory(root, directory, &mut records, &mut files)?;
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(CandidateWorkspace { records, files })
}

fn read_record_directory(
    root: &Path, directory: &str, records: &mut BTreeMap<PathBuf, WorkspaceRecord>, files: &mut Vec<WorkspaceFile>,
) -> Result<()> {
    for entry in read_entries(&root.join(directory))? {
        let name = entry.file_name();
        let relative = Path::new(directory).join(&name);
        let path = root.join(&relative);
        let file_type = entry
            .file_type()
            .map_err(|source| WorkspaceImportError::InspectEntry { path: path.clone(), source })?;
        if !file_type.is_file() {
            return Err(WorkspaceImportError::UnknownEntry { path: relative });
        }
        let Some(filename) = name.to_str() else {
            return Err(WorkspaceImportError::UnknownEntry { path: relative });
        };
        if !filename.ends_with(".md") {
            return Err(WorkspaceImportError::UnknownEntry { path: relative });
        }
        let input = read_utf8_file(&path, &relative)?;
        let record = decode_record(&relative, &input)
            .map_err(|source| WorkspaceImportError::Record { path: relative.clone(), source })?;
        files.push(WorkspaceFile { path: relative.clone(), content: input.into_bytes() });
        if records.insert(relative.clone(), record).is_some() {
            return Err(WorkspaceImportError::UnknownEntry { path: relative });
        }
    }
    Ok(())
}

/// Validate a complete candidate graph and build it.
///
/// `before` is the database graph the candidate replaces. Only blocker rule
/// violations that candidate edges or parent links new to `before` cause are
/// refused; the rest come back as warnings. With no `before`, every violation
/// is a warning; see [`comparison_graph`].
pub(super) fn validate_candidate(
    records: BTreeMap<PathBuf, WorkspaceRecord>, project: Project, _worktree_root: &Path,
    before: Option<&ConnectedGraph>,
) -> Result<(ConnectedGraph, Vec<String>)> {
    let mut ids = HashMap::<String, PathBuf>::new();
    for (path, record) in &records {
        if let Some(first) = ids.insert(record.id_string(), path.clone()) {
            return Err(field_error(
                path.clone(),
                "id",
                format!("record ID is duplicated; first declared in `{}`", first.display()),
            ));
        }
    }
    let index = RecordIndex::from_records(&records, project.id);
    let promotions = validate_captures(&records, &index)?;
    validate_specs(&records, &index, &promotions)?;
    validate_plans(&records, &index)?;
    validate_phases(&records, &index)?;
    let dependencies = validate_tasks(&records, &index)?;
    let memberships = validate_releases(&records, &index, project.id)?;
    let links = validate_links(&records, &index, project.id)?;
    let warnings = validate_blocker_rules(&dependencies, &index, before)?;

    let mut captures = values(&index.captures);
    let mut specs = values(&index.specs);
    let mut plans = values(&index.plans);
    let mut phases = values(&index.phases);
    let mut tasks = values(&index.tasks);
    let mut notes = values(&index.notes);
    let mut releases = values(&index.releases);
    captures.sort_by_key(|value| value.id);
    specs.sort_by_key(|value| value.id);
    plans.sort_by_key(|value| value.id);
    phases.sort_by_key(|value| value.id);
    tasks.sort_by_key(|value| value.id);
    notes.sort_by_key(|value| value.id);
    releases.sort_by_key(|value| value.id);

    let graph = ConnectedGraph {
        project,
        captures,
        capture_promotions: promotions,
        specs,
        plans,
        phases,
        tasks,
        dependencies,
        notes,
        releases,
        release_memberships: memberships,
        links,
    };
    Ok((graph, warnings))
}

fn values<T: Clone>(records: &HashMap<impl std::hash::Hash + Eq, (T, PathBuf)>) -> Vec<T> {
    records.values().map(|(value, _)| value.clone()).collect()
}

fn validate_captures(
    records: &BTreeMap<PathBuf, WorkspaceRecord>, index: &RecordIndex,
) -> Result<Vec<CapturePromotion>> {
    let mut promotions = Vec::new();
    for (path, record) in records {
        let WorkspaceRecord::Capture(record) = record else { continue };
        match (&record.status, &record.promoted_to) {
            (CaptureStatus::Promoted, Some(reference)) => {
                let target = match reference.kind.as_str() {
                    "spec" => {
                        let id = parse_reference_id(path, "promoted-to", &reference.id, SpecId::parse)?;
                        if !index.specs.contains_key(&id) {
                            return Err(relationship_error(
                                path.clone(),
                                "promoted-to",
                                format!("spec `{id}` does not exist"),
                            ));
                        }
                        CapturePromotionTarget::Spec(id)
                    }
                    "task" => {
                        let id = parse_reference_id(path, "promoted-to", &reference.id, TaskId::parse)?;
                        if !index.tasks.contains_key(&id) {
                            return Err(relationship_error(
                                path.clone(),
                                "promoted-to",
                                format!("task `{id}` does not exist"),
                            ));
                        }
                        CapturePromotionTarget::Task(id)
                    }
                    "note" => {
                        let id = parse_reference_id(path, "promoted-to", &reference.id, NoteId::parse)?;
                        if !index.notes.contains_key(&id) {
                            return Err(relationship_error(
                                path.clone(),
                                "promoted-to",
                                format!("note `{id}` does not exist"),
                            ));
                        }
                        CapturePromotionTarget::Note(id)
                    }
                    other => {
                        return Err(relationship_error(
                            path.clone(),
                            "promoted-to",
                            format!("unknown target kind `{other}`"),
                        ));
                    }
                };
                promotions.push(CapturePromotion {
                    project_id: index.captures[&record.id].0.project_id,
                    capture_id: record.id,
                    target,
                });
            }
            (CaptureStatus::Promoted, None) => {
                return Err(field_error(
                    path.clone(),
                    "status",
                    "promoted captures must have `promoted-to`",
                ));
            }
            (_, Some(_)) => {
                return Err(field_error(
                    path.clone(),
                    "promoted-to",
                    format!("capture status `{}` cannot have a promotion", record.status.as_str()),
                ));
            }
            _ => {}
        }
    }
    Ok(promotions)
}

fn validate_specs(
    records: &BTreeMap<PathBuf, WorkspaceRecord>, index: &RecordIndex, promotions: &[CapturePromotion],
) -> Result<()> {
    for (path, record) in records {
        let WorkspaceRecord::Spec(record) = record else { continue };
        let Some(capture_id) = record.source_capture_id else { continue };
        if !index.captures.contains_key(&capture_id) {
            return Err(relationship_error(
                path.clone(),
                "source-capture",
                format!("capture `{capture_id}` does not exist"),
            ));
        }
        let promoted = promotions.iter().find(|promotion| promotion.capture_id == capture_id);
        if promoted.map(|promotion| promotion.target) != Some(CapturePromotionTarget::Spec(record.id)) {
            return Err(relationship_error(
                path.clone(),
                "source-capture",
                format!("capture `{capture_id}` does not promote to spec `{}`", record.id),
            ));
        }
    }
    Ok(())
}

fn validate_plans(records: &BTreeMap<PathBuf, WorkspaceRecord>, index: &RecordIndex) -> Result<()> {
    for (path, record) in records {
        if let WorkspaceRecord::Plan(record) = record
            && !index.specs.contains_key(&record.spec_id)
        {
            return Err(relationship_error(
                path.clone(),
                "spec",
                format!("spec `{}` does not exist", record.spec_id),
            ));
        }
    }
    Ok(())
}

fn validate_phases(records: &BTreeMap<PathBuf, WorkspaceRecord>, index: &RecordIndex) -> Result<()> {
    for (path, record) in records {
        if let WorkspaceRecord::Phase(record) = record
            && !index.plans.contains_key(&record.plan_id)
        {
            return Err(relationship_error(
                path.clone(),
                "plan",
                format!("plan `{}` does not exist", record.plan_id),
            ));
        }
    }
    Ok(())
}

fn validate_tasks(records: &BTreeMap<PathBuf, WorkspaceRecord>, index: &RecordIndex) -> Result<Vec<TaskDependency>> {
    let mut dependencies = Vec::new();
    for (path, record) in records {
        let WorkspaceRecord::Task(record) = record else { continue };
        if record.spec_id.is_some_and(|id| !index.specs.contains_key(&id)) {
            return Err(relationship_error(
                path.clone(),
                "spec",
                format!("specification does not exist for task `{}`", record.id),
            ));
        }
        if record.plan_id.is_some_and(|id| !index.plans.contains_key(&id)) {
            return Err(relationship_error(
                path.clone(),
                "plan",
                format!("plan does not exist for task `{}`", record.id),
            ));
        }
        if record.phase_id.is_some_and(|id| !index.phases.contains_key(&id)) {
            return Err(relationship_error(
                path.clone(),
                "phase",
                format!("phase does not exist for task `{}`", record.id),
            ));
        }
        if let Some(parent_id) = record.parent_id {
            if parent_id == record.id {
                return Err(relationship_error(
                    path.clone(),
                    "parent",
                    "a task cannot be its own parent",
                ));
            }
            if !index.tasks.contains_key(&parent_id) {
                return Err(relationship_error(
                    path.clone(),
                    "parent",
                    format!("parent task `{parent_id}` does not exist"),
                ));
            }
        }
        index.effective_ancestry(record.id, &mut HashSet::new())?;
        let mut seen = HashSet::new();
        for blocker_id in &record.blocked_by {
            if !index.tasks.contains_key(blocker_id) {
                return Err(relationship_error(
                    path.clone(),
                    "blocked-by",
                    format!("blocker task `{blocker_id}` does not exist"),
                ));
            }
            if !seen.insert(*blocker_id) {
                return Err(relationship_error(
                    path.clone(),
                    "blocked-by",
                    format!("dependency to `{blocker_id}` is duplicated"),
                ));
            }
            // A self-loop is built as stored rather than refused here, so
            // the blocker rules treat it as a one-task cycle: a new one is
            // refused and one the database already holds warns.
            dependencies.push(TaskDependency { task_id: record.id, blocker_id: *blocker_id });
        }
    }
    Ok(dependencies)
}

fn validate_releases(
    records: &BTreeMap<PathBuf, WorkspaceRecord>, index: &RecordIndex, project_id: ProjectId,
) -> Result<Vec<ReleaseMembership>> {
    let mut memberships = Vec::new();
    for (path, record) in records {
        let WorkspaceRecord::Release(record) = record else { continue };
        let mut seen = HashSet::new();
        for member in &record.members {
            let kind = match member.kind.as_str() {
                "spec" => {
                    let id = parse_reference_id(path, "members", &member.id, SpecId::parse)?;
                    if !index.specs.contains_key(&id) {
                        return Err(relationship_error(
                            path.clone(),
                            "members",
                            format!("spec `{id}` does not exist"),
                        ));
                    }
                    ReleaseMemberKind::Spec
                }
                "plan" => {
                    let id = parse_reference_id(path, "members", &member.id, PlanId::parse)?;
                    if !index.plans.contains_key(&id) {
                        return Err(relationship_error(
                            path.clone(),
                            "members",
                            format!("plan `{id}` does not exist"),
                        ));
                    }
                    ReleaseMemberKind::Plan
                }
                "task" => {
                    let id = parse_reference_id(path, "members", &member.id, TaskId::parse)?;
                    if !index.tasks.contains_key(&id) {
                        return Err(relationship_error(
                            path.clone(),
                            "members",
                            format!("task `{id}` does not exist"),
                        ));
                    }
                    ReleaseMemberKind::Task
                }
                "note" => {
                    let id = parse_reference_id(path, "members", &member.id, NoteId::parse)?;
                    if !index.notes.contains_key(&id) {
                        return Err(relationship_error(
                            path.clone(),
                            "members",
                            format!("note `{id}` does not exist"),
                        ));
                    }
                    ReleaseMemberKind::Note
                }
                other => {
                    return Err(relationship_error(
                        path.clone(),
                        "members",
                        format!("unknown member kind `{other}`"),
                    ));
                }
            };
            if !seen.insert((kind, member.id.clone())) {
                return Err(relationship_error(
                    path.clone(),
                    "members",
                    format!("member `{}` is duplicated", member.id),
                ));
            }
            memberships.push(ReleaseMembership {
                project_id,
                release_id: record.id,
                record_kind: kind,
                record_id: member.id.clone(),
            });
        }
    }
    Ok(memberships)
}

fn validate_links(
    records: &BTreeMap<PathBuf, WorkspaceRecord>, index: &RecordIndex, project_id: ProjectId,
) -> Result<Vec<RecordLink>> {
    let mut links = Vec::new();
    let mut seen = HashSet::new();
    for (path, record) in records {
        let (source_kind, source_id, references) = match record {
            WorkspaceRecord::Capture(record) => (
                LinkedRecordKind::Capture,
                record.id.to_string(),
                record.links.as_slice(),
            ),
            WorkspaceRecord::Spec(record) => (LinkedRecordKind::Spec, record.id.to_string(), record.links.as_slice()),
            WorkspaceRecord::Plan(record) => (LinkedRecordKind::Plan, record.id.to_string(), record.links.as_slice()),
            WorkspaceRecord::Phase(record) => (LinkedRecordKind::Phase, record.id.to_string(), record.links.as_slice()),
            WorkspaceRecord::Task(record) => (LinkedRecordKind::Task, record.id.to_string(), record.links.as_slice()),
            WorkspaceRecord::Note(record) => (LinkedRecordKind::Note, record.id.to_string(), record.links.as_slice()),
            WorkspaceRecord::Release(record) => (
                LinkedRecordKind::Release,
                record.id.to_string(),
                record.links.as_slice(),
            ),
        };
        for reference in references {
            let target_kind = index.link_target(path, reference)?;
            if !seen.insert((source_kind, source_id.clone(), target_kind, reference.id.clone())) {
                return Err(relationship_error(
                    path.clone(),
                    "links",
                    format!("link to `{}` is duplicated", reference.id),
                ));
            }
            links.push(RecordLink {
                project_id,
                source_kind,
                source_id: source_id.clone(),
                target_kind,
                target_id: reference.id.clone(),
            });
        }
    }
    links.sort_by_key(|link| {
        (
            link.source_kind,
            link.source_id.clone(),
            link.target_kind,
            link.target_id.clone(),
        )
    });
    Ok(links)
}

/// Refuse the first blocker rule violation the candidate introduces, naming
/// the blocked task's record and the tasks along the cycle or parent chain.
///
/// A violation is introduced when it involves an edge, or crosses a parent
/// link, that `before` does not hold. Every other violation is returned as a
/// warning. With no `before`, nothing is introduced.
fn validate_blocker_rules(
    dependencies: &[TaskDependency], index: &RecordIndex, before: Option<&ConnectedGraph>,
) -> Result<Vec<String>> {
    let parents = index
        .tasks
        .values()
        .filter_map(|(task, _)| task.parent_id.map(|parent| (task.id, parent)))
        .collect::<HashMap<_, _>>();
    let (caused, existing) = match before {
        Some(before) => {
            let old_edges = before.dependencies.iter().collect::<HashSet<_>>();
            let old_parents = before
                .tasks
                .iter()
                .filter_map(|task| task.parent_id.map(|parent| (task.id, parent)))
                .collect::<HashSet<_>>();
            let new_edges = dependencies
                .iter()
                .filter(|edge| !old_edges.contains(edge))
                .copied()
                .collect::<Vec<_>>();
            let new_parents = parents
                .iter()
                .map(|(child, parent)| (*child, *parent))
                .filter(|link| !old_parents.contains(link))
                .collect::<Vec<_>>();
            partition_violations(&parents, dependencies, &new_edges, &new_parents)
        }
        None => (Vec::new(), validate_blocker_edges(&parents, dependencies)),
    };
    if let Some(violation) = caused.first() {
        return Err(blocker_error(index, violation));
    }
    let reason = if before.is_some() {
        "it was already in the database, so it loads"
    } else {
        "there is no earlier graph to compare against, so it loads"
    };
    Ok(existing
        .iter()
        .map(|violation| format!("{}; {reason}", blocker_error(index, violation)))
        .collect())
}

/// The error that names a blocker rule violation at the blocked task's record.
fn blocker_error(index: &RecordIndex, violation: &EdgeViolation) -> WorkspaceImportError {
    let task = match violation {
        EdgeViolation::Cycle { path } => path[0],
        EdgeViolation::Ancestry { edge, .. } => edge.task_id,
    };
    relationship_error(index.task_path(task), "blocked-by", violation.to_error().to_string())
}

fn capture_from_record(record: &CaptureRecord, project_id: ProjectId) -> Capture {
    Capture {
        id: record.id,
        project_id,
        title: record.title.clone(),
        body: record.body.clone(),
        status: record.status,
        created_at: record.created_at.clone(),
    }
}
fn spec_from_record(record: &SpecRecord, project_id: ProjectId) -> Spec {
    Spec {
        id: record.id,
        project_id,
        title: record.title.clone(),
        body: record.body.clone(),
        acceptance_criteria: record.acceptance_criteria.clone(),
        status: record.status,
        source_capture_id: record.source_capture_id,
    }
}
fn plan_from_record(record: &PlanRecord, project_id: ProjectId) -> Plan {
    Plan {
        id: record.id,
        project_id,
        spec_id: record.spec_id,
        title: record.title.clone(),
        body: record.body.clone(),
        status: record.status,
    }
}
fn phase_from_record(record: &PhaseRecord, project_id: ProjectId) -> Phase {
    Phase {
        id: record.id,
        project_id,
        plan_id: record.plan_id,
        plan_key: record.plan_key.clone(),
        title: record.title.clone(),
        body: record.body.clone(),
        status: record.status,
        position: record.position,
    }
}
fn task_from_record(record: &TaskRecord, project_id: ProjectId) -> PlanningTask {
    PlanningTask {
        id: record.id,
        project_id,
        spec_id: record.spec_id,
        plan_id: record.plan_id,
        phase_id: record.phase_id,
        parent_id: record.parent_id,
        plan_key: record.plan_key.clone(),
        title: record.title.clone(),
        body: record.body.clone(),
        status: record.status,
        priority: record.priority,
        position: record.position,
        handoff: record.handoff.clone().unwrap_or_default(),
        evidence: record.evidence.clone().unwrap_or_default(),
    }
}
fn note_from_record(record: &NoteRecord, project_id: ProjectId) -> Note {
    Note { id: record.id, project_id, title: record.title.clone(), body: record.body.clone() }
}
fn release_from_record(record: &ReleaseRecord) -> Release {
    Release { id: record.id, title: record.title.clone(), description: record.body.clone(), status: record.status }
}

fn parse_reference_id<T>(
    path: &Path, relationship: &str, value: &str,
    parse: impl Fn(&str) -> std::result::Result<T, arcl_core::domain::IdError>,
) -> Result<T> {
    parse(value).map_err(|error| relationship_error(path.to_owned(), relationship, error.to_string()))
}
fn field_error(path: PathBuf, field: impl Into<String>, message: impl Into<String>) -> WorkspaceImportError {
    WorkspaceImportError::Field { path, field: field.into(), message: message.into() }
}
fn relationship_error(
    path: PathBuf, relationship: impl Into<String>, message: impl Into<String>,
) -> WorkspaceImportError {
    WorkspaceImportError::Relationship { path, relationship: relationship.into(), message: message.into() }
}
fn read_entries(path: &Path) -> Result<Vec<fs::DirEntry>> {
    let entries =
        fs::read_dir(path).map_err(|source| WorkspaceImportError::ReadDirectory { path: path.to_owned(), source })?;
    let mut entries = entries
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|source| WorkspaceImportError::ReadDirectory { path: path.to_owned(), source })?;
    entries.sort_by_key(|entry| entry.file_name().to_string_lossy().into_owned());
    Ok(entries)
}
fn read_utf8_file(path: &Path, relative: &Path) -> Result<String> {
    let bytes =
        fs::read(path).map_err(|source| WorkspaceImportError::ReadDirectory { path: path.to_owned(), source })?;
    String::from_utf8(bytes).map_err(|_| WorkspaceImportError::NonUtf8 { path: relative.to_owned() })
}
fn reject_removed_base_records(base: &[WorkspaceBaseFile], files: &[WorkspaceFile]) -> Result<()> {
    let current_paths = files.iter().map(|file| file.path.clone()).collect::<HashSet<_>>();
    for base_file in base {
        let path = PathBuf::from(&base_file.path);
        if let Some(id) = record_id_from_path(&path)
            && !current_paths.contains(&path)
        {
            return Err(WorkspaceImportError::ExistingRecordRemoved { path, id });
        }
    }
    Ok(())
}
fn reject_removed_workspace_records(existing: &[WorkspaceFile], candidate: &[WorkspaceFile]) -> Result<()> {
    let candidate_paths = candidate.iter().map(|file| &file.path).collect::<HashSet<_>>();
    for file in existing {
        if let Some(id) = record_id_from_path(&file.path)
            && !candidate_paths.contains(&file.path)
        {
            return Err(WorkspaceImportError::ExistingRecordRemoved { path: file.path.clone(), id });
        }
    }
    Ok(())
}
fn reject_divergent_changes(
    base: &[WorkspaceBaseFile], current: &[WorkspaceFile], candidate: &[WorkspaceFile],
) -> Result<()> {
    let base = base
        .iter()
        .map(|file| (PathBuf::from(&file.path), file.content.as_slice()))
        .collect::<HashMap<_, _>>();
    let current = current
        .iter()
        .map(|file| (&file.path, file.content.as_slice()))
        .collect::<HashMap<_, _>>();
    for file in candidate {
        let Some(base_content) = base.get(&file.path) else {
            // Without a stored common base there is no safe three-way
            // comparison. An explicit import is allowed to establish it.
            continue;
        };
        let database_changed = current.get(&file.path).is_none_or(|content| *content != *base_content);
        let workspace_changed = file.content.as_slice() != *base_content;
        if database_changed && workspace_changed && current.get(&file.path) != Some(&file.content.as_slice()) {
            return Err(WorkspaceImportError::Conflict { path: file.path.clone() });
        }
    }
    Ok(())
}

fn record_id_from_path(path: &Path) -> Option<String> {
    let mut components = path.components();
    let Component::Normal(directory) = components.next()? else { return None };
    let Component::Normal(filename) = components.next()? else { return None };
    if components.next().is_some()
        || !RECORD_DIRECTORIES
            .iter()
            .any(|candidate| directory == OsStr::new(candidate))
    {
        return None;
    }
    filename
        .to_str()?
        .strip_suffix(".md")
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vcs::FakeVcs;
    use crate::workspace::export::export_graph;
    use arcl_core::domain::TaskPriority;
    use arcl_store::PlanningTaskCreate;

    #[test]
    fn import_exports_and_reimports_the_connected_graph() {
        let mut source = Database::open_in_memory().expect("database opens");
        let project = source.project().expect("project exists");
        let spec = source
            .create_spec("Spec".to_owned(), "Spec body".to_owned(), "criterion".to_owned())
            .expect("spec creates");
        let plan = source
            .create_plan(spec.id, "Plan".to_owned(), "Plan body".to_owned())
            .expect("plan creates");
        let task = source
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: Some(spec.id),
                plan_id: Some(plan.id),
                phase_id: None,
                parent_id: None,
                title: "Task".to_owned(),
                body: "Task body".to_owned(),
                priority: TaskPriority::High,
                position: 0,
            })
            .expect("task creates");
        let root = tempfile::tempdir().expect("workspace creates");
        export_graph(root.path(), &source.connected_graph().expect("graph loads")).expect("export succeeds");

        let mut destination = Database::open_in_memory().expect("database opens");
        import_graph(root.path(), root.path(), &mut destination, None).expect("import succeeds");
        let graph = destination.connected_graph().expect("graph loads");
        assert_eq!(graph.specs[0].body, "Spec body");
        assert_eq!(graph.plans[0].body, "Plan body");
        assert_eq!(graph.tasks[0].body, "Task body");
        assert_eq!(graph.tasks[0].id, task.id);
        assert_eq!(destination.workspace_base().expect("base loads").len(), 4);
    }

    #[test]
    fn invalid_candidate_does_not_replace_the_database() {
        let mut database = Database::open_in_memory().expect("database opens");
        let existing = database
            .create_spec("Existing".to_owned(), String::new(), String::new())
            .expect("spec creates");
        let root = tempfile::tempdir().expect("workspace creates");
        fs::write(root.path().join("manifest.toml"), "format-version = 1\n").expect("manifest writes");
        fs::create_dir(root.path().join("plans")).expect("plans directory creates");
        let plan_id = "arcl-pl-01K0B2ZWTX7JX9PH7W5G1S6A9Q";
        fs::write(root.path().join(format!("plans/{plan_id}.md")), format!(
            "+++\nid = \"{plan_id}\"\nspec = \"arcl-s-01K0B2ZWTX7JX9PH7W5G1S6AR\"\ntitle = \"Plan\"\nstatus = \"open\"\n+++\n"
        )).expect("plan writes");
        let error = import_graph(root.path(), root.path(), &mut database, None).expect_err("missing spec rejects");
        assert!(error.to_string().contains("spec"));
        assert!(database.spec(existing.id).expect("spec reads").is_some());
    }

    /// Tasks Root, Child under Root, and an unrelated Other in `database`,
    /// synced once, and a workspace exported from them plus the blocker
    /// edges `edges` builds from their IDs. `database` holds no edges.
    struct EdgeFixture {
        root: TaskId,
        child: TaskId,
        other: TaskId,
        database: Database,
        workspace: tempfile::TempDir,
    }

    impl EdgeFixture {
        fn new(edges: impl Fn(TaskId, TaskId, TaskId) -> Vec<TaskDependency>) -> Self {
            let mut database = Database::open_in_memory().expect("database opens");
            let project = database.project().expect("project exists");
            let create = |parent_id, title: &str| PlanningTaskCreate {
                project_id: project.id,
                spec_id: None,
                plan_id: None,
                phase_id: None,
                parent_id,
                title: title.to_owned(),
                body: String::new(),
                priority: TaskPriority::Normal,
                position: 0,
            };
            let root = database
                .create_planning_task(create(None, "Root"))
                .expect("root creates");
            let child = database
                .create_planning_task(create(Some(root.id), "Child"))
                .expect("child creates");
            let other = database
                .create_planning_task(create(None, "Other"))
                .expect("other creates");
            let mut graph = database.connected_graph().expect("graph loads");
            let workspace = tempfile::tempdir().expect("workspace creates");
            // Import the graph without edges first, so the project has synced.
            export_graph(workspace.path(), &graph).expect("export succeeds");
            import_graph(workspace.path(), workspace.path(), &mut database, None).expect("first import succeeds");
            graph.dependencies = edges(root.id, child.id, other.id);
            for file in project_graph(&graph).expect("projection succeeds") {
                fs::write(workspace.path().join(&file.path), &file.content).expect("record writes");
            }
            Self { root: root.id, child: child.id, other: other.id, database, workspace }
        }

        /// Import the workspace into the fixture's database, which lacks the
        /// edges, and return the refusal.
        fn refused(mut self) -> String {
            let error = import_graph(self.workspace.path(), self.workspace.path(), &mut self.database, None)
                .expect_err("a new bad edge refuses the import");
            assert!(
                self.database
                    .connected_graph()
                    .expect("graph loads")
                    .dependencies
                    .is_empty()
            );
            error.to_string()
        }
    }

    #[test]
    fn import_refuses_a_descendant_blocker_the_workspace_adds() {
        let fixture = EdgeFixture::new(|root, child, _| vec![TaskDependency::new(root, child).expect("edge")]);
        let (root, child) = (fixture.root, fixture.child);
        assert_eq!(
            fixture.refused(),
            format!(
                "workspace file `tasks/{root}.md`: relationship `blocked-by`: task `{root}` cannot be blocked by \
                 `{child}`, its descendant; parent chain: {child} -> {root}"
            )
        );
    }

    #[test]
    fn import_refuses_a_blocker_cycle_the_workspace_adds() {
        let fixture = EdgeFixture::new(|_, child, other| {
            vec![
                TaskDependency::new(child, other).expect("edge"),
                TaskDependency::new(other, child).expect("edge"),
            ]
        });
        let (child, other) = (fixture.child, fixture.other);
        let (first, second) = if child < other { (child, other) } else { (other, child) };
        assert_eq!(
            fixture.refused(),
            format!(
                "workspace file `tasks/{first}.md`: relationship `blocked-by`: blocker edges would form a cycle: \
                 {first} -> {second} -> {first}; each task is blocked by the next"
            )
        );
    }

    #[test]
    fn import_loads_a_violation_with_a_warning_when_nothing_is_new() {
        let fixture = EdgeFixture::new(|root, child, _| vec![TaskDependency::new(root, child).expect("edge")]);
        let (root, child) = (fixture.root, fixture.child);
        let violation = format!(
            "workspace file `tasks/{root}.md`: relationship `blocked-by`: task `{root}` cannot be blocked by \
             `{child}`, its descendant; parent chain: {child} -> {root}"
        );

        // An empty database has nothing to compare against.
        let mut destination = Database::open_in_memory().expect("database opens");
        let path = fixture.workspace.path();
        let outcome = import_graph(path, path, &mut destination, None).expect("fresh import loads");
        assert_eq!(
            outcome.warnings,
            vec![format!(
                "{violation}; there is no earlier graph to compare against, so it loads"
            )]
        );
        let edge = TaskDependency::new(root, child).expect("edge");
        assert_eq!(
            destination.connected_graph().expect("graph loads").dependencies,
            vec![edge]
        );

        // The second import finds the edge already in the database.
        let outcome = import_graph(path, path, &mut destination, None).expect("repeat import loads");
        assert_eq!(
            outcome.warnings,
            vec![format!("{violation}; it was already in the database, so it loads")]
        );
    }

    #[test]
    fn import_refuses_a_self_loop_the_workspace_adds() {
        let fixture = EdgeFixture::new(|_, _, other| vec![TaskDependency { task_id: other, blocker_id: other }]);
        let other = fixture.other;
        assert_eq!(
            fixture.refused(),
            format!(
                "workspace file `tasks/{other}.md`: relationship `blocked-by`: blocker edges would form a cycle: \
                 {other} -> {other}; each task is blocked by the next"
            )
        );
    }

    #[test]
    fn import_loads_a_stored_self_loop_with_a_warning() {
        let self_loop = |other| TaskDependency { task_id: other, blocker_id: other };
        let fixture = EdgeFixture::new(|_, _, other| vec![self_loop(other)]);
        let other = fixture.other;
        let violation = format!(
            "workspace file `tasks/{other}.md`: relationship `blocked-by`: blocker edges would form a cycle: \
             {other} -> {other}; each task is blocked by the next"
        );

        // An empty database has nothing to compare against, so the loop loads.
        let mut destination = Database::open_in_memory().expect("database opens");
        let path = fixture.workspace.path();
        let outcome = import_graph(path, path, &mut destination, None).expect("fresh import loads");
        assert_eq!(
            outcome.warnings,
            vec![format!(
                "{violation}; there is no earlier graph to compare against, so it loads"
            )]
        );
        assert_eq!(
            destination.connected_graph().expect("graph loads").dependencies,
            vec![self_loop(other)]
        );

        // The second import finds the stored loop and warns again.
        let outcome = import_graph(path, path, &mut destination, None).expect("repeat import loads");
        assert_eq!(
            outcome.warnings,
            vec![format!("{violation}; it was already in the database, so it loads")]
        );
    }

    /// Export one capture whose body is `body`, and return the workspace
    /// and the capture's path.
    fn exported_capture(body: &str) -> (tempfile::TempDir, PathBuf) {
        let mut source = Database::open_in_memory().expect("database opens");
        source
            .create_capture("Capture".to_owned(), body.to_owned())
            .expect("capture creates");
        let root = tempfile::tempdir().expect("workspace creates");
        let files =
            export_graph(root.path(), &source.connected_graph().expect("graph loads")).expect("export succeeds");
        let capture = files
            .iter()
            .map(|file| file.path.clone())
            .find(|path| path.starts_with("captures"))
            .expect("capture exported");
        (root, capture)
    }

    #[test]
    fn unmerged_files_refuse_the_import() {
        let (root, capture) = exported_capture("Capture body");
        let vcs = FakeVcs::unmerged(&[&capture]);

        let mut destination = Database::open_in_memory().expect("database opens");
        let error =
            import_graph(root.path(), root.path(), &mut destination, Some(&vcs)).expect_err("unmerged files refuse");
        assert!(
            matches!(&error, WorkspaceImportError::Unmerged { paths } if paths.as_slice() == std::slice::from_ref(&capture)),
            "{error:?}"
        );
        assert_eq!(error.exit_code(), 4);
        assert!(error.to_string().contains("unmerged in Git"));
        assert!(destination.connected_graph().expect("graph loads").captures.is_empty());
    }

    #[test]
    fn a_conflict_example_in_a_body_imports() {
        let body = "```text\n<<<<<<< HEAD\nOurs\n=======\nTheirs\n>>>>>>> other\n```";
        let (root, _) = exported_capture(body);
        let vcs = FakeVcs { changed: Vec::new() };

        let mut destination = Database::open_in_memory().expect("database opens");
        import_graph(root.path(), root.path(), &mut destination, Some(&vcs)).expect("import succeeds");
        assert_eq!(
            destination.connected_graph().expect("graph loads").captures[0].body,
            body
        );
    }
}
