use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use thiserror::Error;

use arcl_core::domain::{CapturePromotionTarget, LinkedRecordKind};
use arcl_store::{ConnectedGraph, WorkspaceBaseFile};

use super::{
    ASSIGNEE_FORMAT_VERSION, CaptureRecord, NoteRecord, OLDEST_WORKSPACE_FORMAT_VERSION, PhaseRecord, PlanRecord,
    ReleaseRecord, SpecRecord, TaskRecord, WorkspaceManifest, WorkspaceRecord, WorkspaceReference, encode_manifest,
    encode_record,
};

const MANIFEST: &str = "manifest.toml";

/// A filesystem failure or edit conflict raised while exporting a workspace.
#[derive(Debug, Error)]
pub enum WorkspaceExportError {
    /// A workspace path could not be created or inspected.
    #[error("could not inspect workspace path `{path}`: {source}")]
    Inspect { path: PathBuf, source: io::Error },
    /// A workspace file could not be read.
    #[error("could not read workspace file `{path}`: {source}")]
    Read { path: PathBuf, source: io::Error },
    /// A workspace file could not be written.
    #[error("could not write workspace file `{path}`: {source}")]
    Write { path: PathBuf, source: io::Error },
    /// A filesystem edit would overwrite bytes changed since the last base.
    #[error("workspace file `{path}` was changed outside arcl; refusing to overwrite it")]
    Conflict { path: PathBuf },
    /// A generated record could not be encoded.
    #[error("could not encode workspace file `{path}`: {source}")]
    Encode {
        path: PathBuf,
        source: super::WorkspaceError,
    },
}

/// A single canonical file produced by a workspace projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceFile {
    /// The path relative to the workspace root.
    pub path: PathBuf,
    /// The canonical UTF-8 file contents.
    pub content: Vec<u8>,
}

/// Return the workspace format version `graph` needs:
/// [`ASSIGNEE_FORMAT_VERSION`] when a task carries an assignee, and
/// [`OLDEST_WORKSPACE_FORMAT_VERSION`] otherwise.
pub fn required_format_version(graph: &ConnectedGraph) -> u32 {
    if graph.tasks.iter().any(|task| task.assignee.is_some()) {
        ASSIGNEE_FORMAT_VERSION
    } else {
        OLDEST_WORKSPACE_FORMAT_VERSION
    }
}

/// Render every connected record and explicit relationship in canonical
/// order, with the manifest at the version `graph` needs.
///
/// A caller comparing against an existing workspace uses
/// [`project_graph_at`] instead, so the manifest keeps the version it has.
pub fn project_graph(graph: &ConnectedGraph) -> Result<Vec<WorkspaceFile>, WorkspaceExportError> {
    project_graph_at(graph, OLDEST_WORKSPACE_FORMAT_VERSION)
}

