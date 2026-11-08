//! `arcl compact`: archive old completed tasks and keep their evidence in one
//! summary note per phase.

use std::collections::{BTreeMap, HashMap, HashSet};

use rusqlite::{Connection, TransactionBehavior, params};
use serde::Serialize;

use super::{ConnectedGraph, Result, StorageError, graph};
use crate::storage::task_ancestry;
use arcl_core::domain::*;

/// Every compaction note's title starts with this. A note linked to a phase
/// whose title starts with it is that phase's compaction note.
pub const COMPACTION_TITLE_PREFIX: &str = "Compacted work";

/// The line a new compaction note's body starts with.
const COMPACTION_INTRO: &str = "Evidence from the completed tasks `arcl compact` archived in this phase.";

/// What [`compact`] archives.
#[derive(Clone, Debug)]
pub struct CompactionRequest {
    /// Archive tasks completed before the start of this UTC date, written
    /// `YYYY-MM-DD`.
    pub before: String,
    /// Archive only tasks whose effective phase is this one.
    pub phase_id: Option<PhaseId>,
    /// Also archive completed tasks with no completion time, as if they were
    /// completed before any date.
    pub include_undated: bool,
    /// Report what would change and write nothing.
    pub dry_run: bool,
}

/// What [`compact`] archived, or would archive on a dry run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Compaction {
    /// Whether this was a dry run that wrote nothing.
    pub dry_run: bool,
    /// The archive time set on every task, or `None` on a dry run or when
    /// nothing was archived.
    pub archived_at: Option<String>,
    /// One entry per phase with archived tasks, in phase order.
    pub phases: Vec<PhaseCompaction>,
}

/// The tasks archived in one phase and the note that holds their evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PhaseCompaction {
    /// The phase.
    pub phase_id: PhaseId,
    /// The phase title.
    pub phase_title: String,
    /// The compaction note, or `None` for a note a dry run would create.
    pub note_id: Option<NoteId>,
    /// The compaction note's title.
    pub note_title: String,
    /// Whether the note is new. Otherwise the tasks were appended to it.
    pub note_created: bool,
    /// The archived tasks, oldest completion first, undated tasks first.
    pub tasks: Vec<PlanningTask>,
}

/// Archive the completed tasks `request` selects and write their evidence
/// into each phase's compaction note, in one transaction.
///
/// A task is archived when it is completed and not yet archived, its
/// completion time is before `request.before` (or it has none and
/// `include_undated` is set), it sits under a phase, it does not block a
/// task that is pending, in progress or parked or has such a descendant, and
/// every subtask it has is archived too. Cancelled and open tasks are never
/// archived, so a parent with one stays.
///
/// Each phase's tasks go into the note linked to the phase whose title
/// starts with [`COMPACTION_TITLE_PREFIX`], appended in a section per task.
/// A phase with no such note gets a new one, titled `Compacted work: <phase
/// title>` and linked to the phase. Archived tasks are never selected again,
/// so running the same request twice changes nothing the second time.
pub fn compact(connection: &mut Connection, project_id: ProjectId, request: &CompactionRequest) -> Result<Compaction> {
    if !is_calendar_date(&request.before) {
        return Err(StorageError::InvalidCompactDate { value: request.before.clone() });
    }
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let graph = graph(&tx)?;
    if graph.project.id != project_id {
        return Err(StorageError::ProjectNotFound);
    }
    if let Some(phase_id) = request.phase_id
        && !graph.phases.iter().any(|phase| phase.id == phase_id)
    {
        return Err(StorageError::PhaseNotFound { id: phase_id.to_string() });
    }
    let selected = select(&graph, request)?;
    if selected.is_empty() {
        return Ok(Compaction { dry_run: request.dry_run, archived_at: None, phases: Vec::new() });
    }
    let archived_at = if request.dry_run {
        None
    } else {
        Some(tx.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |row| {
            row.get::<_, String>(0)
        })?)
    };

    let mut phases = Vec::new();
    for phase in &graph.phases {
        let Some(tasks) = selected.get(&phase.id) else { continue };
        let mut tasks = tasks.clone();
        tasks.sort_by(|left, right| (&left.completed_at, left.id).cmp(&(&right.completed_at, right.id)));
        let sections = tasks.iter().map(section).collect::<Vec<_>>().join("\n\n");
        let existing = compaction_note(&graph, phase.id);
        let (note_id, note_title, note_created) = match existing {
            Some(note) => {
                if !request.dry_run {
                    let body = format!("{}\n\n{sections}", note.body.trim_end());
                    tx.execute(
                        "UPDATE notes SET body = ?1 WHERE id = ?2",
                        params![body, note.id.to_string()],
                    )?;
                }
                (Some(note.id), note.title.clone(), false)
            }
            None => {
                let title = format!("{COMPACTION_TITLE_PREFIX}: {}", phase.title);
                let note = Note::new(project_id, title, format!("{COMPACTION_INTRO}\n\n{sections}"))
                    .map_err(StorageError::InvalidNote)?;
                if request.dry_run {
                    (None, note.title, true)
                } else {
                    tx.execute(
                        "INSERT INTO notes (id, project_id, title, body) VALUES (?1, ?2, ?3, ?4)",
                        params![note.id.to_string(), project_id.to_string(), note.title, note.body],
                    )?;
                    tx.execute(
                        "INSERT INTO record_links (project_id, source_kind, source_id, target_kind, target_id)
                         VALUES (?1, 'note', ?2, 'phase', ?3)",
                        params![project_id.to_string(), note.id.to_string(), phase.id.to_string()],
                    )?;
                    (Some(note.id), note.title, true)
                }
            }
        };
        if let Some(archived_at) = &archived_at {
            for task in &mut tasks {
                tx.execute(
                    "UPDATE planning_tasks SET archived_at = ?1 WHERE id = ?2",
                    params![archived_at, task.id.to_string()],
                )?;
                task.archived_at = Some(archived_at.clone());
            }
        }
        phases.push(PhaseCompaction {
            phase_id: phase.id,
            phase_title: phase.title.clone(),
            note_id,
            note_title,
            note_created,
            tasks,
        });
    }
    if !request.dry_run {
        tx.commit()?;
    }
    Ok(Compaction { dry_run: request.dry_run, archived_at, phases })
}

