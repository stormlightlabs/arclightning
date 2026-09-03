use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use thiserror::Error;

use arcl_core::domain::{CapturePromotionTarget, LinkedRecordKind};
use arcl_store::{ConnectedGraph, SnapshotBaseFile};

use super::{
    CaptureRecord, NoteRecord, PhaseRecord, PlanRecord, ReleaseRecord, SnapshotManifest, SnapshotRecord,
    SnapshotReference, SpecRecord, TaskRecord, encode_manifest, encode_record,
};

/// A filesystem failure or edit conflict raised while exporting a snapshot.
#[derive(Debug, Error)]
pub enum SnapshotExportError {
    /// A snapshot path could not be created or inspected.
    #[error("could not inspect snapshot path `{path}`: {source}")]
    Inspect { path: PathBuf, source: io::Error },
    /// A snapshot file could not be read.
    #[error("could not read snapshot file `{path}`: {source}")]
    Read { path: PathBuf, source: io::Error },
    /// A snapshot file could not be written.
    #[error("could not write snapshot file `{path}`: {source}")]
    Write { path: PathBuf, source: io::Error },
    /// A filesystem edit would overwrite bytes changed since the last base.
    #[error("snapshot file `{path}` was changed outside arcl; refusing to overwrite it")]
    Conflict { path: PathBuf },
    /// A generated record could not be encoded.
    #[error("could not encode snapshot file `{path}`: {source}")]
    Encode {
        path: PathBuf,
        source: super::SnapshotError,
    },
}

/// A single canonical file produced by a snapshot projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotFile {
    /// The path relative to the snapshot root.
    pub path: PathBuf,
    /// The canonical UTF-8 file contents.
    pub content: Vec<u8>,
}

