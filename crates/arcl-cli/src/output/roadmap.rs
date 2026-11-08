//! `arcl roadmap`: specs with their plans and milestones, then releases, as
//! one Markdown page.
//!
//! The page is built from the same tree `arcl tree` prints, so its task
//! counts match the tree's: completed tasks over all tasks at any depth,
//! leaving out cancelled tasks. Records keep the tree's order: specs, plans
//! and releases by ID, milestones by position, then ID.

use std::collections::BTreeMap;

use serde::Serialize;

use arcl_core::domain::ReleaseMembership;

use super::{ConnectedTreeNode, StatusMark, task_progress};

/// The state a roadmap shows for a spec, plan, milestone or release.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RoadmapState {
    /// Completed.
    Done,
    /// Cancelled.
    Cancelled,
    /// Open, with a task in progress or completed under it.
    InProgress,
    /// Open, with no task started under it.
    Planned,
}

impl RoadmapState {
    /// The state for a record with the `arcl tree` mark `mark`.
    const fn of(mark: StatusMark) -> Self {
        match mark {
            StatusMark::Done => Self::Done,
            StatusMark::Cancelled => Self::Cancelled,
            StatusMark::Active => Self::InProgress,
            StatusMark::Parked | StatusMark::Ready | StatusMark::Waiting => Self::Planned,
        }
    }

    /// The word the Markdown page prints.
    const fn label(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Cancelled => "cancelled",
            Self::InProgress => "in progress",
            Self::Planned => "planned",
        }
    }
}

/// Completed tasks over counted tasks, as `arcl tree` counts them.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct RoadmapProgress {
    /// Completed tasks.
    pub done: usize,
    /// Tasks that are not cancelled.
    pub total: usize,
}

impl RoadmapProgress {
    /// `done/total`, or `no tasks`.
    fn label(self) -> String {
        if self.total == 0 { "no tasks".to_owned() } else { format!("{}/{}", self.done, self.total) }
    }

    /// `done/total tasks`, or `no tasks`.
    fn sentence(self) -> String {
        if self.total == 0 { self.label() } else { format!("{} tasks", self.label()) }
    }
}

/// One record on the roadmap.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RoadmapItem {
    /// The full record ID.
    pub id: String,
    /// The record title.
    pub title: String,
    /// The stored status.
    pub status: String,
    /// The state the page shows.
    pub state: RoadmapState,
    /// The tasks under the record.
    pub progress: RoadmapProgress,
}

impl RoadmapItem {
    fn new(node: &ConnectedTreeNode, state: RoadmapState, progress: RoadmapProgress) -> Self {
        Self { id: node.id.clone(), title: node.title.clone(), status: node.status.clone(), state, progress }
    }

    /// The item for a spec, plan or phase, counting the tasks below it.
    fn of(node: &ConnectedTreeNode) -> Self {
        let (done, total) = task_progress(&node.children);
        Self::new(node, RoadmapState::of(node.mark), RoadmapProgress { done, total })
    }

    /// The line below a spec or plan heading: its ID, state and task count.
    fn status_line(&self) -> String {
        format!("`{}` · {} · {}", self.id, self.state.label(), self.progress.sentence())
    }

    /// The table row for a milestone or release.
    fn row(&self) -> String {
        format!(
            "| {} | {} | {} |",
            escape_markdown(&self.title),
            self.state.label(),
            self.progress.label()
        )
    }
}

/// A plan and its milestones. A cancelled plan lists none.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RoadmapPlan {
    #[serde(flatten)]
    pub plan: RoadmapItem,
    /// The plan's phases in position order.
    pub milestones: Vec<RoadmapItem>,
}

/// A spec and its plans. A cancelled spec lists none.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RoadmapSpec {
    #[serde(flatten)]
    pub spec: RoadmapItem,
    /// The spec's plans in ID order.
    pub plans: Vec<RoadmapPlan>,
}

/// The data of an `arcl roadmap` page. Captures, notes and tasks outside a
/// spec are left out.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Roadmap {
    /// Specs in ID order.
    pub specs: Vec<RoadmapSpec>,
    /// Releases in ID order. A release counts the tasks it holds and the
    /// tasks under the specs and plans it holds, each task once.
    pub releases: Vec<RoadmapItem>,
}

