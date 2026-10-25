//! Reading and saving one record's workspace file, as the web app's raw
//! record editor does.
//!
//! The file is the record's canonical projection, the same bytes a workspace
//! export writes. A save is checked as a workspace import checks a file: it
//! must parse, match the record's kind and ID, and keep every relationship
//! and blocker rule of the complete graph. Its revision must also match the
//! file it was edited from, and it must leave alone the fields that only a
//! command changes; see [`edit_record`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use thiserror::Error;

use arcl_store::{ConnectedGraph, Database, StorageError};

use super::import::validate_candidate;
use super::{
    RecordKind, WorkspaceExportError, WorkspaceFile, WorkspaceImportError, WorkspaceRecord, decode_record,
    project_graph,
};

/// Why a raw edit cannot change a task's status.
const TASK_STATUS: &str = "a task's status changes only through `arcl task start`, `park`, `unpark`, `handoff`, \
                           `complete` or `cancel`, or the web app's board and task actions, which check readiness \
                           and open subtasks";

/// A failure to read or save one record's file.
#[derive(Debug, Error)]
pub enum RecordEditError {
    /// No record has this ID.
    #[error("record `{id}` was not found")]
    NotFound { id: String },
    /// The record's file changed since the revision the edit started from.
    #[error(
        "workspace file `{path}` changed since it was opened; reload it and make the edit again",
        path = .path.display()
    )]
    Stale { path: PathBuf },
    /// The edit changes a field that only a command changes, such as a
    /// task's `status`.
    #[error("workspace file `{path}`: field `{field}`: {message}", path = .path.display())]
    CommandOwned {
        path: PathBuf,
        field: &'static str,
        message: &'static str,
    },
    /// The edited file failed the import's checks.
    #[error(transparent)]
    Invalid(Box<WorkspaceImportError>),
    /// The graph could not be rendered as workspace files.
    #[error(transparent)]
    Export(Box<WorkspaceExportError>),
    /// The database could not be read or written.
    #[error(transparent)]
    Storage(#[from] StorageError),
}

impl RecordEditError {
    /// Return the CLI exit category for this failure.
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::NotFound { .. } => 5,
            Self::Stale { .. } => 4,
            Self::CommandOwned { .. } => 3,
            Self::Invalid(error) => error.exit_code(),
            Self::Export(_) => 1,
            Self::Storage(error) => error.exit_code(),
        }
    }
}

impl From<WorkspaceImportError> for RecordEditError {
    fn from(error: WorkspaceImportError) -> Self {
        Self::Invalid(Box::new(error))
    }
}

impl From<WorkspaceExportError> for RecordEditError {
    fn from(error: WorkspaceExportError) -> Self {
        Self::Export(Box::new(error))
    }
}

/// One record's workspace file and the revision that names its content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordSource {
    /// The record ID.
    pub id: String,
    /// The record kind, such as `task`.
    pub kind: RecordKind,
    /// The path relative to the workspace root, such as `tasks/<id>.md`.
    pub path: PathBuf,
    /// The canonical file: TOML front matter between `+++` lines, then the
    /// Markdown body.
    pub content: String,
    /// See [`source_revision`].
    pub revision: String,
}

impl RecordSource {
    fn from_file(id: &str, file: &WorkspaceFile) -> Option<Self> {
        let kind = file
            .path
            .parent()
            .and_then(Path::to_str)
            .and_then(RecordKind::from_directory)?;
        Some(Self {
            id: id.to_owned(),
            kind,
            path: file.path.clone(),
            content: String::from_utf8_lossy(&file.content).into_owned(),
            revision: source_revision(&file.content),
        })
    }
}

/// What a saved edit wrote.
#[derive(Debug)]
pub struct RecordEdit {
    /// The record's canonical file after the save.
    pub source: RecordSource,
    /// One message for each blocker rule violation the graph holds that the
    /// edit did not introduce, as [`import_graph`](super::import_graph)
    /// reports them.
    pub warnings: Vec<String>,
}

