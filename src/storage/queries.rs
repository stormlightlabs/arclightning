use std::collections::{HashMap, HashSet};

use rusqlite::Connection;
use serde::Serialize;

use crate::domain::{
    ContainerStatus, Epic, EpicId, Idea, IdeaId, Milestone, MilestoneId, Release, ReleaseId, Task, TaskDependency,
    TaskId, TaskPriority, TaskStatus,
};

use super::{Result, StorageError, dependencies, epics, ideas, milestones, releases, tasks};

/// All v1 records loaded for one read operation.
#[derive(Clone, Debug)]
pub struct Graph {
    pub ideas: Vec<Idea>,
    pub releases: Vec<Release>,
    pub epics: Vec<Epic>,
    pub milestones: Vec<Milestone>,
    pub tasks: Vec<Task>,
    pub dependencies: Vec<TaskDependency>,
}

/// Broad record filters after CLI values have been validated.
#[derive(Clone, Debug, Default)]
pub struct ListFilter {
    pub kind: Option<String>,
    pub statuses: Vec<String>,
    pub priorities: Vec<TaskPriority>,
    pub release_id: Option<ReleaseId>,
    pub epic_id: Option<EpicId>,
    pub milestone_id: Option<MilestoneId>,
    pub parent_id: Option<TaskId>,
}

/// A stable summary record used by list output.
#[derive(Clone, Debug, Serialize)]
pub struct ListItem {
    pub kind: &'static str,
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: Option<String>,
    pub release_id: Option<String>,
    pub epic_id: Option<String>,
    pub milestone_id: Option<String>,
    pub parent_id: Option<String>,
    pub blocked: Option<bool>,
    pub blockers: Vec<String>,
    pub ready: Option<bool>,
    pub handoff: Option<String>,
    pub evidence: Option<String>,
    pub promoted_to: Option<String>,
    pub source_idea: Option<String>,
    pub progress: Option<Progress>,
}

/// Counts for a record's descendants.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Progress {
    pub total: usize,
    pub completed: usize,
    pub cancelled: usize,
    pub open: usize,
}

/// A direct blocker and its completion evidence.
#[derive(Clone, Debug, Serialize)]
pub struct BlockerView {
    pub task: Task,
    pub completed: bool,
    pub evidence: String,
}

/// Every readiness condition that applies to one task.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Readiness {
    pub ready: bool,
    pub blocked: bool,
    pub reasons: Vec<String>,
}

/// A record returned by prefix-routed show.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind")]
pub enum ShowView {
    #[serde(rename = "idea")]
    Idea { record: Idea },
    #[serde(rename = "release")]
    Release {
        record: Release,
        epics: Vec<Epic>,
        progress: Progress,
    },
    #[serde(rename = "epic")]
    Epic {
        record: Epic,
        milestones: Vec<Milestone>,
        progress: Progress,
    },
    #[serde(rename = "milestone")]
    Milestone {
        record: Milestone,
        tasks: Vec<Task>,
        progress: Progress,
    },
    #[serde(rename = "task")]
    Task { record: Box<TaskView> },
}

/// An enriched task record for show output.
#[derive(Clone, Debug, Serialize)]
pub struct TaskView {
    pub task: Task,
    pub readiness: Readiness,
    pub blockers: Vec<BlockerView>,
    pub dependents: Vec<Task>,
    pub children: Vec<Task>,
    pub ancestors: Vec<Task>,
    pub progress: Progress,
    pub milestone: Milestone,
    pub epic: Epic,
    pub release: Option<Release>,
}

/// The bounded context packet for one task.
#[derive(Clone, Debug, Serialize)]
pub struct ContextView {
    pub task: Task,
    pub ancestors: Vec<Task>,
    pub milestone: Milestone,
    pub epic: Epic,
    pub release: Option<Release>,
    pub spec_path: String,
    pub blockers: Vec<BlockerView>,
    pub dependents: Vec<Task>,
    pub readiness: Readiness,
    pub completion_evidence: Vec<BlockerView>,
}

/// A deterministic hierarchy node.
#[derive(Clone, Debug, Serialize)]
pub struct TreeNode {
    pub kind: &'static str,
    pub id: String,
    pub title: String,
    pub status: String,
    pub position: Option<i64>,
    pub children: Vec<TreeNode>,
}