impl Roadmap {
    /// Build the roadmap from the unrooted `arcl tree` nodes, with full IDs,
    /// and every release membership.
    pub fn new(nodes: &[ConnectedTreeNode], memberships: &[ReleaseMembership]) -> Self {
        let specs = nodes
            .iter()
            .filter(|node| node.kind == "spec")
            .map(|spec| {
                let item = RoadmapItem::of(spec);
                let plans = if item.state == RoadmapState::Cancelled {
                    Vec::new()
                } else {
                    spec.children
                        .iter()
                        .filter(|node| node.kind == "plan")
                        .map(roadmap_plan)
                        .collect()
                };
                RoadmapSpec { spec: item, plans }
            })
            .collect();
        let releases = nodes
            .iter()
            .filter(|node| node.kind == "release")
            .map(|release| roadmap_release(release, nodes, memberships))
            .collect();
        Self { specs, releases }
    }

    /// The Markdown page, with no trailing newline.
    pub fn markdown(&self) -> String {
        let mut blocks = vec!["# Roadmap".to_owned()];
        if self.specs.is_empty() && self.releases.is_empty() {
            blocks.push("No specs or releases.".to_owned());
        }
        for spec in &self.specs {
            blocks.push(format!("## {}", escape_markdown(&spec.spec.title)));
            blocks.push(spec.spec.status_line());
            for plan in &spec.plans {
                blocks.push(format!("### {}", escape_markdown(&plan.plan.title)));
                blocks.push(plan.plan.status_line());
                if !plan.milestones.is_empty() {
                    blocks.push(table("Milestone", &plan.milestones));
                }
            }
        }
        if !self.releases.is_empty() {
            blocks.push("## Releases".to_owned());
            blocks.push(table("Release", &self.releases));
        }
        blocks.join("\n\n")
    }
}

/// Escape `text` for a Markdown heading or table cell: inline markup
/// characters and pipes everywhere, and a leading `#`, `-`, `+` or `=`.
/// Line breaks and other control characters become spaces.
pub fn escape_markdown(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for (index, character) in text.chars().enumerate() {
        match character {
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '|' | '~' | '&' => escaped.push('\\'),
            '#' | '-' | '+' | '=' if index == 0 => escaped.push('\\'),
            _ => {}
        }
        escaped.push(if character.is_control() { ' ' } else { character });
    }
    escaped
}

/// A plan and its phases. A cancelled plan lists no milestones.
fn roadmap_plan(plan: &ConnectedTreeNode) -> RoadmapPlan {
    let item = RoadmapItem::of(plan);
    let milestones = if item.state == RoadmapState::Cancelled {
        Vec::new()
    } else {
        plan.children
            .iter()
            .filter(|node| node.kind == "phase")
            .map(RoadmapItem::of)
            .collect()
    };
    RoadmapPlan { plan: item, milestones }
}

/// A release with the tasks its members hold. An open release is in progress
/// once one of those tasks is in progress or completed.
fn roadmap_release(
    release: &ConnectedTreeNode, nodes: &[ConnectedTreeNode], memberships: &[ReleaseMembership],
) -> RoadmapItem {
    let mut tasks = BTreeMap::new();
    for membership in memberships
        .iter()
        .filter(|membership| membership.release_id.to_string() == release.id)
    {
        if let Some(node) = find_node(nodes, &membership.record_id) {
            collect_tasks(node, &mut tasks);
        }
    }
    let progress = tasks
        .values()
        .fold(RoadmapProgress::default(), |progress, mark| match mark {
            StatusMark::Cancelled => progress,
            StatusMark::Done => RoadmapProgress { done: progress.done + 1, total: progress.total + 1 },
            _ => RoadmapProgress { total: progress.total + 1, ..progress },
        });
    let started = tasks
        .values()
        .any(|mark| matches!(mark, StatusMark::Done | StatusMark::Active));
    let state = match RoadmapState::of(release.mark) {
        RoadmapState::Planned if started => RoadmapState::InProgress,
        state => state,
    };
    RoadmapItem::new(release, state, progress)
}