/// The revision of a record file: the lowercase hex SHA-256 of its bytes.
/// An edit names the revision it started from, and a save refuses it when
/// the file no longer has that revision.
pub fn source_revision(content: &[u8]) -> String {
    Sha256::digest(content)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Return the canonical file of the record `id` in `graph`, or `None` when
/// the graph has no such record.
pub fn record_source(graph: &ConnectedGraph, id: &str) -> Result<Option<RecordSource>, RecordEditError> {
    let files = project_graph(graph)?;
    Ok(find(&files, id).and_then(|file| RecordSource::from_file(id, file)))
}

/// Replace the record `id` with `content`, a complete record file, when the
/// record's file still has `revision`.
///
/// The file is decoded at the record's own path, so its kind and ID cannot
/// change. A change to a field a command owns is refused, naming the command:
///
/// | Kind    | Refused fields                         | Commands                                   |
/// | ------- | -------------------------------------- | ------------------------------------------ |
/// | task    | `status`, `handoff`, `evidence`        | `arcl task start`, `park`, `unpark`, `handoff`, `complete`, `cancel` |
/// | capture | `status`, `promoted-to`                | `arcl capture promote`, `discard`          |
/// | spec    | `status`, `source-capture`             | `arcl spec complete`, `cancel`, `arcl capture promote` |
/// | plan    | `status`                               | `arcl plan complete`, `cancel`             |
/// | phase   | `status`                               | `arcl plan phase complete`, `cancel`       |
/// | release | `status`                               | `arcl release complete`, `cancel`          |
///
/// Those commands check the lifecycle rules, readiness and open subtasks that
/// the import's checks do not. A field left at its value passes, so other
/// fields of a closed record can still be edited. The graph with the edit in place passes the same checks as a
/// workspace import before the database changes. The edit is compared with
/// the database graph whether or not a workspace base exists, so blocker rule
/// violations it introduces are refused as `add_planning_dependency` refuses
/// them; ones already in the database load with a warning. The workspace base
/// is left as it was, so the next sync exports the change to the workspace. A
/// failure changes nothing.
pub fn edit_record(
    database: &mut Database, id: &str, content: &str, revision: &str,
) -> Result<RecordEdit, RecordEditError> {
    // Taken before any read, so a commit from another connection after this
    // point refuses the replacement instead of being overwritten.
    let read = database.data_version()?;
    let project = database.project()?;
    let base = database.workspace_base()?;
    let before = database.connected_graph()?;
    let files = project_graph(&before)?;
    let Some(current) = find(&files, id) else {
        return Err(RecordEditError::NotFound { id: id.to_owned() });
    };
    let path = current.path.clone();
    if source_revision(&current.content) != revision {
        return Err(RecordEditError::Stale { path });
    }

    let mut records = BTreeMap::new();
    for file in files.iter().filter(|file| file.path.parent() != Some(Path::new(""))) {
        let current = String::from_utf8_lossy(&file.content);
        let decode = |input: &str| {
            decode_record(&file.path, input)
                .map_err(|source| WorkspaceImportError::Record { path: file.path.clone(), source })
        };
        let record = if file.path == path {
            let edited = decode(content)?;
            if let Some((field, message)) = command_owned_change(&decode(&current)?, &edited) {
                return Err(RecordEditError::CommandOwned { path, field, message });
            }
            edited
        } else {
            decode(&current)?
        };
        records.insert(file.path.clone(), record);
    }
    let (graph, warnings) = validate_candidate(records, project, Path::new(""), Some(&before))?;
    let source = record_source(&graph, id)?.ok_or_else(|| RecordEditError::NotFound { id: id.to_owned() })?;
    database.replace_graph_and_workspace_base(&graph, &base, read)?;
    Ok(RecordEdit { source, warnings })
}

/// The first field a command owns that differs between `before` and `after`,
/// with the message that names the command. See [`edit_record`].
fn command_owned_change(before: &WorkspaceRecord, after: &WorkspaceRecord) -> Option<(&'static str, &'static str)> {
    use WorkspaceRecord::{Capture, Phase, Plan, Release, Spec, Task};
    let fields = match (before, after) {
        (Task(before), Task(after)) => vec![
            (before.status != after.status, "status", TASK_STATUS),
            (
                before.handoff.as_deref().unwrap_or_default() != after.handoff.as_deref().unwrap_or_default(),
                "handoff",
                "a handoff note is set only by `arcl task handoff` or the web app's Hand off action",
            ),
            (
                before.evidence.as_deref().unwrap_or_default() != after.evidence.as_deref().unwrap_or_default(),
                "evidence",
                "evidence is set only by `arcl task complete --evidence` or the web app's Complete action",
            ),
        ],
        (Capture(before), Capture(after)) => vec![
            (
                before.status != after.status,
                "status",
                "a capture's status changes only through `arcl capture promote` or `arcl capture discard`",
            ),
            (
                before.promoted_to != after.promoted_to,
                "promoted-to",
                "a promotion is set only by `arcl capture promote`",
            ),
        ],
        (Spec(before), Spec(after)) => vec![
            (
                before.status != after.status,
                "status",
                "a spec's status changes only through `arcl spec complete` or `arcl spec cancel`",
            ),
            (
                before.source_capture_id != after.source_capture_id,
                "source-capture",
                "a spec's source capture is set only by `arcl capture promote`",
            ),
        ],
        (Plan(before), Plan(after)) => vec![(
            before.status != after.status,
            "status",
            "a plan's status changes only through `arcl plan complete` or `arcl plan cancel`",
        )],
        (Phase(before), Phase(after)) => vec![(
            before.status != after.status,
            "status",
            "a phase's status changes only through `arcl plan phase complete` or `arcl plan phase cancel`",
        )],
        (Release(before), Release(after)) => vec![(
            before.status != after.status,
            "status",
            "a release's status changes only through `arcl release complete` or `arcl release cancel`",
        )],
        _ => Vec::new(),
    };
    fields
        .into_iter()
        .find(|(changed, ..)| *changed)
        .map(|(_, field, message)| (field, message))
}

/// The record file whose name is `id`, leaving out `manifest.toml`.
fn find<'a>(files: &'a [WorkspaceFile], id: &str) -> Option<&'a WorkspaceFile> {
    files.iter().find(|file| {
        file.path.parent() != Some(Path::new("")) && file.path.file_stem().and_then(|stem| stem.to_str()) == Some(id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcl_core::domain::{ContainerAction, PlanId, TaskAction, TaskId, TaskPriority};
    use arcl_store::{OpenSubtasks, PlanningTaskCreate, WorkspaceBaseFile};

    fn task(database: &mut Database, title: &str) -> String {
        task_in(database, title, None, None)
    }

    /// Create a task under `parent` and in `plan`, when given.
    fn task_in(database: &mut Database, title: &str, parent: Option<&str>, plan: Option<PlanId>) -> String {
        let project = database.project().expect("project exists");
        database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: None,
                plan_id: plan,
                phase_id: None,
                parent_id: parent.map(|id| TaskId::parse(id).expect("ID parses")),
                title: title.to_owned(),
                body: "Body".to_owned(),
                priority: TaskPriority::Normal,
                position: 0,
            })
            .expect("task creates")
            .id
            .to_string()
    }

    fn source(database: &Database, id: &str) -> RecordSource {
        record_source(&database.connected_graph().expect("graph reads"), id)
            .expect("graph renders")
            .expect("record exists")
    }

    #[test]
    fn source_is_the_canonical_file_with_its_revision() {
        let mut database = Database::open_in_memory().expect("database opens");
        let id = task(&mut database, "First");
        let source = source(&database, &id);
        assert_eq!(source.kind, RecordKind::Task);
        assert_eq!(source.path, PathBuf::from(format!("tasks/{id}.md")));
        assert!(source.content.starts_with(&format!("+++\nid = \"{id}\"\n")));
        assert!(source.content.ends_with("+++\n\nBody\n"));
        assert_eq!(source.revision, source_revision(source.content.as_bytes()));
        assert_eq!(source.revision.len(), 64);
    }

    #[test]
    fn an_edit_replaces_the_record_and_keeps_the_base() {
        let mut database = Database::open_in_memory().expect("database opens");
        let id = task(&mut database, "First");
        let before = source(&database, &id);
        let content = before
            .content
            .replace("title = \"First\"", "title = \"Renamed\"")
            .replace("Body", "New body");
        let edit = edit_record(&mut database, &id, &content, &before.revision).expect("edit saves");
        assert_eq!(edit.source.content, content);
        let task = database
            .planning_task(TaskId::parse(&id).expect("ID parses"))
            .expect("task reads")
            .expect("task exists");
        assert_eq!(task.title, "Renamed");
        assert_eq!(task.body, "New body");
        assert!(database.workspace_base().expect("base reads").is_empty());
    }

    #[test]
    fn a_stale_revision_is_refused() {
        let mut database = Database::open_in_memory().expect("database opens");
        let id = task(&mut database, "First");
        let before = source(&database, &id);
        let error = edit_record(&mut database, &id, &before.content, &source_revision(b"other")).unwrap_err();
        assert!(matches!(error, RecordEditError::Stale { .. }));
        assert_eq!(error.exit_code(), 4);
    }

    #[test]
    fn invalid_files_are_refused_with_the_import_error_and_change_nothing() {
        let mut database = Database::open_in_memory().expect("database opens");
        let id = task(&mut database, "First");
        let other = task(&mut database, "Second");
        let before = source(&database, &id);
        let cases = [
            ("+++\ntitle = \n+++\n".to_owned(), "front matter TOML is invalid"),
            (before.content.replace(&id, &other), "does not match filename"),
            (
                format!("+++\nid = \"{id}\"\ntitle = \"Spec\"\nstatus = \"open\"\nacceptance-criteria = \"x\"\n+++\n"),
                "unknown field",
            ),
            (
                before
                    .content
                    .replace("priority", "parent = \"arcl-t-01M5STHSRRT2ADFN3Y9YAC2N7A\"\npriority"),
                "relationship `parent`",
            ),
            (before.content.replace("+++\n\n", ""), "closing `+++`"),
        ];
        for (content, message) in cases {
            let error = edit_record(&mut database, &id, &content, &before.revision).unwrap_err();
            assert_eq!(error.exit_code(), 3, "{content}");
            assert!(error.to_string().contains(message), "{error}");
        }
        assert_eq!(source(&database, &id), before);
    }

    fn base(database: &Database) -> Vec<WorkspaceBaseFile> {
        project_graph(&database.connected_graph().expect("graph reads"))
            .expect("graph renders")
            .into_iter()
            .map(|file| WorkspaceBaseFile { path: file.path.to_string_lossy().into_owned(), content: file.content })
            .collect()
    }

    /// Add `blocked-by = [blocker]` to `id` through a raw edit.
    fn block(database: &mut Database, id: &str, blocker: &str) -> Result<RecordEdit, RecordEditError> {
        let before = source(database, id);
        let content = before
            .content
            .replace("position = 0", &format!("position = 0\nblocked-by = [\"{blocker}\"]"));
        edit_record(database, id, &content, &before.revision)
    }

    #[test]
    fn a_blocker_cycle_the_edit_adds_is_refused_with_or_without_a_base() {
        for synced in [false, true] {
            let mut database = Database::open_in_memory().expect("database opens");
            let first = task(&mut database, "First");
            let second = task(&mut database, "Second");
            database
                .add_planning_dependency(
                    TaskId::parse(&second).expect("ID parses"),
                    TaskId::parse(&first).expect("ID parses"),
                )
                .expect("dependency adds");
            // Without a base, as after `arcl init` without `--workspace`, the
            // edit is still compared with the database graph.
            if synced {
                let base = base(&database);
                database.replace_workspace_base(&base).expect("base saves");
            }
            let before = source(&database, &first);
            let error = block(&mut database, &first, &second).unwrap_err();
            assert_eq!(error.exit_code(), 3, "{error}");
            assert!(error.to_string().contains("blocked-by"), "{error}");
            assert_eq!(source(&database, &first), before);
        }
    }

    #[test]
    fn a_blocker_on_an_ancestor_is_refused_without_a_base() {
        let mut database = Database::open_in_memory().expect("database opens");
        let parent = task(&mut database, "Parent");
        let project = database.project().expect("project exists");
        let child = database
            .create_planning_task(PlanningTaskCreate {
                project_id: project.id,
                spec_id: None,
                plan_id: None,
                phase_id: None,
                parent_id: Some(TaskId::parse(&parent).expect("ID parses")),
                title: "Child".to_owned(),
                body: "Body".to_owned(),
                priority: TaskPriority::Normal,
                position: 0,
            })
            .expect("child creates")
            .id
            .to_string();
        assert!(database.workspace_base().expect("base reads").is_empty());
        let before = source(&database, &child);
        let error = block(&mut database, &child, &parent).unwrap_err();
        assert_eq!(error.exit_code(), 3, "{error}");
        assert!(error.to_string().contains("blocked-by"), "{error}");
        assert_eq!(source(&database, &child), before);
    }

    #[test]
    fn a_blocker_without_a_violation_saves_without_a_base() {
        let mut database = Database::open_in_memory().expect("database opens");
        let first = task(&mut database, "First");
        let second = task(&mut database, "Second");
        let edit = block(&mut database, &first, &second).expect("edit saves");
        assert!(edit.warnings.is_empty(), "{:?}", edit.warnings);
        assert!(edit.source.content.contains(&second));
    }

    /// Replace the first `from` in `id`'s file with `to` through a raw edit.
    fn rewrite(database: &mut Database, id: &str, from: &str, to: &str) -> Result<RecordEdit, RecordEditError> {
        let before = source(database, id);
        assert!(before.content.contains(from), "{}", before.content);
        edit_record(database, id, &before.content.replacen(from, to, 1), &before.revision)
    }

    /// Check that the rewrite is refused for changing `field` and changes nothing.
    fn assert_owned(database: &mut Database, id: &str, from: &str, to: &str, field: &str) {
        let before = source(database, id);
        let error = rewrite(database, id, from, to).unwrap_err();
        assert_eq!(error.exit_code(), 3, "{error}");
        assert!(
            matches!(error, RecordEditError::CommandOwned { field: refused, .. } if refused == field),
            "{error}"
        );
        assert!(error.to_string().contains(&format!("field `{field}`")), "{error}");
        assert!(error.to_string().contains("`arcl "), "{error}");
        assert_eq!(source(database, id), before);
    }

    fn task_id(id: &str) -> TaskId {
        TaskId::parse(id).expect("ID parses")
    }

    #[test]
    fn a_task_status_change_is_refused() {
        let mut database = Database::open_in_memory().expect("database opens");
        let blocked = task(&mut database, "Blocked");
        let blocker = task(&mut database, "Blocker");
        database
            .add_planning_dependency(task_id(&blocked), task_id(&blocker))
            .expect("dependency adds");
        // Starting a task whose blocker is pending.
        assert_owned(
            &mut database,
            &blocked,
            "status = \"pending\"",
            "status = \"in_progress\"",
            "status",
        );
        // Cancelling, which no web action offers.
        assert_owned(
            &mut database,
            &blocker,
            "status = \"pending\"",
            "status = \"cancelled\"",
            "status",
        );
        // Reopening a cancelled task, which no command does.
        database
            .transition_planning_task(task_id(&blocker), TaskAction::Cancel, false)
            .expect("task cancels");
        assert_owned(
            &mut database,
            &blocker,
            "status = \"cancelled\"",
            "status = \"pending\"",
            "status",
        );
        // Completing a parent with an open subtask.
        let parent = task(&mut database, "Parent");
        task_in(&mut database, "Child", Some(&parent), None);
        assert_owned(
            &mut database,
            &parent,
            "status = \"pending\"",
            "status = \"completed\"",
            "status",
        );
    }

    #[test]
    fn a_task_handoff_or_evidence_change_is_refused() {
        let mut database = Database::open_in_memory().expect("database opens");
        let id = task(&mut database, "First");
        assert_owned(
            &mut database,
            &id,
            "+++\n",
            "+++\nhandoff = \"Resume here\"\n",
            "handoff",
        );
        assert_owned(&mut database, &id, "+++\n", "+++\nevidence = \"Done\"\n", "evidence");
    }

    #[test]
    fn other_fields_of_a_completed_task_can_be_edited() {
        let mut database = Database::open_in_memory().expect("database opens");
        let id = task(&mut database, "First");
        database
            .complete_planning_task(task_id(&id), OpenSubtasks::Refuse, Some("Done".to_owned()))
            .expect("task completes");
        let edit = rewrite(&mut database, &id, "title = \"First\"", "title = \"Renamed\"").expect("edit saves");
        assert!(edit.source.content.contains("status = \"completed\""));
        let task = database
            .planning_task(task_id(&id))
            .expect("task reads")
            .expect("task exists");
        assert_eq!(task.title, "Renamed");
        assert_eq!(task.evidence, "Done");
    }

    #[test]
    fn a_status_change_on_any_record_with_a_lifecycle_is_refused() {
        let mut database = Database::open_in_memory().expect("database opens");
        let capture = database
            .create_capture("Capture".to_owned(), String::new())
            .expect("capture creates");
        let spec = database
            .create_spec("Spec".to_owned(), String::new(), String::new())
            .expect("spec creates");
        let plan = database
            .create_plan(spec.id, "Plan".to_owned(), String::new())
            .expect("plan creates");
        let phase = database
            .create_phase(plan.id, "Phase".to_owned(), String::new(), 0)
            .expect("phase creates");
        let release = database
            .create_release("Release".to_owned(), String::new())
            .expect("release creates");
        let open = ("status = \"open\"", "status = \"completed\"");
        let cases = [
            (
                capture.id.to_string(),
                ("status = \"captured\"", "status = \"discarded\""),
            ),
            (spec.id.to_string(), open),
            (plan.id.to_string(), open),
            (phase.id.to_string(), open),
            (release.id.to_string(), open),
        ];
        for (id, (from, to)) in cases {
            assert_owned(&mut database, &id, from, to, "status");
        }
        database
            .transition_release(release.id, ContainerAction::Complete, false)
            .expect("release completes");
        assert_owned(
            &mut database,
            &release.id.to_string(),
            "status = \"completed\"",
            "status = \"open\"",
            "status",
        );
    }

    #[test]
    fn a_plan_key_used_twice_in_one_plan_is_refused() {
        let mut database = Database::open_in_memory().expect("database opens");
        let spec = database
            .create_spec("Spec".to_owned(), String::new(), String::new())
            .expect("spec creates");
        let plan = database
            .create_plan(spec.id, "Plan".to_owned(), String::new())
            .expect("plan creates");
        let first = task_in(&mut database, "First", None, Some(plan.id));
        let second = task_in(&mut database, "Second", None, Some(plan.id));
        let key = "+++\nplan-key = \"build\"\n";
        rewrite(&mut database, &first, "+++\n", key).expect("first key saves");
        let before = source(&database, &second);
        let error = rewrite(&mut database, &second, "+++\n", key).unwrap_err();
        assert_eq!(error.exit_code(), 3, "{error}");
        assert!(error.to_string().contains("field `plan-key`"), "{error}");
        assert_eq!(source(&database, &second), before);
    }

    #[test]
    fn an_unknown_record_is_not_found() {
        let mut database = Database::open_in_memory().expect("database opens");
        let error = edit_record(&mut database, "arcl-t-01M5STHSRRT2ADFN3Y9YAC2N7A", "", "").unwrap_err();
        assert_eq!(error.exit_code(), 5);
    }
}
