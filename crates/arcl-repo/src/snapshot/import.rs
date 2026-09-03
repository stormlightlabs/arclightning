use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ffi::OsStr,
    fs, io,
    path::{Component, Path, PathBuf},
};

use thiserror::Error;

use arcl_core::domain::*;
use arcl_store::{ConnectedGraph, Database, SnapshotBaseFile, StorageError};

use super::{
    CaptureRecord, NoteRecord, PhaseRecord, PlanRecord, ReleaseRecord, SnapshotError, SnapshotFile, SnapshotRecord,
    SnapshotReference, SpecRecord, TaskRecord, decode_manifest, decode_record,
};
use super::{export::export_observed, export::project_graph};

const DEFAULT_PROJECT_ID: &str = "arcl-pj-00000000000000000000000000";
const RECORD_DIRECTORIES: &[&str] = &["captures", "releases", "specs", "plans", "phases", "tasks", "notes"];

/// A validation or I/O failure raised while importing a complete snapshot.
#[derive(Debug, Error)]
pub enum SnapshotImportError {
    /// The workspace root or one of its record directories could not be read.
    #[error("could not read snapshot directory `{path}`: {source}")]
    ReadDirectory { path: PathBuf, source: io::Error },
    /// A directory entry could not be inspected.
    #[error("could not inspect snapshot entry `{path}`: {source}")]
    InspectEntry { path: PathBuf, source: io::Error },
    /// The required manifest was not present.
    #[error("snapshot manifest `{path}` is missing")]
    MissingManifest { path: PathBuf },
    /// A file or directory is outside the workspace layout.
    #[error("snapshot entry `{path}` is not allowed in the snapshot layout")]
    UnknownEntry { path: PathBuf },
    /// A known workspace path has the wrong file type.
    #[error("snapshot entry `{path}` has the wrong type")]
    WrongEntryType { path: PathBuf },
    /// A snapshot file contains bytes that are not UTF-8.
    #[error("snapshot file `{path}` is not valid UTF-8")]
    NonUtf8 { path: PathBuf },
    /// The manifest could not be decoded or validated.
    #[error("snapshot file `{path}`: field `format-version`: {source}")]
    Manifest { path: PathBuf, source: SnapshotError },
    /// A record could not be decoded or validated by the snapshot codec.
    #[error("snapshot file `{path}`: field `front matter`: {source}")]
    Record { path: PathBuf, source: SnapshotError },
    /// A scalar field violates a complete-graph invariant.
    #[error("snapshot file `{path}`: field `{field}`: {message}")]
    Field {
        path: PathBuf,
        field: String,
        message: String,
    },
    /// A relationship violates a complete-graph invariant.
    #[error("snapshot file `{path}`: relationship `{relationship}`: {message}")]
    Relationship {
        path: PathBuf,
        relationship: String,
        message: String,
    },
    /// A record ID represented in the stored base was removed or renamed.
    #[error("snapshot file `{path}`: record ID `{id}` was removed or renamed; snapshots do not support hard deletion")]
    ExistingRecordRemoved { path: PathBuf, id: String },
    /// Both the database projection and workspace changed since the last base.
    #[error("snapshot file `{path}` has divergent database and workspace changes")]
    Conflict { path: PathBuf },
    /// A filesystem export used to canonicalize the candidate failed.
    #[error(transparent)]
    Export(#[from] super::SnapshotExportError),
    /// The complete replacement could not be committed to SQLite.
    #[error(transparent)]
    Storage(#[from] StorageError),
}

impl SnapshotImportError {
    /// Return the CLI exit category for this import failure.
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Storage(_) | Self::ReadDirectory { .. } | Self::InspectEntry { .. } => 1,
            Self::Export(super::SnapshotExportError::Conflict { .. }) => 4,
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
            Self::Conflict { .. } => 4,
        }
    }
}

struct CandidateSnapshot {
    records: BTreeMap<PathBuf, SnapshotRecord>,
    files: Vec<SnapshotFile>,
}