/// Database and graph validation results.
#[derive(Clone, Debug, Default, Serialize)]
pub struct CheckReport {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn load(connection: &Connection) -> Result<Graph> {
    Ok(Graph {
        ideas: ideas::list(connection)?,
        releases: releases::list(connection)?,
        epics: epics::list(connection)?,
        milestones: milestones::list(connection)?,
        tasks: tasks::list(connection)?,
        dependencies: dependencies::list(connection)?,
    })
}

pub fn task_view(connection: &Connection, id: TaskId) -> Result<TaskView> {
    let graph = load(connection)?;
    task_view_from_graph(&graph, id)
}

pub fn show(connection: &Connection, id: &str) -> Result<ShowView> {
    let graph = load(connection)?;
    graph.show(id)
}

pub fn context(connection: &Connection, id: TaskId) -> Result<ContextView> {
    let graph = load(connection)?;
    let view = task_view_from_graph(&graph, id)?;
    let completion_evidence = view
        .blockers
        .iter()
        .filter(|blocker| blocker.completed && !blocker.evidence.is_empty())
        .cloned()
        .collect();
    Ok(ContextView {
        task: view.task,
        ancestors: view.ancestors,
        milestone: view.milestone,
        epic: view.epic.clone(),
        release: view.release,
        spec_path: view.epic.spec_path,
        blockers: view.blockers,
        dependents: view.dependents,
        readiness: view.readiness,
        completion_evidence,
    })
}

impl Graph {
    fn show(&self, id: &str) -> Result<ShowView> {
        if id.starts_with(IdeaId::PREFIX) {
            let id = IdeaId::parse(id)
                .map_err(|error| StorageError::InvalidTask(crate::domain::DomainError::InvalidId(error)))?;
            let record = self
                .ideas
                .iter()
                .find(|idea| idea.id == id)
                .cloned()
                .ok_or_else(|| StorageError::IdeaNotFound { id: id.to_string() })?;
            return Ok(ShowView::Idea { record });
        }
        if id.starts_with(ReleaseId::PREFIX) {
            let id = ReleaseId::parse(id)
                .map_err(|error| StorageError::InvalidTask(crate::domain::DomainError::InvalidId(error)))?;
            let record = self
                .releases
                .iter()
                .find(|release| release.id == id)
                .cloned()
                .ok_or_else(|| StorageError::ReleaseNotFound { id: id.to_string() })?;
            let epics = self
                .epics
                .iter()
                .filter(|epic| epic.release_id == Some(id))
                .cloned()
                .collect();
            return Ok(ShowView::Release { record, epics, progress: self.release_progress(id) });
        }
        if id.starts_with(EpicId::PREFIX) {
            let id = EpicId::parse(id)
                .map_err(|error| StorageError::InvalidTask(crate::domain::DomainError::InvalidId(error)))?;
            let record = self
                .epics
                .iter()
                .find(|epic| epic.id == id)
                .cloned()
                .ok_or_else(|| StorageError::EpicNotFound { id: id.to_string() })?;
            let milestones = self
                .milestones
                .iter()
                .filter(|milestone| milestone.epic_id == id)
                .cloned()
                .collect();
            return Ok(ShowView::Epic { record, milestones, progress: self.epic_progress(id) });
        }
        if id.starts_with(MilestoneId::PREFIX) {
            let id = MilestoneId::parse(id)
                .map_err(|error| StorageError::InvalidTask(crate::domain::DomainError::InvalidId(error)))?;
            let record = self
                .milestones
                .iter()
                .find(|milestone| milestone.id == id)
                .cloned()
                .ok_or_else(|| StorageError::MilestoneNotFound { id: id.to_string() })?;
            let tasks = self
                .tasks
                .iter()
                .filter(|task| task.milestone_id == id)
                .cloned()
                .collect();
            return Ok(ShowView::Milestone { record, tasks, progress: self.milestone_progress(id) });
        }
        if id.starts_with(TaskId::PREFIX) {
            let id = TaskId::parse(id)
                .map_err(|error| StorageError::InvalidTask(crate::domain::DomainError::InvalidId(error)))?;
            return Ok(ShowView::Task { record: Box::new(task_view_from_graph(self, id)?) });
        }
        Err(StorageError::InvalidTask(crate::domain::DomainError::InvalidId(
            crate::domain::IdError::InvalidPrefix { entity: "record", expected: "arcl-<kind>-", value: id.to_owned() },
        )))
    }

