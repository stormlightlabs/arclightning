use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ffi::OsStr,
    fs, io,
    path::{Component, Path, PathBuf},
};

use thiserror::Error;

use crate::{
    domain::{EpicId, TaskDependency, TaskId},
    storage::{Database, Graph, SnapshotBaseFile, StorageError},
};

use super::{
    EpicRecord, IdeaRecord, MilestoneRecord, ReleaseRecord, SnapshotError, SnapshotFile, SnapshotRecord, TaskRecord,
    decode_manifest, decode_record, export::export_observed, project_graph,
};

const SNAPSHOT_DIRECTORIES: &[&str] = &["ideas", "releases", "epics", "milestones", "tasks"];

/// A validation or I/O failure raised while importing a complete snapshot.
#[derive(Debug, Error)]
pub enum SnapshotImportError {
    /// The snapshot root or one of its record directories could not be read.
    #[error("could not read snapshot directory `{path}`: {source}")]
    ReadDirectory { path: PathBuf, source: io::Error },
    /// A directory entry could not be inspected.
    #[error("could not inspect snapshot entry `{path}`: {source}")]
    InspectEntry { path: PathBuf, source: io::Error },
    /// The required manifest was not present.
    #[error("snapshot manifest `{path}` is missing")]
    MissingManifest { path: PathBuf },
    /// A file or directory is outside the v1 snapshot layout.
    #[error("snapshot entry `{path}` is not allowed in the v1 snapshot layout")]
    UnknownEntry { path: PathBuf },
    /// A known snapshot path is not a regular file or directory as required.
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
    /// A spec path does not identify a regular Markdown file inside the worktree.
    #[error("snapshot file `{path}`: field `spec-path` `{spec_path}`: {message}")]
    SpecPath {
        path: PathBuf,
        spec_path: String,
        message: String,
    },
    /// An ID represented in the stored synchronization base was removed or renamed.
    #[error("snapshot file `{path}`: record ID `{id}` was removed or renamed; v1 has no hard deletion")]
    ExistingRecordRemoved { path: PathBuf, id: String },
    /// The complete replacement could not be committed to SQLite.
    #[error(transparent)]
    Storage(#[from] StorageError),
    /// Canonical snapshot normalization failed before the database replacement.
    #[error(transparent)]
    Export(#[from] super::SnapshotExportError),
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
            | Self::SpecPath { .. }
            | Self::ExistingRecordRemoved { .. } => 3,
        }
    }
}

type Result<T> = std::result::Result<T, SnapshotImportError>;

struct CandidateSnapshot {
    records: BTreeMap<PathBuf, SnapshotRecord>,
    files: Vec<SnapshotFile>,
}

struct PreparedImport {
    graph: Graph,
    files: Vec<SnapshotFile>,
    observed: Vec<SnapshotFile>,
}

/// Parse, validate, replace, and normalize a snapshot using an open database.
///
/// All filesystem parsing and complete-graph validation happen before the SQLite
/// replacement transaction is opened. Canonical formatting is written only while
/// the parsed files remain unchanged, then the graph and synchronization base are
/// committed together.
pub fn import_graph(root: &Path, worktree_root: &Path, database: &mut Database) -> Result<Vec<SnapshotFile>> {
    let prepared = prepare_import(root, worktree_root)?;
    apply_prepared(root, prepared, database)
}

/// Import a snapshot while opening the database only after semantic validation.
///
/// This entry point lets a clone rebuild an absent database without creating or
/// migrating SQLite state when the snapshot itself is invalid.
pub fn import_snapshot(root: &Path, worktree_root: &Path, database_path: &Path) -> Result<Vec<SnapshotFile>> {
    let prepared = prepare_import(root, worktree_root)?;
    let mut database = Database::open(database_path)?;
    apply_prepared(root, prepared, &mut database)
}

fn prepare_import(root: &Path, worktree_root: &Path) -> Result<PreparedImport> {
    let candidate = read_candidate(root)?;
    let observed = candidate.files.clone();
    let graph = validate_candidate(candidate, worktree_root)?;
    let files = project_graph(&graph).map_err(|source| SnapshotImportError::Field {
        path: PathBuf::from("manifest.toml"),
        field: "canonical rendering".to_owned(),
        message: source.to_string(),
    })?;
    Ok(PreparedImport { graph, files, observed })
}