/// Render as [`project_graph`] does, with the manifest at `floor` or the
/// version `graph` needs, whichever is higher.
///
/// `floor` is the version the workspace already has, so a projection never
/// lowers it: a workspace stays at version 2 once a claim has moved it there.
pub fn project_graph_at(graph: &ConnectedGraph, floor: u32) -> Result<Vec<WorkspaceFile>, WorkspaceExportError> {
    let manifest = WorkspaceManifest { format_version: required_format_version(graph).max(floor) };
    let mut files = Vec::new();
    files.push(WorkspaceFile {
        path: PathBuf::from(MANIFEST),
        content: encode_manifest(&manifest)
            .map_err(|source| WorkspaceExportError::Encode { path: PathBuf::from(MANIFEST), source })?
            .into_bytes(),
    });

    let links = link_map(graph);
    let promotions = graph
        .capture_promotions
        .iter()
        .map(|promotion| (promotion.capture_id, promotion.target))
        .collect::<HashMap<_, _>>();
    let memberships = membership_map(graph);

    for capture in &graph.captures {
        let promoted_to = promotions.get(&capture.id).copied().map(promotion_reference);
        push_record(
            &mut files,
            WorkspaceRecord::Capture(CaptureRecord {
                id: capture.id,
                title: capture.title.clone(),
                status: capture.status,
                created_at: capture.created_at.clone(),
                promoted_to,
                links: links_for(&links, LinkedRecordKind::Capture, capture.id.to_string()),
                body: capture.body.clone(),
            }),
        )?;
    }
    for release in &graph.releases {
        push_record(
            &mut files,
            WorkspaceRecord::Release(ReleaseRecord {
                id: release.id,
                title: release.title.clone(),
                status: release.status,
                body: release.description.clone(),
                members: memberships.get(&release.id).cloned().unwrap_or_default(),
                links: links_for(&links, LinkedRecordKind::Release, release.id.to_string()),
            }),
        )?;
    }
    for spec in &graph.specs {
        push_record(
            &mut files,
            WorkspaceRecord::Spec(SpecRecord {
                id: spec.id,
                title: spec.title.clone(),
                status: spec.status,
                source_capture_id: spec.source_capture_id,
                acceptance_criteria: spec.acceptance_criteria.clone(),
                links: links_for(&links, LinkedRecordKind::Spec, spec.id.to_string()),
                body: spec.body.clone(),
            }),
        )?;
    }
    for plan in &graph.plans {
        push_record(
            &mut files,
            WorkspaceRecord::Plan(PlanRecord {
                id: plan.id,
                spec_id: plan.spec_id,
                title: plan.title.clone(),
                status: plan.status,
                links: links_for(&links, LinkedRecordKind::Plan, plan.id.to_string()),
                body: plan.body.clone(),
            }),
        )?;
    }
    for phase in &graph.phases {
        push_record(
            &mut files,
            WorkspaceRecord::Phase(PhaseRecord {
                id: phase.id,
                plan_id: phase.plan_id,
                plan_key: phase.plan_key.clone(),
                title: phase.title.clone(),
                status: phase.status,
                position: phase.position,
                links: links_for(&links, LinkedRecordKind::Phase, phase.id.to_string()),
                body: phase.body.clone(),
            }),
        )?;
    }
    for task in &graph.tasks {
        push_record(
            &mut files,
            WorkspaceRecord::Task(TaskRecord {
                id: task.id,
                spec_id: task.spec_id,
                plan_id: task.plan_id,
                phase_id: task.phase_id,
                parent_id: task.parent_id,
                plan_key: task.plan_key.clone(),
                title: task.title.clone(),
                status: task.status,
                priority: task.priority,
                position: task.position,
                blocked_by: graph
                    .dependencies
                    .iter()
                    .filter(|dependency| dependency.task_id == task.id)
                    .map(|dependency| dependency.blocker_id)
                    .collect(),
                handoff: nonempty(&task.handoff),
                evidence: nonempty(&task.evidence),
                assignee: task.assignee.clone(),
                links: links_for(&links, LinkedRecordKind::Task, task.id.to_string()),
                body: task.body.clone(),
            }),
        )?;
    }
    for note in &graph.notes {
        push_record(
            &mut files,
            WorkspaceRecord::Note(NoteRecord {
                id: note.id,
                title: note.title.clone(),
                links: links_for(&links, LinkedRecordKind::Note, note.id.to_string()),
                body: note.body.clone(),
            }),
        )?;
    }
    files.sort_by_key(|file| (file.path != Path::new(MANIFEST), file.path.clone()));
    Ok(files)
}

/// Render and atomically preflight an export against the last stored workspace base.
///
/// A changed working-tree file is overwritten only when it is unchanged from the
/// stored base (or already contains the canonical bytes). All conflicts are
/// found before any generated file is replaced.
pub fn export_graph_with_base(
    root: &Path, graph: &ConnectedGraph, base: &[WorkspaceBaseFile],
) -> Result<Vec<WorkspaceFile>, WorkspaceExportError> {
    let on_disk = fs::read(root.join(MANIFEST))
        .ok()
        .map(|content| WorkspaceFile { path: PathBuf::from(MANIFEST), content });
    let files = project_graph_at(graph, format_floor(base, on_disk.as_slice()))?;
    let base = base
        .iter()
        .map(|file| (PathBuf::from(&file.path), file))
        .collect::<HashMap<_, _>>();
    let existing = existing_files(root, &files)?;
    for file in &files {
        let Some(current) = existing.get(&file.path) else { continue };
        if current == &file.content {
            continue;
        }
        if base
            .get(&file.path)
            .is_some_and(|base_file| base_file.content == *current)
        {
            continue;
        }
        if base.contains_key(&file.path) || !current.is_empty() {
            return Err(WorkspaceExportError::Conflict { path: file.path.clone() });
        }
    }
    write_files(root, &files, &existing)
}