    /// Produce filtered, deterministic record summaries.
    pub fn list_items(&self, filter: &ListFilter) -> Vec<ListItem> {
        let mut items = Vec::new();
        for idea in &self.ideas {
            if matches_kind(filter, "idea") && filter.applies_to_ideas() && filter.statuses_match(idea.status.as_str())
            {
                items.push(ListItem {
                    kind: "idea",
                    id: idea.id.to_string(),
                    title: idea.title.clone(),
                    description: idea.description.clone(),
                    status: idea.status.as_str().to_owned(),
                    priority: None,
                    release_id: None,
                    epic_id: None,
                    milestone_id: None,
                    parent_id: None,
                    blocked: None,
                    blockers: Vec::new(),
                    ready: None,
                    handoff: None,
                    evidence: None,
                    promoted_to: idea.promoted_to.map(|id| id.to_string()),
                    source_idea: None,
                    progress: None,
                });
            }
        }
        for release in &self.releases {
            if matches_kind(filter, "release")
                && filter.applies_to_releases()
                && filter.statuses_match(release.status.as_str())
                && filter.release_id.is_none_or(|id| id == release.id)
            {
                items.push(ListItem {
                    kind: "release",
                    id: release.id.to_string(),
                    title: release.title.clone(),
                    description: release.description.clone(),
                    status: release.status.as_str().to_owned(),
                    priority: None,
                    release_id: None,
                    epic_id: None,
                    milestone_id: None,
                    parent_id: None,
                    blocked: None,
                    blockers: Vec::new(),
                    ready: None,
                    handoff: None,
                    evidence: None,
                    promoted_to: None,
                    source_idea: None,
                    progress: Some(self.release_progress(release.id)),
                });
            }
        }
        for epic in &self.epics {
            if !matches_kind(filter, "epic")
                || !filter.applies_to_epics()
                || !filter.statuses_match(epic.status.as_str())
                || !filter.epic_matches(epic.id)
                || !filter.release_matches(epic.release_id)
            {
                continue;
            }
            items.push(ListItem {
                kind: "epic",
                id: epic.id.to_string(),
                title: epic.title.clone(),
                description: epic.description.clone(),
                status: epic.status.as_str().to_owned(),
                priority: None,
                release_id: epic.release_id.map(|id| id.to_string()),
                epic_id: None,
                milestone_id: None,
                parent_id: None,
                blocked: None,
                blockers: Vec::new(),
                ready: None,
                handoff: None,
                evidence: None,
                promoted_to: None,
                source_idea: epic.source_idea.map(|id| id.to_string()),
                progress: Some(self.epic_progress(epic.id)),
            });
        }
        for milestone in &self.milestones {
            let epic = self.epics.iter().find(|epic| epic.id == milestone.epic_id);
            let Some(epic) = epic else { continue };
            if !matches_kind(filter, "milestone")
                || !filter.applies_to_milestones()
                || !filter.statuses_match(milestone.status.as_str())
                || !filter.epic_matches(epic.id)
                || !filter.release_matches(epic.release_id)
                || !filter.milestone_matches(milestone.id)
            {
                continue;
            }
            items.push(ListItem {
                kind: "milestone",
                id: milestone.id.to_string(),
                title: milestone.title.clone(),
                description: milestone.description.clone(),
                status: milestone.status.as_str().to_owned(),
                priority: None,
                release_id: epic.release_id.map(|id| id.to_string()),
                epic_id: Some(epic.id.to_string()),
                milestone_id: None,
                parent_id: None,
                blocked: None,
                blockers: Vec::new(),
                ready: None,
                handoff: None,
                evidence: None,
                promoted_to: None,
                source_idea: None,
                progress: Some(self.milestone_progress(milestone.id)),
            });
        }
        for task in &self.tasks {
            let Some(milestone) = self
                .milestones
                .iter()
                .find(|milestone| milestone.id == task.milestone_id)
            else {
                continue;
            };
            let Some(epic) = self.epics.iter().find(|epic| epic.id == milestone.epic_id) else { continue };
            if !matches_kind(filter, "task")
                || !filter.status_match(task.status.as_str())
                || !filter.priority_matches(task.priority)
                || !filter.release_matches(epic.release_id)
                || !filter.epic_matches(epic.id)
                || !filter.milestone_matches(milestone.id)
                || !filter.parent_matches(task.parent_id)
            {
                continue;
            }
            let readiness = readiness(self, task);
            items.push(ListItem {
                kind: "task",
                id: task.id.to_string(),
                title: task.title.clone(),
                description: task.description.clone(),
                status: task.status.as_str().to_owned(),
                priority: Some(task.priority.as_str().to_owned()),
                release_id: epic.release_id.map(|id| id.to_string()),
                epic_id: Some(epic.id.to_string()),
                milestone_id: Some(milestone.id.to_string()),
                parent_id: task.parent_id.map(|id| id.to_string()),
                blocked: Some(readiness.blocked),
                blockers: self
                    .dependencies
                    .iter()
                    .filter(|dependency| dependency.task_id == task.id)
                    .map(|dependency| dependency.blocker_id.to_string())
                    .collect(),
                ready: Some(readiness.ready),
                handoff: Some(task.handoff.clone()),
                evidence: Some(task.evidence.clone()),
                promoted_to: None,
                source_idea: None,
                progress: Some(self.task_progress(task.id)),
            });
        }
        items.sort_by(|left, right| left.kind.cmp(right.kind).then_with(|| left.id.cmp(&right.id)));
        items
    }