/// The node with `id` at any depth of `nodes`.
fn find_node<'a>(nodes: &'a [ConnectedTreeNode], id: &str) -> Option<&'a ConnectedTreeNode> {
    nodes
        .iter()
        .find_map(|node| if node.id == id { Some(node) } else { find_node(&node.children, id) })
}

/// Add `node`, when it is a task, and every task below it to `tasks`.
fn collect_tasks<'a>(node: &'a ConnectedTreeNode, tasks: &mut BTreeMap<&'a str, StatusMark>) {
    if node.kind == "task" {
        tasks.insert(&node.id, node.mark);
    }
    for child in &node.children {
        collect_tasks(child, tasks);
    }
}

/// A Markdown table of milestones or releases under the first column
/// `heading`.
fn table(heading: &str, items: &[RoadmapItem]) -> String {
    let mut rows = vec![
        format!("| {heading} | Status | Tasks |"),
        "| --- | --- | --- |".to_owned(),
    ];
    rows.extend(items.iter().map(RoadmapItem::row));
    rows.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(
        kind: &'static str, id: &str, title: &str, mark: StatusMark, children: Vec<ConnectedTreeNode>,
    ) -> ConnectedTreeNode {
        let status = match mark {
            StatusMark::Done => "completed",
            StatusMark::Cancelled => "cancelled",
            _ if kind == "task" => "pending",
            _ => "open",
        };
        ConnectedTreeNode {
            kind,
            id: id.to_owned(),
            title: title.to_owned(),
            status: status.to_owned(),
            mark,
            blocked_by: Vec::new(),
            children,
        }
    }

    fn task(id: &str, mark: StatusMark) -> ConnectedTreeNode {
        node("task", id, id, mark, Vec::new())
    }

    #[test]
    fn escapes_pipes_markup_and_leading_block_markers() {
        assert_eq!(escape_markdown("a | b"), "a \\| b");
        assert_eq!(escape_markdown("# not a heading"), "\\# not a heading");
        assert_eq!(escape_markdown("- not a list"), "\\- not a list");
        assert_eq!(escape_markdown("C# and x-y"), "C# and x-y");
        assert_eq!(
            escape_markdown("*bold* `code` [link]"),
            "\\*bold\\* \\`code\\` \\[link\\]"
        );
        assert_eq!(escape_markdown("two\nlines"), "two lines");
    }

    #[test]
    fn renders_specs_plans_milestones_and_releases() {
        let phases = vec![
            node(
                "phase",
                "ph-1",
                "Milestone 1: Done",
                StatusMark::Done,
                vec![task("t-1", StatusMark::Done), task("t-2", StatusMark::Cancelled)],
            ),
            node(
                "phase",
                "ph-2",
                "Milestone 2: A | B",
                StatusMark::Active,
                vec![task("t-3", StatusMark::Done), task("t-4", StatusMark::Ready)],
            ),
            node("phase", "ph-3", "Milestone 3", StatusMark::Waiting, Vec::new()),
        ];
        let nodes = vec![
            node(
                "spec",
                "s-1",
                "Spec",
                StatusMark::Active,
                vec![node("plan", "pl-1", "Plan", StatusMark::Active, phases)],
            ),
            node(
                "spec",
                "s-2",
                "Dropped",
                StatusMark::Cancelled,
                vec![node("plan", "pl-2", "Hidden", StatusMark::Waiting, Vec::new())],
            ),
            node("release", "r-1", "v1", StatusMark::Waiting, Vec::new()),
        ];
        let roadmap = Roadmap::new(&nodes, &[]);
        assert_eq!(
            roadmap.markdown(),
            "# Roadmap

## Spec

`s-1` · in progress · 2/3 tasks

### Plan

`pl-1` · in progress · 2/3 tasks

| Milestone | Status | Tasks |
| --- | --- | --- |
| Milestone 1: Done | done | 1/1 |
| Milestone 2: A \\| B | in progress | 1/2 |
| Milestone 3 | planned | no tasks |

## Dropped

`s-2` · cancelled · no tasks

## Releases

| Release | Status | Tasks |
| --- | --- | --- |
| v1 | planned | no tasks |"
        );
    }

    #[test]
    fn an_empty_project_says_so() {
        assert_eq!(Roadmap::default().markdown(), "# Roadmap\n\nNo specs or releases.");
    }
}