/// Render and export without a stored base. Existing differing files are conflicts.
pub fn export_graph(root: &Path, graph: &ConnectedGraph) -> Result<Vec<WorkspaceFile>, WorkspaceExportError> {
    export_graph_with_base(root, graph, &[])
}

/// Return the highest format version among the manifests in `base` and
/// `files`, or [`OLDEST_WORKSPACE_FORMAT_VERSION`] when neither holds a
/// readable one. Pass the result to [`project_graph_at`] so a projection keeps
/// the version the workspace already has.
pub(super) fn format_floor(base: &[WorkspaceBaseFile], files: &[WorkspaceFile]) -> u32 {
    let version = |content: &[u8]| {
        std::str::from_utf8(content)
            .ok()
            .and_then(|input| WorkspaceManifest::parse(input).ok())
            .map(|manifest| manifest.format_version)
    };
    let from_base = base
        .iter()
        .filter(|file| Path::new(&file.path) == Path::new(MANIFEST))
        .filter_map(|file| version(&file.content));
    let from_files = files
        .iter()
        .filter(|file| file.path == Path::new(MANIFEST))
        .filter_map(|file| version(&file.content));
    from_base
        .chain(from_files)
        .max()
        .unwrap_or(OLDEST_WORKSPACE_FORMAT_VERSION)
}

/// Write `files` only if the workspace still matches what a sync observed.
///
/// Every observed path must still hold its observed bytes, and every other
/// path in `files` must still be absent. A file edited or created since the
/// workspace was read is a [`WorkspaceExportError::Conflict`].
pub(crate) fn export_observed(
    root: &Path, files: &[WorkspaceFile], observed: &[WorkspaceFile],
) -> Result<(), WorkspaceExportError> {
    let observed = observed
        .iter()
        .map(|file| (&file.path, &file.content))
        .collect::<HashMap<_, _>>();
    for file in observed.keys() {
        let path = root.join(file);
        let current = fs::read(&path).map_err(|source| WorkspaceExportError::Read { path: path.clone(), source })?;
        if observed[file].as_slice() != current.as_slice() {
            return Err(WorkspaceExportError::Conflict { path: (*file).clone() });
        }
    }
    let existing = existing_files(root, files)?;
    if let Some(created) = files
        .iter()
        .find(|file| !observed.contains_key(&file.path) && existing.contains_key(&file.path))
    {
        return Err(WorkspaceExportError::Conflict { path: created.path.clone() });
    }
    write_files(root, files, &existing)?;
    Ok(())
}

fn push_record(files: &mut Vec<WorkspaceFile>, record: WorkspaceRecord) -> Result<(), WorkspaceExportError> {
    let path = record.path();
    let content = encode_record(&path, &record)
        .map_err(|source| WorkspaceExportError::Encode { path: path.clone(), source })?
        .into_bytes();
    files.push(WorkspaceFile { path, content });
    Ok(())
}

fn link_map(graph: &ConnectedGraph) -> HashMap<(LinkedRecordKind, String), Vec<WorkspaceReference>> {
    let mut map = HashMap::new();
    for link in &graph.links {
        map.entry((link.source_kind, link.source_id.clone()))
            .or_insert_with(Vec::new)
            .push(WorkspaceReference::new(
                link.target_kind.as_str(),
                link.target_id.clone(),
            ));
    }
    for references in map.values_mut() {
        references.sort();
    }
    map
}

fn links_for(
    links: &HashMap<(LinkedRecordKind, String), Vec<WorkspaceReference>>, kind: LinkedRecordKind, id: String,
) -> Vec<WorkspaceReference> {
    links.get(&(kind, id)).cloned().unwrap_or_default()
}

fn membership_map(graph: &ConnectedGraph) -> HashMap<arcl_core::domain::ReleaseId, Vec<WorkspaceReference>> {
    let mut map = HashMap::new();
    for membership in &graph.release_memberships {
        map.entry(membership.release_id)
            .or_insert_with(Vec::new)
            .push(WorkspaceReference::new(
                membership.record_kind.as_str(),
                membership.record_id.clone(),
            ));
    }
    for references in map.values_mut() {
        references.sort();
    }
    map
}