    /// Return the hierarchy rooted at an optional record.
    pub fn tree(&self, root: Option<&str>) -> Result<Vec<TreeNode>> {
        match root {
            None => {
                let mut nodes = Vec::new();
                for release in &self.releases {
                    nodes.push(self.release_node(release)?);
                }
                for epic in self.epics.iter().filter(|epic| epic.release_id.is_none()) {
                    nodes.push(self.epic_node(epic)?);
                }
                Ok(nodes)
            }
            Some(id) if id.starts_with(IdeaId::PREFIX) => {
                let id = IdeaId::parse(id).map_err(DomainErrorToStorage::into_storage)?;
                let idea = self
                    .ideas
                    .iter()
                    .find(|idea| idea.id == id)
                    .ok_or_else(|| StorageError::IdeaNotFound { id: id.to_string() })?;
                Ok(vec![TreeNode {
                    kind: "idea",
                    id: idea.id.to_string(),
                    title: idea.title.clone(),
                    status: idea.status.as_str().to_owned(),
                    position: None,
                    children: Vec::new(),
                }])
            }
            Some(id) if id.starts_with(ReleaseId::PREFIX) => {
                let id = ReleaseId::parse(id).map_err(DomainErrorToStorage::into_storage)?;
                let release = self
                    .releases
                    .iter()
                    .find(|release| release.id == id)
                    .ok_or_else(|| StorageError::ReleaseNotFound { id: id.to_string() })?;
                Ok(vec![self.release_node(release)?])
            }
            Some(id) if id.starts_with(EpicId::PREFIX) => {
                let id = EpicId::parse(id).map_err(DomainErrorToStorage::into_storage)?;
                let epic = self
                    .epics
                    .iter()
                    .find(|epic| epic.id == id)
                    .ok_or_else(|| StorageError::EpicNotFound { id: id.to_string() })?;
                Ok(vec![self.epic_node(epic)?])
            }
            Some(id) if id.starts_with(MilestoneId::PREFIX) => {
                let id = MilestoneId::parse(id).map_err(DomainErrorToStorage::into_storage)?;
                let milestone = self
                    .milestones
                    .iter()
                    .find(|milestone| milestone.id == id)
                    .ok_or_else(|| StorageError::MilestoneNotFound { id: id.to_string() })?;
                Ok(vec![self.milestone_node(milestone)?])
            }
            Some(id) if id.starts_with(TaskId::PREFIX) => {
                let id = TaskId::parse(id).map_err(DomainErrorToStorage::into_storage)?;
                let task = self
                    .tasks
                    .iter()
                    .find(|task| task.id == id)
                    .ok_or_else(|| StorageError::TaskNotFound { id: id.to_string() })?;
                Ok(vec![self.task_node(task, &mut HashSet::new())?])
            }
            Some(id) => Err(StorageError::InvalidTask(crate::domain::DomainError::InvalidId(
                crate::domain::IdError::InvalidPrefix {
                    entity: "record",
                    expected: "arcl-<kind>-",
                    value: id.to_owned(),
                },
            ))),
        }
    }

