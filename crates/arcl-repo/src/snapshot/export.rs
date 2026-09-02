use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use thiserror::Error;

use arcl_core::domain::TaskId;
use arcl_store::Graph;

use super::{
    EpicRecord, IdeaRecord, MilestoneRecord, ReleaseRecord, SnapshotError, SnapshotManifest, SnapshotRecord,
    TaskRecord, encode_manifest, encode_record,
};

/// One canonical file in a snapshot projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotFile {
    /// The path relative to the snapshot root.
    pub path: PathBuf,
    /// The exact bytes that belong in the file.
    pub content: Vec<u8>,
}

/// Failures encountered while writing a snapshot projection.
#[derive(Debug, Error)]
pub enum SnapshotExportError {
    /// The graph could not be encoded by the snapshot codec.
    #[error(transparent)]
    Codec(#[from] SnapshotError),
    /// A snapshot directory could not be created.
    #[error("could not create snapshot directory `{path}`: {source}")]
    CreateDirectory { path: PathBuf, source: io::Error },
    /// A destination could not be read for the start-of-command comparison.
    #[error("could not read snapshot destination `{path}`: {source}")]
    ReadDestination { path: PathBuf, source: io::Error },
    /// A destination changed after it was captured and before replacement.
    #[error("snapshot destination `{path}` changed during export")]
    Conflict { path: PathBuf },
    /// A same-directory temporary file could not be created.
    #[error("could not create temporary snapshot file `{path}`: {source}")]
    CreateTemporary { path: PathBuf, source: io::Error },
    /// A same-directory temporary file could not be written or flushed.
    #[error("could not write temporary snapshot file `{path}`: {source}")]
    WriteTemporary { path: PathBuf, source: io::Error },
    /// A temporary file could not be atomically renamed into place.
    #[error("could not atomically replace snapshot file `{path}`: {source}")]
    Rename { path: PathBuf, source: io::Error },
}

/// Project the complete graph into deterministic manifest and record files.
pub fn project_graph(graph: &Graph) -> Result<Vec<SnapshotFile>, SnapshotError> {
    let mut files = vec![SnapshotFile {
        path: PathBuf::from("manifest.toml"),
        content: encode_manifest(&SnapshotManifest::default())?.into_bytes(),
    }];

    let mut ideas = graph.ideas.iter().collect::<Vec<_>>();
    ideas.sort_by_key(|idea| idea.id);
    for idea in ideas {
        files.push(encode_snapshot_record(SnapshotRecord::Idea(IdeaRecord {
            id: idea.id,
            title: idea.title.clone(),
            status: idea.status,
            promoted_to: idea.promoted_to,
            description: idea.description.clone(),
        }))?);
    }

    let mut releases = graph.releases.iter().collect::<Vec<_>>();
    releases.sort_by_key(|release| release.id);
    for release in releases {
        files.push(encode_snapshot_record(SnapshotRecord::Release(ReleaseRecord {
            id: release.id,
            title: release.title.clone(),
            status: release.status,
            description: release.description.clone(),
        }))?);
    }

    let mut epics = graph.epics.iter().collect::<Vec<_>>();
    epics.sort_by_key(|epic| epic.id);
    for epic in epics {
        files.push(encode_snapshot_record(SnapshotRecord::Epic(EpicRecord {
            id: epic.id,
            title: epic.title.clone(),
            status: epic.status,
            spec_path: epic.spec_path.clone(),
            release: epic.release_id,
            source_idea: epic.source_idea,
            description: epic.description.clone(),
        }))?);
    }

    let mut milestones = graph.milestones.iter().collect::<Vec<_>>();
    milestones.sort_by_key(|milestone| milestone.id);
    for milestone in milestones {
        files.push(encode_snapshot_record(SnapshotRecord::Milestone(MilestoneRecord {
            id: milestone.id,
            title: milestone.title.clone(),
            status: milestone.status,
            epic: milestone.epic_id,
            position: milestone.position,
            plan_key: milestone.plan_key.clone(),
            description: milestone.description.clone(),
        }))?);
    }

    let mut blockers = BTreeMap::<TaskId, Vec<TaskId>>::new();
    for dependency in &graph.dependencies {
        blockers
            .entry(dependency.task_id)
            .or_default()
            .push(dependency.blocker_id);
    }
    for task_blockers in blockers.values_mut() {
        task_blockers.sort();
    }

    let mut tasks = graph.tasks.iter().collect::<Vec<_>>();
    tasks.sort_by_key(|task| task.id);
    for task in tasks {
        files.push(encode_snapshot_record(SnapshotRecord::Task(TaskRecord {
            id: task.id,
            title: task.title.clone(),
            status: task.status,
            priority: task.priority,
            milestone: task.milestone_id,
            position: task.position,
            parent: task.parent_id,
            plan_key: task.plan_key.clone(),
            blocked_by: blockers.remove(&task.id).unwrap_or_default(),
            handoff: non_empty(task.handoff.clone()),
            evidence: non_empty(task.evidence.clone()),
            description: task.description.clone(),
        }))?);
    }

    files[1..].sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

/// Export a graph with same-directory temporary files and atomic per-file renames.
pub fn export_graph(root: &Path, graph: &Graph) -> Result<Vec<SnapshotFile>, SnapshotExportError> {
    export_graph_with_hook(root, graph, &mut |_| {})
}

/// Write a canonical projection only if every destination still matches the
/// exact files observed while an import was parsed.
pub(crate) fn export_observed(
    root: &Path, files: &[SnapshotFile], observed: &[SnapshotFile],
) -> Result<(), SnapshotExportError> {
    let observed = observed
        .iter()
        .map(|file| (file.path.clone(), Some(file.content.clone())))
        .collect::<BTreeMap<_, _>>();
    export_files(root, files, &observed, &mut |_| {})
}

fn export_graph_with_hook<F>(
    root: &Path, graph: &Graph, before_recheck: &mut F,
) -> Result<Vec<SnapshotFile>, SnapshotExportError>
where
    F: FnMut(&Path),
{
    let files = project_graph(graph)?;
    let observed = capture_destinations(root, &files)?;

    export_files(root, &files, &observed, before_recheck)?;
    Ok(files)
}

fn export_files<F>(
    root: &Path, files: &[SnapshotFile], observed: &BTreeMap<PathBuf, Option<Vec<u8>>>, before_recheck: &mut F,
) -> Result<(), SnapshotExportError>
where
    F: FnMut(&Path),
{
    ensure_directory(root)?;
    for directory in ["ideas", "releases", "epics", "milestones", "tasks"] {
        let path = root.join(directory);
        ensure_directory(&path)?;
    }

    for file in files {
        let destination = root.join(&file.path);
        before_recheck(&file.path);
        let current = read_destination(&destination)?;
        if current != observed.get(&file.path).cloned().flatten() {
            return Err(SnapshotExportError::Conflict { path: file.path.clone() });
        }
        if current.as_ref() == Some(&file.content) {
            continue;
        }
        write_atomically(&destination, &file.content)?;
    }

    Ok(())
}

fn ensure_directory(path: &Path) -> Result<(), SnapshotExportError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(SnapshotExportError::CreateDirectory {
            path: path.to_owned(),
            source: io::Error::other("path exists but is not a directory"),
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => fs::create_dir_all(path)
            .map_err(|source| SnapshotExportError::CreateDirectory { path: path.to_owned(), source }),
        Err(source) => Err(SnapshotExportError::CreateDirectory { path: path.to_owned(), source }),
    }
}

fn encode_snapshot_record(record: SnapshotRecord) -> Result<SnapshotFile, SnapshotError> {
    let path = record.path();
    let content = encode_record(&path, &record)?.into_bytes();
    Ok(SnapshotFile { path, content })
}

fn non_empty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn capture_destinations(
    root: &Path, files: &[SnapshotFile],
) -> Result<BTreeMap<PathBuf, Option<Vec<u8>>>, SnapshotExportError> {
    files
        .iter()
        .map(|file| Ok((file.path.clone(), read_destination(&root.join(&file.path))?)))
        .collect()
}

fn read_destination(path: &Path) -> Result<Option<Vec<u8>>, SnapshotExportError> {
    match fs::read(path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(SnapshotExportError::ReadDestination { path: path.to_owned(), source }),
    }
}

fn write_atomically(path: &Path, content: &[u8]) -> Result<(), SnapshotExportError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().and_then(|name| name.to_str()).unwrap_or("snapshot");
    let process_id = std::process::id();
    let mut attempt = 0_u32;

    loop {
        let temporary = parent.join(format!(".{file_name}.arcl-tmp-{process_id}-{attempt}"));
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&temporary) {
            Ok(file) => file,
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {
                attempt = attempt.saturating_add(1);
                continue;
            }
            Err(source) => return Err(SnapshotExportError::CreateTemporary { path: temporary, source }),
        };

        let result = (|| {
            file.write_all(content)
                .map_err(|source| SnapshotExportError::WriteTemporary { path: temporary.clone(), source })?;
            file.flush()
                .map_err(|source| SnapshotExportError::WriteTemporary { path: temporary.clone(), source })?;
            file.sync_all()
                .map_err(|source| SnapshotExportError::WriteTemporary { path: temporary.clone(), source })?;
            fs::rename(&temporary, path).map_err(|source| SnapshotExportError::Rename { path: path.to_owned(), source })
        })();

        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        return result;
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use arcl_store::Database;

    #[test]
    fn export_projection_sorts_records_and_dependencies() {
        let mut database = Database::open_in_memory().expect("database opens");
        let idea = database
            .create_idea("Idea".to_owned(), String::new())
            .expect("idea creates");
        let release = database
            .create_release("Release".to_owned(), String::new())
            .expect("release creates");
        let epic = database
            .create_epic("Epic".to_owned(), String::new(), "spec.md".to_owned(), Some(release.id))
            .expect("epic creates");
        let milestone = database
            .create_milestone(epic.id, "Milestone".to_owned(), String::new(), 0)
            .expect("milestone creates");
        let blocker = database
            .create_task(
                milestone.id,
                None,
                "Blocker".to_owned(),
                String::new(),
                arcl_core::domain::TaskPriority::Normal,
                0,
            )
            .expect("blocker creates");
        let task = database
            .create_task(
                milestone.id,
                None,
                "Task".to_owned(),
                String::new(),
                arcl_core::domain::TaskPriority::Normal,
                1,
            )
            .expect("task creates");
        database
            .add_dependency(task.id, blocker.id)
            .expect("dependency creates");

        let files = project_graph(&database.graph().expect("graph loads")).expect("projection succeeds");
        assert_eq!(files[0].path, PathBuf::from("manifest.toml"));
        assert!(
            files
                .iter()
                .any(|file| file.path == Path::new(format!("ideas/{}.md", idea.id).as_str()))
        );
        let task_file = files
            .iter()
            .find(|file| file.path == Path::new(format!("tasks/{}.md", task.id).as_str()))
            .expect("task file exists");
        let task_text = String::from_utf8(task_file.content.clone()).expect("task is UTF-8");
        assert!(task_text.contains(&format!("blocked-by = [\"{}\"]", blocker.id)));
    }

    #[test]
    fn concurrent_change_is_reported_before_replacement() {
        let mut database = Database::open_in_memory().expect("database opens");
        let idea = database
            .create_idea("Idea".to_owned(), String::new())
            .expect("idea creates");
        let root = tempfile::tempdir().expect("snapshot directory creates");
        let path = root.path().join(format!("ideas/{}.md", idea.id));
        let mut changed = false;
        let error = export_graph_with_hook(root.path(), &database.graph().expect("graph loads"), &mut |relative| {
            if !changed && relative == Path::new("ideas").join(format!("{}.md", idea.id)) {
                fs::write(&path, "concurrent edit").expect("concurrent edit writes");
                changed = true;
            }
        })
        .expect_err("concurrent edit conflicts");
        assert!(matches!(error, SnapshotExportError::Conflict { .. }));
    }

    #[cfg(unix)]
    #[test]
    fn export_rejects_symlinked_snapshot_directories() {
        use std::os::unix::fs::symlink;

        let mut database = Database::open_in_memory().expect("database opens");
        database
            .create_idea("Idea".to_owned(), String::new())
            .expect("idea creates");
        let root = tempfile::tempdir().expect("snapshot directory creates");
        let outside = tempfile::tempdir().expect("outside directory creates");
        symlink(outside.path(), root.path().join("ideas")).expect("symlink creates");

        let error = export_graph(root.path(), &database.graph().expect("graph loads"))
            .expect_err("symlinked directory rejects");

        assert!(matches!(error, SnapshotExportError::CreateDirectory { .. }));
        assert!(fs::read_dir(outside.path()).expect("outside reads").next().is_none());
    }
}