struct PreparedImport {
    graph: ConnectedGraph,
    files: Vec<SnapshotFile>,
    observed: Vec<SnapshotFile>,
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
    fn from_records(records: &BTreeMap<PathBuf, SnapshotRecord>, project_id: ProjectId) -> Self {
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
                SnapshotRecord::Capture(record) => {
                    index
                        .captures
                        .insert(record.id, (capture_from_record(record, project_id), path.clone()));
                }
                SnapshotRecord::Spec(record) => {
                    index
                        .specs
                        .insert(record.id, (spec_from_record(record, project_id), path.clone()));
                }
                SnapshotRecord::Plan(record) => {
                    index
                        .plans
                        .insert(record.id, (plan_from_record(record, project_id), path.clone()));
                }
                SnapshotRecord::Phase(record) => {
                    index
                        .phases
                        .insert(record.id, (phase_from_record(record, project_id), path.clone()));
                }
                SnapshotRecord::Task(record) => {
                    index
                        .tasks
                        .insert(record.id, (task_from_record(record, project_id), path.clone()));
                }
                SnapshotRecord::Note(record) => {
                    index
                        .notes
                        .insert(record.id, (note_from_record(record, project_id), path.clone()));
                }
                SnapshotRecord::Release(record) => {
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

    fn link_target(&self, path: &Path, reference: &SnapshotReference) -> Result<LinkedRecordKind> {
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

type Result<T> = std::result::Result<T, SnapshotImportError>;

/// Parse, validate, replace, and canonicalize a workspace using an open database.
///
/// Files are parsed and the complete candidate graph is validated before the
/// database replacement transaction starts.
pub fn import_graph(root: &Path, worktree_root: &Path, database: &mut Database) -> Result<Vec<SnapshotFile>> {
    let project = database.project()?;
    let prepared = prepare_import(root, worktree_root, project)?;
    apply_prepared(root, prepared, database)
}

/// Import a workspace while opening the database only after validation.
///
/// This entry point supports cloning a project with no local SQLite file. The
/// initial project ID is the same deterministic ID used by `arcl init`.
pub fn import_snapshot(root: &Path, worktree_root: &Path, database_path: &Path) -> Result<Vec<SnapshotFile>> {
    let project = Project::from_parts(
        ProjectId::parse(DEFAULT_PROJECT_ID).expect("default project ID is canonical"),
        "Project".to_owned(),
    )
    .map_err(|error| SnapshotImportError::Field {
        path: PathBuf::from("manifest.toml"),
        field: "project".to_owned(),
        message: error.to_string(),
    })?;
    let prepared = prepare_import(root, worktree_root, project)?;
    let mut database = Database::open(database_path)?;
    apply_prepared(root, prepared, &mut database)
}

fn prepare_import(root: &Path, worktree_root: &Path, project: Project) -> Result<PreparedImport> {
    let candidate = read_candidate(root)?;
    let graph = validate_candidate(candidate.records, project, worktree_root)?;
    let files = project_graph(&graph).map_err(|source| SnapshotImportError::Field {
        path: PathBuf::from("manifest.toml"),
        field: "canonical rendering".to_owned(),
        message: source.to_string(),
    })?;
    Ok(PreparedImport { graph, files, observed: candidate.files })
}

fn apply_prepared(root: &Path, prepared: PreparedImport, database: &mut Database) -> Result<Vec<SnapshotFile>> {
    let base = database.snapshot_base()?;
    reject_removed_base_records(&base, &prepared.files)?;
    let current_files = project_graph(&database.connected_graph()?).map_err(|source| SnapshotImportError::Field {
        path: PathBuf::from("manifest.toml"),
        field: "current database projection".to_owned(),
        message: source.to_string(),
    })?;
    reject_removed_snapshot_records(&current_files, &prepared.files)?;
    reject_divergent_changes(&base, &current_files, &prepared.files)?;

    // Re-check exact bytes before filesystem canonicalization and before the
    // SQLite replacement transaction starts.
    export_observed(root, &prepared.files, &prepared.observed)?;
    let base = prepared
        .files
        .iter()
        .map(|file| SnapshotBaseFile { path: file.path.to_string_lossy().into_owned(), content: file.content.clone() })
        .collect::<Vec<_>>();
    database.replace_graph_and_snapshot_base(&prepared.graph, &base)?;
    Ok(prepared.files)
}

fn read_candidate(root: &Path) -> Result<CandidateSnapshot> {
    let entries = read_entries(root)?;
    let Some(manifest_entry) = entries
        .iter()
        .find(|entry| entry.file_name() == OsStr::new("manifest.toml"))
    else {
        return Err(SnapshotImportError::MissingManifest { path: root.join("manifest.toml") });
    };
    let manifest_path = root.join(manifest_entry.file_name());
    let manifest_type = manifest_entry
        .file_type()
        .map_err(|source| SnapshotImportError::InspectEntry { path: manifest_path.clone(), source })?;
    if !manifest_type.is_file() {
        return Err(SnapshotImportError::WrongEntryType { path: PathBuf::from("manifest.toml") });
    }
    let manifest_input = read_utf8_file(&manifest_path, Path::new("manifest.toml"))?;
    decode_manifest(&manifest_input)
        .map_err(|source| SnapshotImportError::Manifest { path: PathBuf::from("manifest.toml"), source })?;
    read_candidate_records(root, entries, manifest_input)
}

fn read_candidate_records(
    root: &Path, entries: Vec<fs::DirEntry>, manifest_input: String,
) -> Result<CandidateSnapshot> {
    let mut records = BTreeMap::new();
    let mut files = vec![SnapshotFile { path: PathBuf::from("manifest.toml"), content: manifest_input.into_bytes() }];
    for entry in entries {
        let name = entry.file_name();
        if name == OsStr::new("manifest.toml") {
            continue;
        }
        let relative = PathBuf::from(&name);
        let path = root.join(&name);
        let file_type = entry
            .file_type()
            .map_err(|source| SnapshotImportError::InspectEntry { path: path.clone(), source })?;
        let Some(directory) = name.to_str() else {
            return Err(SnapshotImportError::UnknownEntry { path: relative });
        };
        if !RECORD_DIRECTORIES.contains(&directory) {
            return Err(SnapshotImportError::UnknownEntry { path: relative });
        }
        if !file_type.is_dir() {
            return Err(SnapshotImportError::WrongEntryType { path: relative });
        }
        read_record_directory(root, directory, &mut records, &mut files)?;
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(CandidateSnapshot { records, files })
}

fn read_record_directory(
    root: &Path, directory: &str, records: &mut BTreeMap<PathBuf, SnapshotRecord>, files: &mut Vec<SnapshotFile>,
) -> Result<()> {
    for entry in read_entries(&root.join(directory))? {
        let name = entry.file_name();
        let relative = Path::new(directory).join(&name);
        let path = root.join(&relative);
        let file_type = entry
            .file_type()
            .map_err(|source| SnapshotImportError::InspectEntry { path: path.clone(), source })?;
        if !file_type.is_file() {
            return Err(SnapshotImportError::UnknownEntry { path: relative });
        }
        let Some(filename) = name.to_str() else {
            return Err(SnapshotImportError::UnknownEntry { path: relative });
        };
        if !filename.ends_with(".md") {
            return Err(SnapshotImportError::UnknownEntry { path: relative });
        }
        let input = read_utf8_file(&path, &relative)?;
        let record = decode_record(&relative, &input)
            .map_err(|source| SnapshotImportError::Record { path: relative.clone(), source })?;
        files.push(SnapshotFile { path: relative.clone(), content: input.into_bytes() });
        if records.insert(relative.clone(), record).is_some() {
            return Err(SnapshotImportError::UnknownEntry { path: relative });
        }
    }
    Ok(())
}

fn validate_candidate(
    records: BTreeMap<PathBuf, SnapshotRecord>, project: Project, _worktree_root: &Path,
) -> Result<ConnectedGraph> {
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
    validate_dependency_cycles(&dependencies, &index)?;

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

    Ok(ConnectedGraph {
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
    })
}

fn values<T: Clone>(records: &HashMap<impl std::hash::Hash + Eq, (T, PathBuf)>) -> Vec<T> {
    records.values().map(|(value, _)| value.clone()).collect()
}

fn validate_captures(
    records: &BTreeMap<PathBuf, SnapshotRecord>, index: &RecordIndex,
) -> Result<Vec<CapturePromotion>> {
    let mut promotions = Vec::new();
    for (path, record) in records {
        let SnapshotRecord::Capture(record) = record else { continue };
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
    records: &BTreeMap<PathBuf, SnapshotRecord>, index: &RecordIndex, promotions: &[CapturePromotion],
) -> Result<()> {
    for (path, record) in records {
        let SnapshotRecord::Spec(record) = record else { continue };
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

fn validate_plans(records: &BTreeMap<PathBuf, SnapshotRecord>, index: &RecordIndex) -> Result<()> {
    for (path, record) in records {
        if let SnapshotRecord::Plan(record) = record
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

fn validate_phases(records: &BTreeMap<PathBuf, SnapshotRecord>, index: &RecordIndex) -> Result<()> {
    for (path, record) in records {
        if let SnapshotRecord::Phase(record) = record
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

fn validate_tasks(records: &BTreeMap<PathBuf, SnapshotRecord>, index: &RecordIndex) -> Result<Vec<TaskDependency>> {
    let mut dependencies = Vec::new();
    for (path, record) in records {
        let SnapshotRecord::Task(record) = record else { continue };
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
            dependencies.push(
                TaskDependency::new(record.id, *blocker_id)
                    .map_err(|error| relationship_error(path.clone(), "blocked-by", error.to_string()))?,
            );
        }
    }
    Ok(dependencies)
}

fn validate_releases(
    records: &BTreeMap<PathBuf, SnapshotRecord>, index: &RecordIndex, project_id: ProjectId,
) -> Result<Vec<ReleaseMembership>> {
    let mut memberships = Vec::new();
    for (path, record) in records {
        let SnapshotRecord::Release(record) = record else { continue };
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
    records: &BTreeMap<PathBuf, SnapshotRecord>, index: &RecordIndex, project_id: ProjectId,
) -> Result<Vec<RecordLink>> {
    let mut links = Vec::new();
    let mut seen = HashSet::new();
    for (path, record) in records {
        let (source_kind, source_id, references) = match record {
            SnapshotRecord::Capture(record) => (
                LinkedRecordKind::Capture,
                record.id.to_string(),
                record.links.as_slice(),
            ),
            SnapshotRecord::Spec(record) => (LinkedRecordKind::Spec, record.id.to_string(), record.links.as_slice()),
            SnapshotRecord::Plan(record) => (LinkedRecordKind::Plan, record.id.to_string(), record.links.as_slice()),
            SnapshotRecord::Phase(record) => (LinkedRecordKind::Phase, record.id.to_string(), record.links.as_slice()),
            SnapshotRecord::Task(record) => (LinkedRecordKind::Task, record.id.to_string(), record.links.as_slice()),
            SnapshotRecord::Note(record) => (LinkedRecordKind::Note, record.id.to_string(), record.links.as_slice()),
            SnapshotRecord::Release(record) => (
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

fn validate_dependency_cycles(dependencies: &[TaskDependency], index: &RecordIndex) -> Result<()> {
    fn visit(
        current: TaskId, start: TaskId, graph: &HashMap<TaskId, Vec<TaskId>>, active: &mut HashSet<TaskId>,
        visited: &mut HashSet<TaskId>,
    ) -> bool {
        if !active.insert(current) {
            return current == start;
        }
        if !visited.insert(current) {
            active.remove(&current);
            return false;
        }
        let cycle = graph
            .get(&current)
            .into_iter()
            .flatten()
            .copied()
            .any(|next| visit(next, start, graph, active, visited));
        active.remove(&current);
        cycle
    }
    let mut graph = HashMap::<TaskId, Vec<TaskId>>::new();
    for dependency in dependencies {
        graph.entry(dependency.task_id).or_default().push(dependency.blocker_id);
    }
    for id in index.tasks.keys().copied() {
        if visit(id, id, &graph, &mut HashSet::new(), &mut HashSet::new()) {
            return Err(relationship_error(
                index.task_path(id),
                "blocked-by",
                format!("dependency cycle includes task `{id}`"),
            ));
        }
    }
    Ok(())
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
fn field_error(path: PathBuf, field: impl Into<String>, message: impl Into<String>) -> SnapshotImportError {
    SnapshotImportError::Field { path, field: field.into(), message: message.into() }
}
fn relationship_error(
    path: PathBuf, relationship: impl Into<String>, message: impl Into<String>,
) -> SnapshotImportError {
    SnapshotImportError::Relationship { path, relationship: relationship.into(), message: message.into() }
}
fn read_entries(path: &Path) -> Result<Vec<fs::DirEntry>> {
    let entries =
        fs::read_dir(path).map_err(|source| SnapshotImportError::ReadDirectory { path: path.to_owned(), source })?;
    let mut entries = entries
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|source| SnapshotImportError::ReadDirectory { path: path.to_owned(), source })?;
    entries.sort_by_key(|entry| entry.file_name().to_string_lossy().into_owned());
    Ok(entries)
}
fn read_utf8_file(path: &Path, relative: &Path) -> Result<String> {
    let bytes =
        fs::read(path).map_err(|source| SnapshotImportError::ReadDirectory { path: path.to_owned(), source })?;
    String::from_utf8(bytes).map_err(|_| SnapshotImportError::NonUtf8 { path: relative.to_owned() })
}
fn reject_removed_base_records(base: &[SnapshotBaseFile], files: &[SnapshotFile]) -> Result<()> {
    let current_paths = files.iter().map(|file| file.path.clone()).collect::<HashSet<_>>();
    for base_file in base {
        let path = PathBuf::from(&base_file.path);
        if let Some(id) = record_id_from_path(&path)
            && !current_paths.contains(&path)
        {
            return Err(SnapshotImportError::ExistingRecordRemoved { path, id });
        }
    }
    Ok(())
}
fn reject_removed_snapshot_records(existing: &[SnapshotFile], candidate: &[SnapshotFile]) -> Result<()> {
    let candidate_paths = candidate.iter().map(|file| &file.path).collect::<HashSet<_>>();
    for file in existing {
        if let Some(id) = record_id_from_path(&file.path)
            && !candidate_paths.contains(&file.path)
        {
            return Err(SnapshotImportError::ExistingRecordRemoved { path: file.path.clone(), id });
        }
    }
    Ok(())
}
fn reject_divergent_changes(
    base: &[SnapshotBaseFile], current: &[SnapshotFile], candidate: &[SnapshotFile],
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
            return Err(SnapshotImportError::Conflict { path: file.path.clone() });
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
    use crate::snapshot::export::export_graph;
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
        import_graph(root.path(), root.path(), &mut destination).expect("import succeeds");
        let graph = destination.connected_graph().expect("graph loads");
        assert_eq!(graph.specs[0].body, "Spec body");
        assert_eq!(graph.plans[0].body, "Plan body");
        assert_eq!(graph.tasks[0].body, "Task body");
        assert_eq!(graph.tasks[0].id, task.id);
        assert_eq!(destination.snapshot_base().expect("base loads").len(), 4);
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
        let error = import_graph(root.path(), root.path(), &mut database).expect_err("missing spec rejects");
        assert!(error.to_string().contains("spec"));
        assert!(database.spec(existing.id).expect("spec reads").is_some());
    }
}