    fn release_node(&self, release: &Release) -> Result<TreeNode> {
        let mut children = self
            .epics
            .iter()
            .filter(|epic| epic.release_id == Some(release.id))
            .map(|epic| self.epic_node(epic))
            .collect::<Result<Vec<_>>>()?;
        children.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(node(
            "release",
            release.id.to_string(),
            release.title.clone(),
            release.status.as_str(),
            None,
            children,
        ))
    }

    fn epic_node(&self, epic: &Epic) -> Result<TreeNode> {
        let mut children = self
            .milestones
            .iter()
            .filter(|milestone| milestone.epic_id == epic.id)
            .map(|milestone| self.milestone_node(milestone))
            .collect::<Result<Vec<_>>>()?;
        children.sort_by(|a, b| a.position.cmp(&b.position).then_with(|| a.id.cmp(&b.id)));
        Ok(node(
            "epic",
            epic.id.to_string(),
            epic.title.clone(),
            epic.status.as_str(),
            None,
            children,
        ))
    }

    fn milestone_node(&self, milestone: &Milestone) -> Result<TreeNode> {
        let mut children = self
            .tasks
            .iter()
            .filter(|task| task.milestone_id == milestone.id && task.parent_id.is_none())
            .map(|task| self.task_node(task, &mut HashSet::new()))
            .collect::<Result<Vec<_>>>()?;
        children.sort_by(|a, b| a.position.cmp(&b.position).then_with(|| a.id.cmp(&b.id)));
        Ok(node(
            "milestone",
            milestone.id.to_string(),
            milestone.title.clone(),
            milestone.status.as_str(),
            Some(milestone.position),
            children,
        ))
    }

    fn task_node(&self, task: &Task, path: &mut HashSet<TaskId>) -> Result<TreeNode> {
        if !path.insert(task.id) {
            return Err(StorageError::InvalidTask(crate::domain::DomainError::ParentCycle {
                task: task.id.to_string(),
                parent: task.id.to_string(),
            }));
        }
        let mut children = self
            .tasks
            .iter()
            .filter(|child| child.parent_id == Some(task.id))
            .map(|child| self.task_node(child, path))
            .collect::<Result<Vec<_>>>()?;
        path.remove(&task.id);
        children.sort_by(|a, b| a.position.cmp(&b.position).then_with(|| a.id.cmp(&b.id)));
        Ok(node(
            "task",
            task.id.to_string(),
            task.title.clone(),
            task.status.as_str(),
            Some(task.position),
            children,
        ))
    }

    fn release_progress(&self, id: ReleaseId) -> Progress {
        self.epics
            .iter()
            .filter(|epic| epic.release_id == Some(id))
            .map(|epic| self.epic_progress(epic.id))
            .fold(Progress::default(), Progress::combine)
    }

    fn epic_progress(&self, id: EpicId) -> Progress {
        self.milestones
            .iter()
            .filter(|milestone| milestone.epic_id == id)
            .map(|milestone| self.milestone_progress(milestone.id))
            .fold(Progress::default(), Progress::combine)
    }

    fn milestone_progress(&self, id: MilestoneId) -> Progress {
        self.tasks
            .iter()
            .filter(|task| task.milestone_id == id)
            .fold(Progress::default(), |mut progress, task| {
                progress.add(task.status);
                progress
            })
    }

    fn task_progress(&self, id: TaskId) -> Progress {
        self.tasks
            .iter()
            .filter(|task| task.id != id && self.is_descendant(task.id, id))
            .fold(Progress::default(), |mut progress, task| {
                progress.add(task.status);
                progress
            })
    }

    fn is_descendant(&self, mut child: TaskId, ancestor: TaskId) -> bool {
        let by_id = self
            .tasks
            .iter()
            .map(|task| (task.id, task.parent_id))
            .collect::<HashMap<_, _>>();
        let mut seen = HashSet::new();
        while let Some(parent) = by_id.get(&child).copied().flatten() {
            if parent == ancestor {
                return true;
            }
            if !seen.insert(parent) {
                return false;
            }
            child = parent;
        }
        false
    }
}

impl Progress {
    fn add(&mut self, status: TaskStatus) {
        self.total += 1;
        match status {
            TaskStatus::Completed => self.completed += 1,
            TaskStatus::Cancelled => self.cancelled += 1,
            _ => self.open += 1,
        }
    }