fn promotion_reference(target: CapturePromotionTarget) -> WorkspaceReference {
    WorkspaceReference::new(target.promotion_kind(), target.promotion_target_id())
}

fn nonempty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

fn existing_files(root: &Path, files: &[WorkspaceFile]) -> Result<HashMap<PathBuf, Vec<u8>>, WorkspaceExportError> {
    if root.exists() {
        let metadata = fs::symlink_metadata(root)
            .map_err(|source| WorkspaceExportError::Inspect { path: root.to_owned(), source })?;
        if !metadata.is_dir() {
            return Err(WorkspaceExportError::Inspect {
                path: root.to_owned(),
                source: io::Error::from(io::ErrorKind::AlreadyExists),
            });
        }
    }
    let mut existing = HashMap::new();
    for file in files {
        validate_path_components(root, &file.path)?;
        let path = root.join(&file.path);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(WorkspaceExportError::Inspect {
                    path,
                    source: io::Error::from(io::ErrorKind::InvalidInput),
                });
            }
            Ok(_) => {
                let content =
                    fs::read(&path).map_err(|source| WorkspaceExportError::Read { path: path.clone(), source })?;
                existing.insert(file.path.clone(), content);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(source) => return Err(WorkspaceExportError::Inspect { path, source }),
        }
    }
    Ok(existing)
}

fn validate_path_components(root: &Path, relative: &Path) -> Result<(), WorkspaceExportError> {
    let mut path = root.to_owned();
    for component in relative.components() {
        if let std::path::Component::Normal(component) = component {
            path.push(component);
            if fs::symlink_metadata(&path)
                .map(|metadata| metadata.file_type().is_symlink())
                .unwrap_or(false)
            {
                return Err(WorkspaceExportError::Inspect {
                    path,
                    source: io::Error::from(io::ErrorKind::InvalidInput),
                });
            }
        }
    }
    Ok(())
}