/// Render every connected record and explicit relationship in canonical order.
pub fn project_graph(graph: &ConnectedGraph) -> Result<Vec<SnapshotFile>, SnapshotExportError> {
    let mut files = Vec::new();
    files.push(SnapshotFile {
        path: PathBuf::from("manifest.toml"),
        content: encode_manifest(&SnapshotManifest::default())
            .map_err(|source| SnapshotExportError::Encode { path: PathBuf::from("manifest.toml"), source })?
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
            SnapshotRecord::Capture(CaptureRecord {
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
            SnapshotRecord::Release(ReleaseRecord {
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
            SnapshotRecord::Spec(SpecRecord {
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
            SnapshotRecord::Plan(PlanRecord {
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
            SnapshotRecord::Phase(PhaseRecord {
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
            SnapshotRecord::Task(TaskRecord {
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
                links: links_for(&links, LinkedRecordKind::Task, task.id.to_string()),
                body: task.body.clone(),
            }),
        )?;
    }
    for note in &graph.notes {
        push_record(
            &mut files,
            SnapshotRecord::Note(NoteRecord {
                id: note.id,
                title: note.title.clone(),
                links: links_for(&links, LinkedRecordKind::Note, note.id.to_string()),
                body: note.body.clone(),
            }),
        )?;
    }
    files.sort_by_key(|file| (file.path != Path::new("manifest.toml"), file.path.clone()));
    Ok(files)
}

/// Render and atomically preflight an export against the last stored snapshot base.
///
/// A changed working-tree file is overwritten only when it is unchanged from the
/// stored base (or already contains the canonical bytes). All conflicts are
/// found before any generated file is replaced.
pub fn export_graph_with_base(
    root: &Path, graph: &ConnectedGraph, base: &[SnapshotBaseFile],
) -> Result<Vec<SnapshotFile>, SnapshotExportError> {
    let files = project_graph(graph)?;
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
            return Err(SnapshotExportError::Conflict { path: file.path.clone() });
        }
    }
    write_files(root, &files, &existing)
}

/// Render and export without a stored base. Existing differing files are conflicts.
pub fn export_graph(root: &Path, graph: &ConnectedGraph) -> Result<Vec<SnapshotFile>, SnapshotExportError> {
    export_graph_with_base(root, graph, &[])
}

pub(crate) fn export_observed(
    root: &Path, files: &[SnapshotFile], observed: &[SnapshotFile],
) -> Result<(), SnapshotExportError> {
    let observed = observed
        .iter()
        .map(|file| (&file.path, &file.content))
        .collect::<HashMap<_, _>>();
    for file in observed.keys() {
        let path = root.join(file);
        let current = fs::read(&path).map_err(|source| SnapshotExportError::Read { path: path.clone(), source })?;
        if observed[file].as_slice() != current.as_slice() {
            return Err(SnapshotExportError::Conflict { path: (*file).clone() });
        }
    }
    let existing = existing_files(root, files)?;
    write_files(root, files, &existing)?;
    Ok(())
}

pub(crate) fn remove_legacy_files(
    root: &Path, observed: &[SnapshotFile], canonical: &[SnapshotFile],
) -> Result<(), SnapshotExportError> {
    let canonical_paths = canonical
        .iter()
        .map(|file| &file.path)
        .collect::<std::collections::HashSet<_>>();
    for file in observed {
        if canonical_paths.contains(&file.path) {
            continue;
        }
        let path = root.join(&file.path);
        if path.exists() {
            fs::remove_file(&path).map_err(|source| SnapshotExportError::Write { path, source })?;
        }
    }
    Ok(())
}

fn push_record(files: &mut Vec<SnapshotFile>, record: SnapshotRecord) -> Result<(), SnapshotExportError> {
    let path = record.path();
    let content = encode_record(&path, &record)
        .map_err(|source| SnapshotExportError::Encode { path: path.clone(), source })?
        .into_bytes();
    files.push(SnapshotFile { path, content });
    Ok(())
}

fn link_map(graph: &ConnectedGraph) -> HashMap<(LinkedRecordKind, String), Vec<SnapshotReference>> {
    let mut map = HashMap::new();
    for link in &graph.links {
        map.entry((link.source_kind, link.source_id.clone()))
            .or_insert_with(Vec::new)
            .push(SnapshotReference::new(
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
    links: &HashMap<(LinkedRecordKind, String), Vec<SnapshotReference>>, kind: LinkedRecordKind, id: String,
) -> Vec<SnapshotReference> {
    links.get(&(kind, id)).cloned().unwrap_or_default()
}

fn membership_map(graph: &ConnectedGraph) -> HashMap<arcl_core::domain::ReleaseId, Vec<SnapshotReference>> {
    let mut map = HashMap::new();
    for membership in &graph.release_memberships {
        map.entry(membership.release_id)
            .or_insert_with(Vec::new)
            .push(SnapshotReference::new(
                membership.record_kind.as_str(),
                membership.record_id.clone(),
            ));
    }
    for references in map.values_mut() {
        references.sort();
    }
    map
}

fn promotion_reference(target: CapturePromotionTarget) -> SnapshotReference {
    SnapshotReference::new(target.promotion_kind(), target.promotion_target_id())
}

fn nonempty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

fn existing_files(root: &Path, files: &[SnapshotFile]) -> Result<HashMap<PathBuf, Vec<u8>>, SnapshotExportError> {
    if root.exists() {
        let metadata = fs::symlink_metadata(root)
            .map_err(|source| SnapshotExportError::Inspect { path: root.to_owned(), source })?;
        if !metadata.is_dir() {
            return Err(SnapshotExportError::Inspect {
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
                return Err(SnapshotExportError::Inspect {
                    path,
                    source: io::Error::from(io::ErrorKind::InvalidInput),
                });
            }
            Ok(_) => {
                let content =
                    fs::read(&path).map_err(|source| SnapshotExportError::Read { path: path.clone(), source })?;
                existing.insert(file.path.clone(), content);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(source) => return Err(SnapshotExportError::Inspect { path, source }),
        }
    }
    Ok(existing)
}

fn validate_path_components(root: &Path, relative: &Path) -> Result<(), SnapshotExportError> {
    let mut path = root.to_owned();
    for component in relative.components() {
        if let std::path::Component::Normal(component) = component {
            path.push(component);
            if fs::symlink_metadata(&path)
                .map(|metadata| metadata.file_type().is_symlink())
                .unwrap_or(false)
            {
                return Err(SnapshotExportError::Inspect {
                    path,
                    source: io::Error::from(io::ErrorKind::InvalidInput),
                });
            }
        }
    }
    Ok(())
}

fn write_files(
    root: &Path, files: &[SnapshotFile], existing: &HashMap<PathBuf, Vec<u8>>,
) -> Result<Vec<SnapshotFile>, SnapshotExportError> {
    for file in files {
        validate_path_components(root, &file.path)?;
        let path = root.join(&file.path);
        let current = match fs::read(&path) {
            Ok(content) => Some(content),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(source) => return Err(SnapshotExportError::Read { path, source }),
        };
        if current.as_ref() != existing.get(&file.path) {
            return Err(SnapshotExportError::Conflict { path: file.path.clone() });
        }
    }
    fs::create_dir_all(root).map_err(|source| SnapshotExportError::Inspect { path: root.to_owned(), source })?;
    for directory in ["captures", "releases", "specs", "plans", "phases", "tasks", "notes"] {
        let path = root.join(directory);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(SnapshotExportError::Inspect {
                    path,
                    source: io::Error::from(io::ErrorKind::InvalidInput),
                });
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&path).map_err(|source| SnapshotExportError::Write { path: path.clone(), source })?;
            }
            Err(source) => return Err(SnapshotExportError::Inspect { path, source }),
        }
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let staging = root.with_extension(format!("export-{nonce}"));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|source| SnapshotExportError::Write { path: staging.clone(), source })?;
    }
    fs::create_dir_all(&staging).map_err(|source| SnapshotExportError::Write { path: staging.clone(), source })?;
    let result = (|| {
        for file in files {
            if existing.get(&file.path) == Some(&file.content) {
                continue;
            }
            let path = staging.join(&file.path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|source| SnapshotExportError::Write { path: parent.to_owned(), source })?;
            }
            fs::write(&path, &file.content).map_err(|source| SnapshotExportError::Write { path, source })?;
        }
        for file in files {
            if existing.get(&file.path) == Some(&file.content) {
                continue;
            }
            let staged = staging.join(&file.path);
            let destination = root.join(&file.path);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .map_err(|source| SnapshotExportError::Write { path: parent.to_owned(), source })?;
            }
            fs::rename(&staged, &destination)
                .map_err(|source| SnapshotExportError::Write { path: destination, source })?;
        }
        Ok(())
    })();
    let cleanup = fs::remove_dir_all(&staging);
    result.and(cleanup.map_err(|source| SnapshotExportError::Write { path: staging, source }))?;
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
}