    fn combine(mut self, other: Progress) -> Progress {
        self.total += other.total;
        self.completed += other.completed;
        self.cancelled += other.cancelled;
        self.open += other.open;
        self
    }
}

impl ListFilter {
    fn applies_to_ideas(&self) -> bool {
        self.release_id.is_none()
            && self.epic_id.is_none()
            && self.milestone_id.is_none()
            && self.parent_id.is_none()
            && self.priorities.is_empty()
    }
    fn applies_to_releases(&self) -> bool {
        self.epic_id.is_none() && self.milestone_id.is_none() && self.parent_id.is_none() && self.priorities.is_empty()
    }
    fn applies_to_epics(&self) -> bool {
        self.milestone_id.is_none() && self.parent_id.is_none() && self.priorities.is_empty()
    }
    fn applies_to_milestones(&self) -> bool {
        self.parent_id.is_none() && self.priorities.is_empty()
    }
    fn statuses_match(&self, status: &str) -> bool {
        self.statuses.is_empty() || self.statuses.iter().any(|value| value == status)
    }
    fn status_match(&self, status: &str) -> bool {
        self.statuses_match(status)
    }
    fn priority_matches(&self, priority: TaskPriority) -> bool {
        self.priorities.is_empty() || self.priorities.contains(&priority)
    }
    fn release_matches(&self, release: Option<ReleaseId>) -> bool {
        self.release_id.is_none_or(|id| release == Some(id))
    }
    fn epic_matches(&self, epic: EpicId) -> bool {
        self.epic_id.is_none_or(|id| id == epic)
    }
    fn milestone_matches(&self, milestone: MilestoneId) -> bool {
        self.milestone_id.is_none_or(|id| id == milestone)
    }
    fn parent_matches(&self, parent: Option<TaskId>) -> bool {
        self.parent_id.is_none_or(|id| parent == Some(id))
    }
}

fn matches_kind(filter: &ListFilter, kind: &str) -> bool {
    filter.kind.as_deref().is_none_or(|value| value == kind)
}

fn node(
    kind: &'static str, id: String, title: String, status: &str, position: Option<i64>, children: Vec<TreeNode>,
) -> TreeNode {
    TreeNode { kind, id, title, status: status.to_owned(), position, children }
}

fn task_view_from_graph(graph: &Graph, id: TaskId) -> Result<TaskView> {
    let task = graph
        .tasks
        .iter()
        .find(|task| task.id == id)
        .cloned()
        .ok_or_else(|| StorageError::TaskNotFound { id: id.to_string() })?;
    let milestone = graph
        .milestones
        .iter()
        .find(|milestone| milestone.id == task.milestone_id)
        .cloned()
        .ok_or_else(|| StorageError::MilestoneNotFound { id: task.milestone_id.to_string() })?;
    let epic = graph
        .epics
        .iter()
        .find(|epic| epic.id == milestone.epic_id)
        .cloned()
        .ok_or_else(|| StorageError::EpicNotFound { id: milestone.epic_id.to_string() })?;
    let release = epic
        .release_id
        .and_then(|id| graph.releases.iter().find(|release| release.id == id).cloned());
    let blockers = graph
        .dependencies
        .iter()
        .filter(|dependency| dependency.task_id == id)
        .filter_map(|dependency| graph.tasks.iter().find(|task| task.id == dependency.blocker_id))
        .map(|task| BlockerView {
            task: task.clone(),
            completed: task.status == TaskStatus::Completed,
            evidence: task.evidence.clone(),
        })
        .collect::<Vec<_>>();
    let dependents = graph
        .dependencies
        .iter()
        .filter(|dependency| dependency.blocker_id == id)
        .filter_map(|dependency| graph.tasks.iter().find(|task| task.id == dependency.task_id).cloned())
        .collect::<Vec<_>>();
    let children = graph
        .tasks
        .iter()
        .filter(|child| child.parent_id == Some(id))
        .cloned()
        .collect::<Vec<_>>();
    let ancestors = ancestors(graph, id);
    Ok(TaskView {
        readiness: readiness(graph, &task),
        progress: graph.task_progress(id),
        task,
        blockers,
        dependents,
        children,
        ancestors,
        milestone,
        epic,
        release,
    })
}

fn ancestors(graph: &Graph, id: TaskId) -> Vec<Task> {
    let by_id = graph
        .tasks
        .iter()
        .map(|task| (task.id, task))
        .collect::<HashMap<_, _>>();
    let mut chain = Vec::new();
    let mut current = by_id.get(&id).and_then(|task| task.parent_id);
    let mut seen = HashSet::new();
    while let Some(parent_id) = current {
        if !seen.insert(parent_id) {
            break;
        }
        let Some(parent) = by_id.get(&parent_id) else {
            break;
        };
        chain.push((*parent).clone());
        current = parent.parent_id;
    }
    chain.reverse();
    chain
}

fn readiness(graph: &Graph, task: &Task) -> Readiness {
    let mut reasons = Vec::new();
    if task.status != TaskStatus::Pending {
        reasons.push(format!("task status is `{}`", task.status.as_str()));
    }
    let children = graph
        .tasks
        .iter()
        .filter(|child| child.parent_id == Some(task.id))
        .collect::<Vec<_>>();
    if !children.is_empty() {
        reasons.push(format!("task has {} child task(s)", children.len()));
    }
    for ancestor in ancestors(graph, task.id) {
        if matches!(
            ancestor.status,
            TaskStatus::Parked | TaskStatus::Completed | TaskStatus::Cancelled
        ) {
            reasons.push(format!("ancestor `{}` is `{}`", ancestor.id, ancestor.status.as_str()));
        }
    }
    let milestone = graph
        .milestones
        .iter()
        .find(|milestone| milestone.id == task.milestone_id);
    let epic = milestone.and_then(|milestone| graph.epics.iter().find(|epic| epic.id == milestone.epic_id));
    if milestone.is_none_or(|milestone| milestone.status != ContainerStatus::Open) {
        reasons.push("milestone is not open".to_owned());
    }
    if epic.is_none_or(|epic| epic.status != ContainerStatus::Open) {
        reasons.push("epic is not open".to_owned());
    }
    if let Some(epic) = epic
        && let Some(release_id) = epic.release_id
        && graph
            .releases
            .iter()
            .find(|release| release.id == release_id)
            .is_none_or(|release| release.status != ContainerStatus::Open)
    {
        reasons.push("release is not open".to_owned());
    }
    let mut blocked = false;
    for dependency in graph
        .dependencies
        .iter()
        .filter(|dependency| dependency.task_id == task.id)
    {
        match graph
            .tasks
            .iter()
            .find(|candidate| candidate.id == dependency.blocker_id)
        {
            Some(blocker) if blocker.status == TaskStatus::Completed => {}
            Some(blocker) => {
                blocked = true;
                reasons.push(format!("blocked by `{}` ({})", blocker.id, blocker.status.as_str()));
            }
            None => {
                blocked = true;
                reasons.push(format!("blocked by missing task `{}`", dependency.blocker_id));
            }
        }
    }
    Readiness { ready: reasons.is_empty(), blocked, reasons }
}

/// Convert a typed-ID parse failure into the storage error category used by queries.
struct DomainErrorToStorage;
impl DomainErrorToStorage {
    fn into_storage(error: crate::domain::IdError) -> StorageError {
        StorageError::InvalidTask(crate::domain::DomainError::InvalidId(error))
    }
}

pub fn check(connection: &Connection) -> Result<CheckReport> {
    let graph = load(connection)?;
    let mut report = CheckReport::default();
    let integrity: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        report.errors.push(format!("SQLite integrity check: {integrity}"));
    }
    let fk_violations = connection
        .prepare("PRAGMA foreign_key_check")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for table in fk_violations {
        report.errors.push(format!("foreign-key violation in `{table}`"));
    }