fn write_files(
    root: &Path, files: &[WorkspaceFile], existing: &HashMap<PathBuf, Vec<u8>>,
) -> Result<Vec<WorkspaceFile>, WorkspaceExportError> {
    for file in files {
        validate_path_components(root, &file.path)?;
        let path = root.join(&file.path);
        let current = match fs::read(&path) {
            Ok(content) => Some(content),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(source) => return Err(WorkspaceExportError::Read { path, source }),
        };
        if current.as_ref() != existing.get(&file.path) {
            return Err(WorkspaceExportError::Conflict { path: file.path.clone() });
        }
    }
    fs::create_dir_all(root).map_err(|source| WorkspaceExportError::Inspect { path: root.to_owned(), source })?;
    for directory in ["captures", "releases", "specs", "plans", "phases", "tasks", "notes"] {
        let path = root.join(directory);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(WorkspaceExportError::Inspect {
                    path,
                    source: io::Error::from(io::ErrorKind::InvalidInput),
                });
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&path).map_err(|source| WorkspaceExportError::Write { path: path.clone(), source })?;
            }
            Err(source) => return Err(WorkspaceExportError::Inspect { path, source }),
        }
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let staging = root.with_extension(format!("export-{nonce}"));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|source| WorkspaceExportError::Write { path: staging.clone(), source })?;
    }
    fs::create_dir_all(&staging).map_err(|source| WorkspaceExportError::Write { path: staging.clone(), source })?;
    let result = (|| {
        for file in files {
            if existing.get(&file.path) == Some(&file.content) {
                continue;
            }
            let path = staging.join(&file.path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|source| WorkspaceExportError::Write { path: parent.to_owned(), source })?;
            }
            fs::write(&path, &file.content).map_err(|source| WorkspaceExportError::Write { path, source })?;
        }
        for file in files {
            if existing.get(&file.path) == Some(&file.content) {
                continue;
            }
            let staged = staging.join(&file.path);
            let destination = root.join(&file.path);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .map_err(|source| WorkspaceExportError::Write { path: parent.to_owned(), source })?;
            }
            fs::rename(&staged, &destination)
                .map_err(|source| WorkspaceExportError::Write { path: destination, source })?;
        }
        Ok(())
    })();
    let cleanup = fs::remove_dir_all(&staging);
    result.and(cleanup.map_err(|source| WorkspaceExportError::Write { path: staging, source }))?;
    Ok(files.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcl_core::domain::{LinkedRecordKind, ReleaseMemberKind, TaskPriority};
    use arcl_store::{Database, PlanningTaskCreate};

    #[test]
    fn projects_the_connected_graph_and_keeps_unchanged_bytes_stable() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project = database.project().expect("project exists");
        let capture = database
            .create_capture("Capture".to_owned(), "Capture body".to_owned())
            .expect("capture creates");
        let spec = database
            .create_spec("Spec".to_owned(), "Spec body".to_owned(), "- [ ] criterion".to_owned())
            .expect("spec creates");
        let plan = database
            .create_plan(spec.id, "Plan".to_owned(), "Plan body".to_owned())
            .expect("plan creates");
        let phase = database
            .create_phase(plan.id, "Phase".to_owned(), "Phase body".to_owned(), 0)
            .expect("phase creates");
        let blocker = database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: Some(spec.id),
                plan_id: Some(plan.id),
                phase_id: Some(phase.id),
                parent_id: None,
                title: "Blocker".to_owned(),
                body: "Blocker body".to_owned(),
                priority: TaskPriority::Normal,
                position: 0,
            })
            .expect("blocker creates");
        let task = database
            .create_planning_task_with_dependencies(
                PlanningTaskCreate {
                    project_id: project.id,
                    spec_id: Some(spec.id),
                    plan_id: Some(plan.id),
                    phase_id: Some(phase.id),
                    parent_id: None,
                    title: "Task".to_owned(),
                    body: "Task body".to_owned(),
                    priority: TaskPriority::High,
                    position: 1,
                },
                &[blocker.id],
            )
            .expect("task creates");
        let note = database
            .create_note("Note".to_owned(), "Note body".to_owned())
            .expect("note creates");
        let release = database
            .create_release("Release".to_owned(), "Release body".to_owned())
            .expect("release creates");
        database
            .add_release_membership(release.id, ReleaseMemberKind::Spec, spec.id.to_string())
            .expect("membership creates");
        database
            .add_release_membership(release.id, ReleaseMemberKind::Task, task.id.to_string())
            .expect("membership creates");
        database
            .add_note_link(note.id, LinkedRecordKind::Capture, capture.id.to_string())
            .expect("link creates");

        let graph = database.connected_graph().expect("graph loads");
        let files = project_graph(&graph).expect("projection succeeds");
        assert_eq!(
            files.first().map(|file| file.path.as_path()),
            Some(Path::new("manifest.toml"))
        );
        assert!(
            files
                .iter()
                .any(|file| file.path == Path::new(&format!("captures/{}.md", capture.id)))
        );
        assert!(
            files
                .iter()
                .any(|file| file.path == Path::new(&format!("specs/{}.md", spec.id)))
        );
        let root = tempfile::tempdir().expect("workspace creates");
        export_graph(root.path(), &graph).expect("export succeeds");
        let task_path = root.path().join(format!("tasks/{}.md", task.id));
        let before = fs::read(&task_path).expect("task file reads");
        export_graph(root.path(), &graph).expect("repeat export succeeds");
        assert_eq!(fs::read(task_path).expect("task file reads"), before);
    }

    #[test]
    fn an_observed_export_refuses_a_file_created_after_the_workspace_was_read() {
        let root = tempfile::tempdir().expect("workspace creates");
        let manifest =
            WorkspaceFile { path: PathBuf::from("manifest.toml"), content: b"format-version = 1\n".to_vec() };
        fs::write(root.path().join(&manifest.path), &manifest.content).expect("manifest writes");
        let note = WorkspaceFile { path: PathBuf::from("notes/new.md"), content: b"from the database\n".to_vec() };
        fs::create_dir(root.path().join("notes")).expect("notes directory creates");
        fs::write(root.path().join(&note.path), "created by hand\n").expect("note writes");

        let error = export_observed(root.path(), &[manifest.clone(), note.clone()], &[manifest])
            .expect_err("a created file refuses");
        assert!(matches!(&error, WorkspaceExportError::Conflict { path } if path == &note.path));
        assert_eq!(
            fs::read_to_string(root.path().join(&note.path)).expect("note reads"),
            "created by hand\n"
        );
    }
}