fn apply_prepared(root: &Path, prepared: PreparedImport, database: &mut Database) -> Result<Vec<SnapshotFile>> {
    let current_base = database.snapshot_base()?;
    reject_removed_base_records(&current_base, &prepared.files)?;
    let current_files = project_graph(&database.graph()?).map_err(|source| SnapshotImportError::Field {
        path: PathBuf::from("manifest.toml"),
        field: "current database projection".to_owned(),
        message: source.to_string(),
    })?;
    reject_removed_snapshot_records(&current_files, &prepared.files)?;
    let base = prepared
        .files
        .iter()
        .map(|file| SnapshotBaseFile { path: file.path.to_string_lossy().into_owned(), content: file.content.clone() })
        .collect::<Vec<_>>();

    export_observed(root, &prepared.files, &prepared.observed)?;
    database.replace_graph_and_snapshot_base(&prepared.graph, &base)?;
    Ok(prepared.files)
}

fn read_candidate(root: &Path) -> Result<CandidateSnapshot> {
    let entries = read_entries(root)?;
    let mut manifest = None;
    let mut records = BTreeMap::new();
    let mut files = Vec::new();

    for entry in entries {
        let name = entry.file_name();
        let relative = PathBuf::from(&name);
        let path = root.join(&name);
        let file_type = entry
            .file_type()
            .map_err(|source| SnapshotImportError::InspectEntry { path: path.clone(), source })?;
        if name == OsStr::new("manifest.toml") {
            if !file_type.is_file() {
                return Err(SnapshotImportError::WrongEntryType { path: relative });
            }
            let input = read_utf8_file(&path, &relative)?;
            manifest = Some(
                decode_manifest(&input)
                    .map_err(|source| SnapshotImportError::Manifest { path: relative.clone(), source })?,
            );
            files.push(SnapshotFile { path: relative, content: input.into_bytes() });
            continue;
        }

        let Some(directory) = name.to_str() else {
            return Err(SnapshotImportError::UnknownEntry { path: relative });
        };
        if !SNAPSHOT_DIRECTORIES.contains(&directory) {
            return Err(SnapshotImportError::UnknownEntry { path: relative });
        }
        if !file_type.is_dir() {
            return Err(SnapshotImportError::WrongEntryType { path: relative });
        }
        read_record_directory(root, directory, &mut records, &mut files)?;
    }

    if manifest.is_none() {
        return Err(SnapshotImportError::MissingManifest { path: root.join("manifest.toml") });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(CandidateSnapshot { records, files })
}

fn read_record_directory(
    root: &Path, directory: &str, records: &mut BTreeMap<PathBuf, SnapshotRecord>, files: &mut Vec<SnapshotFile>,
) -> Result<()> {
    let directory_path = root.join(directory);
    for entry in read_entries(&directory_path)? {
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
        records.insert(relative, record);
    }
    Ok(())
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

fn validate_candidate(candidate: CandidateSnapshot, worktree_root: &Path) -> Result<Graph> {
    let mut ids = HashMap::<String, PathBuf>::new();
    let mut sources = HashMap::<String, PathBuf>::new();
    for (path, record) in &candidate.records {
        let id = record.id_string();
        if let Some(first_path) = ids.insert(id.clone(), path.clone()) {
            return Err(field_error(
                path.clone(),
                "id",
                format!(
                    "record ID `{id}` is duplicated; first declared in `{}`",
                    first_path.display()
                ),
            ));
        }
        sources.insert(id, path.clone());
    }

    let mut graph = Graph {
        ideas: Vec::new(),
        releases: Vec::new(),
        epics: Vec::new(),
        milestones: Vec::new(),
        tasks: Vec::new(),
        dependencies: Vec::new(),
    };
    let mut task_blockers = Vec::<(TaskId, Vec<TaskId>)>::new();
    for record in candidate.records.values() {
        match record {
            SnapshotRecord::Idea(record) => graph.ideas.push(idea_from_record(record)),
            SnapshotRecord::Release(record) => graph.releases.push(release_from_record(record)),
            SnapshotRecord::Epic(record) => graph.epics.push(epic_from_record(record)),
            SnapshotRecord::Milestone(record) => graph.milestones.push(milestone_from_record(record)),
            SnapshotRecord::Task(record) => {
                graph.tasks.push(task_from_record(record));
                task_blockers.push((record.id, record.blocked_by.clone()));
            }
        }
    }

    for epic in &mut graph.epics {
        epic.spec_path = normalize_spec_path(worktree_root, source_for(&sources, epic.id), &epic.spec_path)?;
    }

    validate_relationships(&graph, &sources, &task_blockers)?;
    for (task_id, blockers) in task_blockers {
        for blocker_id in blockers {
            let dependency = TaskDependency::new(task_id, blocker_id).map_err(|source| {
                relationship_error(source_for(&sources, task_id), "blocked-by", source.to_string())
            })?;
            graph.dependencies.push(dependency);
        }
    }
    graph
        .dependencies
        .sort_by_key(|dependency| (dependency.task_id, dependency.blocker_id));
    Ok(graph)
}

fn validate_relationships(
    graph: &Graph, sources: &HashMap<String, PathBuf>, task_blockers: &[(TaskId, Vec<TaskId>)],
) -> Result<()> {
    let ideas = graph
        .ideas
        .iter()
        .map(|record| (record.id, record))
        .collect::<HashMap<_, _>>();
    let releases = graph
        .releases
        .iter()
        .map(|record| (record.id, record))
        .collect::<HashMap<_, _>>();
    let epics = graph
        .epics
        .iter()
        .map(|record| (record.id, record))
        .collect::<HashMap<_, _>>();
    let milestones = graph
        .milestones
        .iter()
        .map(|record| (record.id, record))
        .collect::<HashMap<_, _>>();
    let tasks = graph
        .tasks
        .iter()
        .map(|record| (record.id, record))
        .collect::<HashMap<_, _>>();

    let mut spec_paths = HashMap::<String, (EpicId, PathBuf)>::new();
    for epic in &graph.epics {
        if let Some((first_id, first_path)) =
            spec_paths.insert(epic.spec_path.clone(), (epic.id, source_for(sources, epic.id)))
        {
            return Err(field_error(
                source_for(sources, epic.id),
                "spec-path",
                format!(
                    "specification path `{}` is already used by epic `{first_id}` in `{}`",
                    epic.spec_path,
                    first_path.display()
                ),
            ));
        }
        if let Some(release_id) = epic.release_id
            && !releases.contains_key(&release_id)
        {
            return Err(relationship_error(
                source_for(sources, epic.id),
                "release",
                format!("release `{release_id}` does not exist"),
            ));
        }
        if let Some(idea_id) = epic.source_idea
            && !ideas.contains_key(&idea_id)
        {
            return Err(relationship_error(
                source_for(sources, epic.id),
                "source-idea",
                format!("idea `{idea_id}` does not exist"),
            ));
        }
    }

    for idea in &graph.ideas {
        match (idea.status, idea.promoted_to) {
            (crate::domain::IdeaStatus::Promoted, Some(epic_id)) => {
                let Some(epic) = epics.get(&epic_id) else {
                    return Err(relationship_error(
                        source_for(sources, idea.id),
                        "promoted-to",
                        format!("epic `{epic_id}` does not exist"),
                    ));
                };
                if epic.source_idea != Some(idea.id) {
                    return Err(relationship_error(
                        source_for(sources, idea.id),
                        "promoted-to/source-idea",
                        format!("epic `{epic_id}` does not point back to idea `{}`", idea.id),
                    ));
                }
            }
            (crate::domain::IdeaStatus::Promoted, None) => {
                return Err(field_error(
                    source_for(sources, idea.id),
                    "status",
                    "promoted ideas must have exactly one `promoted-to` epic".to_owned(),
                ));
            }
            (_, Some(epic_id)) => {
                return Err(relationship_error(
                    source_for(sources, idea.id),
                    "promoted-to",
                    format!(
                        "idea status `{}` cannot point to epic `{epic_id}`",
                        idea.status.as_str()
                    ),
                ));
            }
            _ => {}
        }
    }
    for epic in &graph.epics {
        if let Some(idea_id) = epic.source_idea {
            let Some(idea) = ideas.get(&idea_id) else {
                continue;
            };
            if idea.status != crate::domain::IdeaStatus::Promoted || idea.promoted_to != Some(epic.id) {
                return Err(relationship_error(
                    source_for(sources, epic.id),
                    "source-idea/promoted-to",
                    format!("idea `{idea_id}` does not promote to epic `{}`", epic.id),
                ));
            }
        }
    }

    let mut milestone_keys = HashMap::<(EpicId, String), PathBuf>::new();
    for milestone in &graph.milestones {
        if !epics.contains_key(&milestone.epic_id) {
            return Err(relationship_error(
                source_for(sources, milestone.id),
                "epic",
                format!("epic `{}` does not exist", milestone.epic_id),
            ));
        }
        if let Some(plan_key) = &milestone.plan_key {
            if plan_key.trim().is_empty() {
                return Err(field_error(
                    source_for(sources, milestone.id),
                    "plan-key",
                    "plan keys cannot be blank".to_owned(),
                ));
            }
            if let Some(first_path) =
                milestone_keys.insert((milestone.epic_id, plan_key.clone()), source_for(sources, milestone.id))
            {
                return Err(field_error(
                    source_for(sources, milestone.id),
                    "plan-key",
                    format!(
                        "plan key `{plan_key}` is duplicated; first declared in `{}`",
                        first_path.display()
                    ),
                ));
            }
        }
    }

    let mut task_keys = HashMap::<(crate::domain::MilestoneId, String), PathBuf>::new();
    for task in &graph.tasks {
        if !milestones.contains_key(&task.milestone_id) {
            return Err(relationship_error(
                source_for(sources, task.id),
                "milestone",
                format!("milestone `{}` does not exist", task.milestone_id),
            ));
        }
        if let Some(parent_id) = task.parent_id {
            let Some(parent) = tasks.get(&parent_id) else {
                return Err(relationship_error(
                    source_for(sources, task.id),
                    "parent",
                    format!("parent task `{parent_id}` does not exist"),
                ));
            };
            if parent_id == task.id {
                return Err(relationship_error(
                    source_for(sources, task.id),
                    "parent",
                    "a task cannot be its own parent".to_owned(),
                ));
            }
            if parent.milestone_id != task.milestone_id {
                return Err(relationship_error(
                    source_for(sources, task.id),
                    "parent",
                    format!("parent `{parent_id}` belongs to a different milestone"),
                ));
            }
        }
        if let Some(plan_key) = &task.plan_key {
            if plan_key.trim().is_empty() {
                return Err(field_error(
                    source_for(sources, task.id),
                    "plan-key",
                    "plan keys cannot be blank".to_owned(),
                ));
            }
            if let Some(first_path) =
                task_keys.insert((task.milestone_id, plan_key.clone()), source_for(sources, task.id))
            {
                return Err(field_error(
                    source_for(sources, task.id),
                    "plan-key",
                    format!(
                        "plan key `{plan_key}` is duplicated; first declared in `{}`",
                        first_path.display()
                    ),
                ));
            }
        }
    }

    let mut dependencies = HashSet::<(TaskId, TaskId)>::new();
    for (task_id, blockers) in task_blockers {
        for blocker_id in blockers {
            if !tasks.contains_key(blocker_id) {
                return Err(relationship_error(
                    source_for(sources, *task_id),
                    "blocked-by",
                    format!("blocker task `{blocker_id}` does not exist"),
                ));
            }
            if !dependencies.insert((*task_id, *blocker_id)) {
                return Err(relationship_error(
                    source_for(sources, *task_id),
                    "blocked-by",
                    format!("dependency to `{blocker_id}` is duplicated"),
                ));
            }
            if task_id == blocker_id {
                return Err(relationship_error(
                    source_for(sources, *task_id),
                    "blocked-by",
                    "a task cannot depend on itself".to_owned(),
                ));
            }
        }
    }

    validate_parent_cycles(graph, sources)?;
    validate_dependency_cycles(graph, sources, task_blockers)
}

fn validate_parent_cycles(graph: &Graph, sources: &HashMap<String, PathBuf>) -> Result<()> {
    let parents = graph
        .tasks
        .iter()
        .map(|task| (task.id, task.parent_id))
        .collect::<HashMap<_, _>>();
    for task in &graph.tasks {
        let mut seen = HashSet::new();
        let mut current = Some(task.id);
        while let Some(task_id) = current {
            if !seen.insert(task_id) {
                return Err(relationship_error(
                    source_for(sources, task.id),
                    "parent",
                    format!("parent chain reaches task `{task_id}` more than once"),
                ));
            }
            current = parents.get(&task_id).copied().flatten();
        }
    }
    Ok(())
}

fn validate_dependency_cycles(
    graph: &Graph, sources: &HashMap<String, PathBuf>, task_blockers: &[(TaskId, Vec<TaskId>)],
) -> Result<()> {
    let blockers = task_blockers.iter().cloned().collect::<HashMap<_, _>>();
    for task in &graph.tasks {
        let mut seen = HashSet::new();
        let mut pending = vec![task.id];
        while let Some(task_id) = pending.pop() {
            if !seen.insert(task_id) {
                if task_id == task.id {
                    return Err(relationship_error(
                        source_for(sources, task.id),
                        "blocked-by",
                        format!("dependency cycle includes task `{task_id}`"),
                    ));
                }
                continue;
            }
            if let Some(next) = blockers.get(&task_id) {
                pending.extend(next.iter().copied());
            }
        }
    }
    Ok(())
}

fn normalize_spec_path(worktree_root: &Path, source: PathBuf, spec_path: &str) -> Result<String> {
    let path = Path::new(spec_path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(SnapshotImportError::SpecPath {
            path: source,
            spec_path: spec_path.to_owned(),
            message: "path must stay relative to the worktree and cannot contain `..`".to_owned(),
        });
    }
    let canonical_root = fs::canonicalize(worktree_root).map_err(|source_error| SnapshotImportError::SpecPath {
        path: source.clone(),
        spec_path: spec_path.to_owned(),
        message: format!("worktree root could not be resolved: {source_error}"),
    })?;
    let candidate = canonical_root.join(path);
    let resolved = fs::canonicalize(&candidate).map_err(|source_error| SnapshotImportError::SpecPath {
        path: source.clone(),
        spec_path: spec_path.to_owned(),
        message: format!("file could not be resolved: {source_error}"),
    })?;
    if !resolved.starts_with(&canonical_root) {
        return Err(SnapshotImportError::SpecPath {
            path: source,
            spec_path: spec_path.to_owned(),
            message: "resolved outside the worktree".to_owned(),
        });
    }
    let metadata = fs::metadata(&resolved).map_err(|source_error| SnapshotImportError::SpecPath {
        path: source.clone(),
        spec_path: spec_path.to_owned(),
        message: format!("metadata could not be read: {source_error}"),
    })?;
    if !metadata.is_file() {
        return Err(SnapshotImportError::SpecPath {
            path: source,
            spec_path: spec_path.to_owned(),
            message: "path is not a regular file".to_owned(),
        });
    }
    if resolved.extension() != Some(OsStr::new("md")) {
        return Err(SnapshotImportError::SpecPath {
            path: source,
            spec_path: spec_path.to_owned(),
            message: "file must end in `.md`".to_owned(),
        });
    }
    let relative = resolved
        .strip_prefix(&canonical_root)
        .map_err(|_| SnapshotImportError::SpecPath {
            path: source.clone(),
            spec_path: spec_path.to_owned(),
            message: "resolved outside the worktree".to_owned(),
        })?;
    relative
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| SnapshotImportError::SpecPath {
            path: source,
            spec_path: spec_path.to_owned(),
            message: "resolved path is not valid UTF-8".to_owned(),
        })
}

fn reject_removed_base_records(base: &[SnapshotBaseFile], files: &[SnapshotFile]) -> Result<()> {
    let current_paths = files.iter().map(|file| file.path.clone()).collect::<HashSet<_>>();
    for base_file in base {
        let path = PathBuf::from(&base_file.path);
        let Some(id) = record_id_from_path(&path) else { continue };
        if !current_paths.contains(&path) {
            return Err(SnapshotImportError::ExistingRecordRemoved { path, id });
        }
    }
    Ok(())
}

fn reject_removed_snapshot_records(existing: &[SnapshotFile], candidate: &[SnapshotFile]) -> Result<()> {
    let candidate_paths = candidate.iter().map(|file| &file.path).collect::<HashSet<_>>();
    for file in existing {
        let Some(id) = record_id_from_path(&file.path) else { continue };
        if !candidate_paths.contains(&file.path) {
            return Err(SnapshotImportError::ExistingRecordRemoved { path: file.path.clone(), id });
        }
    }
    Ok(())
}

fn record_id_from_path(path: &Path) -> Option<String> {
    let mut components = path.components();
    let Component::Normal(directory) = components.next()? else { return None };
    let Component::Normal(filename) = components.next()? else { return None };
    if components.next().is_some()
        || !SNAPSHOT_DIRECTORIES
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

fn source_for(sources: &HashMap<String, PathBuf>, id: impl ToString) -> PathBuf {
    sources
        .get(&id.to_string())
        .cloned()
        .unwrap_or_else(|| PathBuf::from("<snapshot>"))
}

fn field_error(path: PathBuf, field: impl Into<String>, message: impl Into<String>) -> SnapshotImportError {
    SnapshotImportError::Field { path, field: field.into(), message: message.into() }
}

fn relationship_error(
    path: PathBuf, relationship: impl Into<String>, message: impl Into<String>,
) -> SnapshotImportError {
    SnapshotImportError::Relationship { path, relationship: relationship.into(), message: message.into() }
}

fn idea_from_record(record: &IdeaRecord) -> crate::domain::Idea {
    crate::domain::Idea {
        id: record.id,
        title: record.title.clone(),
        description: record.description.clone(),
        status: record.status,
        promoted_to: record.promoted_to,
    }
}

fn release_from_record(record: &ReleaseRecord) -> crate::domain::Release {
    crate::domain::Release {
        id: record.id,
        title: record.title.clone(),
        description: record.description.clone(),
        status: record.status,
    }
}

fn epic_from_record(record: &EpicRecord) -> crate::domain::Epic {
    crate::domain::Epic {
        id: record.id,
        release_id: record.release,
        title: record.title.clone(),
        description: record.description.clone(),
        spec_path: record.spec_path.clone(),
        status: record.status,
        source_idea: record.source_idea,
    }
}

fn milestone_from_record(record: &MilestoneRecord) -> crate::domain::Milestone {
    crate::domain::Milestone {
        id: record.id,
        epic_id: record.epic,
        title: record.title.clone(),
        description: record.description.clone(),
        status: record.status,
        position: record.position,
        plan_key: record.plan_key.clone(),
    }
}

fn task_from_record(record: &TaskRecord) -> crate::domain::Task {
    crate::domain::Task {
        id: record.id,
        milestone_id: record.milestone,
        parent_id: record.parent,
        title: record.title.clone(),
        description: record.description.clone(),
        status: record.status,
        priority: record.priority,
        position: record.position,
        plan_key: record.plan_key.clone(),
        handoff: record.handoff.clone().unwrap_or_default(),
        evidence: record.evidence.clone().unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::{ContainerStatus, EpicId, IdeaId, IdeaStatus, MilestoneId, TaskPriority, TaskStatus},
        snapshot::SnapshotExportError,
    };
    use std::fs;

    const IDEA_ID: &str = "arcl-i-01K0B3N4QSC9R7K6W8X2M5YH1Z";
    const EPIC_ID: &str = "arcl-e-01K0B2ZWTX7JX9PH7W5G1S6A9Q";
    const MILESTONE_ID: &str = "arcl-m-01K0B2ZWTX7JX9PH7W5G1S6A9Q";
    const TASK_ID: &str = "arcl-t-01K0B31M6VGK4YH8VKT4C0D2DR";
    const SECOND_EPIC_ID: &str = "arcl-e-01K0B2ZWTX7JX9PH7W5G1S6A9R";
    const CHILD_TASK_ID: &str = "arcl-t-01K0B31M6VGK4YH8VKT4C0D2DA";
    const PARENT_TASK_ID: &str = "arcl-t-01K0B31M6VGK4YH8VKT4C0D2DZ";

    fn empty_snapshot() -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("snapshot root creates");
        fs::write(root.path().join("manifest.toml"), "format-version = 1\n").expect("manifest writes");
        root
    }

    fn write_record(root: &Path, path: &str, content: &str) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("record parent exists")).expect("record directory creates");
        fs::write(path, content).expect("record writes");
    }

    fn complete_graph() -> Graph {
        let idea_id = IdeaId::parse(IDEA_ID).expect("idea ID");
        let epic_id = EpicId::parse(EPIC_ID).expect("epic ID");
        let milestone_id = MilestoneId::parse(MILESTONE_ID).expect("milestone ID");
        let task_id = TaskId::parse(TASK_ID).expect("task ID");
        Graph {
            ideas: vec![crate::domain::Idea {
                id: idea_id,
                title: "Idea".to_owned(),
                description: String::new(),
                status: IdeaStatus::Captured,
                promoted_to: None,
            }],
            releases: Vec::new(),
            epics: vec![crate::domain::Epic {
                id: epic_id,
                release_id: None,
                title: "Epic".to_owned(),
                description: String::new(),
                spec_path: "spec.md".to_owned(),
                status: ContainerStatus::Open,
                source_idea: None,
            }],
            milestones: vec![crate::domain::Milestone {
                id: milestone_id,
                epic_id,
                title: "Milestone".to_owned(),
                description: String::new(),
                status: ContainerStatus::Open,
                position: 0,
                plan_key: Some("milestone".to_owned()),
            }],
            tasks: vec![crate::domain::Task {
                id: task_id,
                milestone_id,
                parent_id: None,
                title: "Task".to_owned(),
                description: String::new(),
                status: TaskStatus::Pending,
                priority: TaskPriority::High,
                position: 0,
                plan_key: Some("task".to_owned()),
                handoff: String::new(),
                evidence: String::new(),
            }],
            dependencies: Vec::new(),
        }
    }

    #[test]
    fn empty_snapshot_rebuilds_database_and_refreshes_base() {
        let root = empty_snapshot();
        let worktree = tempfile::tempdir().expect("worktree creates");
        fs::write(worktree.path().join("spec.md"), "# Spec\n").expect("spec writes");
        let mut database = Database::open_in_memory().expect("database opens");

        import_graph(root.path(), worktree.path(), &mut database).expect("empty snapshot imports");

        assert!(database.graph().expect("graph loads").ideas.is_empty());
        assert_eq!(database.snapshot_base().expect("base loads").len(), 1);
    }

    #[test]
    fn invalid_graph_does_not_change_database_or_snapshot_files() {
        let root = empty_snapshot();
        write_record(
            root.path(),
            &format!("epics/{EPIC_ID}.md"),
            &format!("+++\nid = \"{EPIC_ID}\"\ntitle = \"Epic\"\nstatus = \"open\"\nspec-path = \"missing.md\"\n+++\n"),
        );
        let before = fs::read(root.path().join(format!("epics/{EPIC_ID}.md"))).expect("record reads");
        let mut database = Database::open_in_memory().expect("database opens");
        let idea = database
            .create_idea("Existing".to_owned(), String::new())
            .expect("idea creates");
        let error = import_graph(root.path(), root.path(), &mut database).expect_err("missing spec rejects");
        assert!(error.to_string().contains("spec-path"));
        assert!(database.idea(idea.id).expect("idea reads").is_some());
        assert_eq!(
            fs::read(root.path().join(format!("epics/{EPIC_ID}.md"))).expect("record reads"),
            before
        );
    }

    #[test]
    fn empty_snapshot_cannot_remove_an_unbased_database_record() {
        let root = empty_snapshot();
        let worktree = tempfile::tempdir().expect("worktree creates");
        let mut database = Database::open_in_memory().expect("database opens");
        let idea = database
            .create_idea("Existing".to_owned(), String::new())
            .expect("idea creates");

        let error = import_graph(root.path(), worktree.path(), &mut database).expect_err("removal rejects");

        assert!(matches!(error, SnapshotImportError::ExistingRecordRemoved { .. }));
        assert!(database.idea(idea.id).expect("idea reads").is_some());
    }

    #[test]
    fn concurrent_snapshot_edit_aborts_before_database_replacement() {
        let root = empty_snapshot();
        let worktree = tempfile::tempdir().expect("worktree creates");
        fs::write(worktree.path().join("spec.md"), "# Spec\n").expect("spec writes");
        let files = project_graph(&complete_graph()).expect("graph projects");
        for file in &files {
            let path = root.path().join(&file.path);
            fs::create_dir_all(path.parent().expect("record parent exists")).expect("record directory creates");
            fs::write(path, &file.content).expect("snapshot writes");
        }
        let prepared = prepare_import(root.path(), worktree.path()).expect("snapshot prepares");
        let changed_path = root.path().join(format!("ideas/{IDEA_ID}.md"));
        fs::write(&changed_path, "concurrent edit\n").expect("concurrent edit writes");
        let mut database = Database::open_in_memory().expect("database opens");

        let error = apply_prepared(root.path(), prepared, &mut database).expect_err("concurrent edit conflicts");

        assert!(matches!(
            error,
            SnapshotImportError::Export(SnapshotExportError::Conflict { .. })
        ));
        assert!(database.graph().expect("graph loads").ideas.is_empty());
        assert_eq!(
            fs::read_to_string(changed_path).expect("changed file reads"),
            "concurrent edit\n"
        );
    }

    #[test]
    fn normalized_spec_paths_must_be_unique() {
        let root = empty_snapshot();
        let worktree = tempfile::tempdir().expect("worktree creates");
        fs::write(worktree.path().join("spec.md"), "# Spec\n").expect("spec writes");
        let mut graph = complete_graph();
        graph.epics.push(crate::domain::Epic {
            id: EpicId::parse(SECOND_EPIC_ID).expect("epic ID"),
            release_id: None,
            title: "Second epic".to_owned(),
            description: String::new(),
            spec_path: "./spec.md".to_owned(),
            status: ContainerStatus::Open,
            source_idea: None,
        });
        for file in project_graph(&graph).expect("graph projects") {
            let path = root.path().join(&file.path);
            fs::create_dir_all(path.parent().expect("record parent exists")).expect("record directory creates");
            fs::write(path, file.content).expect("snapshot writes");
        }

        let error = match prepare_import(root.path(), worktree.path()) {
            Ok(_) => panic!("duplicate spec should reject"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("already used"));
    }

    #[test]
    fn child_task_can_sort_before_its_parent() {
        let root = empty_snapshot();
        let worktree = tempfile::tempdir().expect("worktree creates");
        fs::write(worktree.path().join("spec.md"), "# Spec\n").expect("spec writes");
        let mut graph = complete_graph();
        let milestone_id = graph.milestones[0].id;
        let child_id = TaskId::parse(CHILD_TASK_ID).expect("child ID");
        let parent_id = TaskId::parse(PARENT_TASK_ID).expect("parent ID");
        graph.tasks = vec![
            crate::domain::Task {
                id: child_id,
                milestone_id,
                parent_id: Some(parent_id),
                title: "Child".to_owned(),
                description: String::new(),
                status: TaskStatus::Pending,
                priority: TaskPriority::Normal,
                position: 0,
                plan_key: None,
                handoff: String::new(),
                evidence: String::new(),
            },
            crate::domain::Task {
                id: parent_id,
                milestone_id,
                parent_id: None,
                title: "Parent".to_owned(),
                description: String::new(),
                status: TaskStatus::Pending,
                priority: TaskPriority::Normal,
                position: 1,
                plan_key: None,
                handoff: String::new(),
                evidence: String::new(),
            },
        ];
        for file in project_graph(&graph).expect("graph projects") {
            let path = root.path().join(&file.path);
            fs::create_dir_all(path.parent().expect("record parent exists")).expect("record directory creates");
            fs::write(path, file.content).expect("snapshot writes");
        }
        let mut database = Database::open_in_memory().expect("database opens");

        import_graph(root.path(), worktree.path(), &mut database).expect("snapshot imports");

        let loaded = database.graph().expect("graph loads");
        let child = loaded
            .tasks
            .iter()
            .find(|task| task.id == child_id)
            .expect("child loads");
        assert_eq!(child.parent_id, Some(parent_id));
    }

    #[test]
    fn concurrent_import_conflicts_use_the_conflict_exit_code() {
        let error =
            SnapshotImportError::Export(SnapshotExportError::Conflict { path: PathBuf::from("ideas/changed.md") });

        assert_eq!(error.exit_code(), 4);
    }

    #[test]
    fn plan_keys_round_trip_through_complete_replacement() {
        let root = empty_snapshot();
        let worktree = tempfile::tempdir().expect("worktree creates");
        fs::write(worktree.path().join("spec.md"), "# Spec\n").expect("spec writes");
        let graph = complete_graph();
        let files = project_graph(&graph).expect("graph projects");
        for file in &files {
            let path = root.path().join(&file.path);
            fs::create_dir_all(path.parent().expect("record parent exists")).expect("record directory creates");
            fs::write(path, &file.content).expect("snapshot writes");
        }
        let mut database = Database::open_in_memory().expect("database opens");
        import_graph(root.path(), worktree.path(), &mut database).expect("snapshot imports");
        let loaded = database.graph().expect("graph loads");
        assert_eq!(loaded.milestones[0].plan_key.as_deref(), Some("milestone"));
        assert_eq!(loaded.tasks[0].plan_key.as_deref(), Some("task"));
    }
}