    let task_ids = graph.tasks.iter().map(|task| task.id).collect::<HashSet<_>>();
    let milestone_ids = graph
        .milestones
        .iter()
        .map(|milestone| milestone.id)
        .collect::<HashSet<_>>();
    let epic_ids = graph.epics.iter().map(|epic| epic.id).collect::<HashSet<_>>();
    let release_ids = graph.releases.iter().map(|release| release.id).collect::<HashSet<_>>();
    for epic in &graph.epics {
        if let Some(release_id) = epic.release_id
            && !release_ids.contains(&release_id)
        {
            report
                .errors
                .push(format!("epic `{}` references missing release `{release_id}`", epic.id));
        }
    }
    for task in &graph.tasks {
        if !milestone_ids.contains(&task.milestone_id) {
            report
                .errors
                .push(format!("task `{}` references missing milestone", task.id));
        }
        if let Some(parent_id) = task.parent_id {
            match graph.tasks.iter().find(|candidate| candidate.id == parent_id) {
                None => report
                    .errors
                    .push(format!("task `{}` references missing parent `{parent_id}`", task.id)),
                Some(parent) if parent.milestone_id != task.milestone_id => report.errors.push(format!(
                    "task `{}` and parent `{parent_id}` are in different milestones",
                    task.id
                )),
                Some(_) => {}
            }
        }
    }
    for milestone in &graph.milestones {
        if !epic_ids.contains(&milestone.epic_id) {
            report
                .errors
                .push(format!("milestone `{}` references missing epic", milestone.id));
        }
    }
    for dependency in &graph.dependencies {
        if !task_ids.contains(&dependency.task_id) || !task_ids.contains(&dependency.blocker_id) {
            report.errors.push(format!(
                "dependency `{}` -> `{}` references a missing task",
                dependency.task_id, dependency.blocker_id
            ));
        }
    }
    for task in &graph.tasks {
        if has_parent_cycle(&graph.tasks, task.id) {
            report.errors.push(format!("parent cycle includes `{}`", task.id));
        }
    }
    for task in &graph.tasks {
        if has_dependency_cycle(&graph.dependencies, task.id) {
            report.errors.push(format!("dependency cycle includes `{}`", task.id));
        }
    }
    for idea in &graph.ideas {
        match (idea.status, idea.promoted_to) {
            (crate::domain::IdeaStatus::Promoted, Some(epic_id))
                if graph
                    .epics
                    .iter()
                    .find(|epic| epic.id == epic_id)
                    .is_none_or(|epic| epic.source_idea != Some(idea.id)) =>
            {
                report
                    .errors
                    .push(format!("idea `{}` promotion is not symmetric", idea.id));
            }
            (crate::domain::IdeaStatus::Promoted, None) => {
                report.errors.push(format!("promoted idea `{}` has no epic", idea.id))
            }
            (_, Some(_)) => report
                .errors
                .push(format!("idea `{}` has a promotion but is not promoted", idea.id)),
            _ => {}
        }
    }
    for epic in &graph.epics {
        if let Some(idea_id) = epic.source_idea
            && graph
                .ideas
                .iter()
                .find(|idea| idea.id == idea_id)
                .is_none_or(|idea| idea.promoted_to != Some(epic.id))
        {
            report
                .errors
                .push(format!("epic `{}` source-idea link is not symmetric", epic.id));
        }
    }
    duplicate_positions(&graph, &mut report);
    report.valid = report.errors.is_empty();
    Ok(report)
}