/// The tasks to archive, by effective phase, in graph order.
fn select(graph: &ConnectedGraph, request: &CompactionRequest) -> Result<BTreeMap<PhaseId, Vec<PlanningTask>>> {
    let before = format!("{}T00:00:00.000Z", request.before);
    let mut children = HashMap::<TaskId, Vec<&PlanningTask>>::new();
    for task in &graph.tasks {
        if let Some(parent) = task.parent_id {
            children.entry(parent).or_default().push(task);
        }
    }
    let open = graph
        .tasks
        .iter()
        .filter(|task| has_open_work(task, &children))
        .map(|task| task.id)
        .collect::<HashSet<_>>();
    let protected = graph
        .dependencies
        .iter()
        .filter(|dependency| open.contains(&dependency.task_id))
        .map(|dependency| dependency.blocker_id)
        .collect::<HashSet<_>>();

    let mut phases = HashMap::new();
    for task in &graph.tasks {
        phases.insert(task.id, task_ancestry(graph, task.id)?.2);
    }
    let candidate = |task: &PlanningTask| {
        let phase = phases.get(&task.id).copied().flatten();
        task.status == TaskStatus::Completed
            && task.archived_at.is_none()
            && task
                .completed_at
                .as_ref()
                .map_or(request.include_undated, |completed_at| *completed_at < before)
            && phase.is_some()
            && request.phase_id.is_none_or(|id| phase == Some(id))
            && !protected.contains(&task.id)
    };
    let mut memo = HashMap::new();
    let mut selected = BTreeMap::<PhaseId, Vec<PlanningTask>>::new();
    for task in &graph.tasks {
        if eligible(task, &children, &candidate, &mut memo)
            && let Some(phase) = phases.get(&task.id).copied().flatten()
        {
            selected.entry(phase).or_default().push(task.clone());
        }
    }
    Ok(selected)
}

/// Whether `task` or one of its descendants is pending, in progress or
/// parked.
fn has_open_work(task: &PlanningTask, children: &HashMap<TaskId, Vec<&PlanningTask>>) -> bool {
    matches!(
        task.status,
        TaskStatus::Pending | TaskStatus::InProgress | TaskStatus::Parked
    ) || children
        .get(&task.id)
        .is_some_and(|tasks| tasks.iter().any(|child| has_open_work(child, children)))
}

/// Whether `task` is a candidate and every subtask is archived already or
/// eligible itself.
fn eligible(
    task: &PlanningTask, children: &HashMap<TaskId, Vec<&PlanningTask>>, candidate: &impl Fn(&PlanningTask) -> bool,
    memo: &mut HashMap<TaskId, bool>,
) -> bool {
    if let Some(known) = memo.get(&task.id) {
        return *known;
    }
    let result = candidate(task)
        && children.get(&task.id).is_none_or(|tasks| {
            tasks
                .iter()
                .all(|child| child.archived_at.is_some() || eligible(child, children, candidate, memo))
        });
    memo.insert(task.id, result);
    result
}

/// The note linked to `phase` whose title starts with
/// [`COMPACTION_TITLE_PREFIX`], the lowest ID first.
fn compaction_note(graph: &ConnectedGraph, phase: PhaseId) -> Option<&Note> {
    let phase = phase.to_string();
    let linked = graph
        .links
        .iter()
        .filter(|link| {
            link.source_kind == LinkedRecordKind::Note
                && link.target_kind == LinkedRecordKind::Phase
                && link.target_id == phase
        })
        .map(|link| link.source_id.as_str())
        .collect::<HashSet<_>>();
    graph
        .notes
        .iter()
        .filter(|note| linked.contains(note.id.to_string().as_str()) && note.title.starts_with(COMPACTION_TITLE_PREFIX))
        .min_by_key(|note| note.id)
}

/// The note section for one archived task: its title, ID, completion time
/// and evidence.
fn section(task: &PlanningTask) -> String {
    let completed = task.completed_at.as_deref().map_or_else(
        || "completed at an unknown time".to_owned(),
        |at| format!("completed {at}"),
    );
    let evidence = task.evidence.trim();
    let evidence = if evidence.is_empty() { "No evidence recorded." } else { evidence };
    format!("## {}\n\n`{}` · {completed}\n\n{evidence}", task.title.trim(), task.id)
}