fn duplicate_positions(graph: &Graph, report: &mut CheckReport) {
    let mut milestones = HashSet::new();
    for milestone in &graph.milestones {
        if !milestones.insert((milestone.epic_id, milestone.position)) {
            report.warnings.push(format!(
                "duplicate milestone position {} in epic `{}`",
                milestone.position, milestone.epic_id
            ));
        }
    }
    let mut tasks = HashSet::new();
    for task in &graph.tasks {
        if !tasks.insert((task.milestone_id, task.parent_id, task.position)) {
            report.warnings.push(format!(
                "duplicate task position {} in milestone `{}`",
                task.position, task.milestone_id
            ));
        }
    }
}

fn has_parent_cycle(tasks: &[Task], start: TaskId) -> bool {
    let by_id = tasks
        .iter()
        .map(|task| (task.id, task.parent_id))
        .collect::<HashMap<_, _>>();
    let mut seen = HashSet::new();
    let mut current = Some(start);
    while let Some(id) = current {
        if !seen.insert(id) {
            return true;
        }
        current = by_id.get(&id).copied().flatten();
    }
    false
}

fn has_dependency_cycle(dependencies: &[TaskDependency], start: TaskId) -> bool {
    let mut current = vec![start];
    let mut seen = HashSet::new();
    while let Some(id) = current.pop() {
        if !seen.insert(id) {
            return id == start;
        }
        current.extend(
            dependencies
                .iter()
                .filter(|dependency| dependency.task_id == id)
                .map(|dependency| dependency.blocker_id),
        );
    }
    false
}
