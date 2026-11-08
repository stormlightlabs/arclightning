//! `arcl roadmap`: specs with their plans and phases, then releases, as one
//! Markdown page.
//!
//! The page is built from the same tree `arcl tree` prints, so its task
//! counts match the tree's: completed tasks over all tasks at any depth,
//! leaving out cancelled tasks. Tasks directly under a spec or plan get their
//! own line or row, so the numbers on the page add up to each total. Records
//! keep the tree's order: specs, plans and releases by ID, phases by position,
//! then ID.

use std::collections::BTreeMap;

use serde::Serialize;

use arcl_core::domain::ReleaseMembership;

use super::{ConnectedTreeNode, StatusMark, task_progress};

/// The label of the line for tasks directly under a spec.
const TASKS_OUTSIDE_PLANS: &str = "Tasks outside plans";
/// The label of the row for tasks directly under a plan.
const TASKS_OUTSIDE_PHASES: &str = "Tasks outside phases";

/// The state a roadmap shows for a spec, plan, phase or release.
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

    /// The table row for a phase or release.
    fn row(&self) -> String {
        format!(
            "| {} | {} | {} |",
            escape_markdown(&self.title),
            self.state.label(),
            self.progress.label()
        )
    }
}

/// A plan, its phases and the tasks directly under it. A cancelled plan lists
/// neither.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RoadmapPlan {
    #[serde(flatten)]
    pub plan: RoadmapItem,
    /// The plan's phases in position order.
    pub phases: Vec<RoadmapItem>,
    /// The tasks directly under the plan, or `None` when it has none.
    pub tasks_outside_phases: Option<RoadmapTasks>,
}

/// A spec, its plans and the tasks directly under it. A cancelled spec lists
/// neither.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RoadmapSpec {
    #[serde(flatten)]
    pub spec: RoadmapItem,
    /// The spec's plans in ID order.
    pub plans: Vec<RoadmapPlan>,
    /// The tasks directly under the spec, or `None` when it has none.
    pub tasks_outside_plans: Option<RoadmapTasks>,
}

/// Tasks directly under a spec or plan, outside its plans or phases, with
/// the tasks below them.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct RoadmapTasks {
    /// `done` when every counted task is completed, `cancelled` when every
    /// task is cancelled, `in_progress` once one is started, else `planned`.
    pub state: RoadmapState,
    /// The tasks, counted as `arcl tree` counts them.
    pub progress: RoadmapProgress,
}

impl RoadmapTasks {
    /// The tasks among `children` that are not under a child of kind
    /// `grouped`, or `None` when there are none.
    fn outside(children: &[ConnectedTreeNode], grouped: &str) -> Option<Self> {
        let mut marks = BTreeMap::new();
        for node in children.iter().filter(|node| node.kind != grouped) {
            collect_tasks(node, &mut marks);
        }
        if marks.is_empty() {
            return None;
        }
        let progress = count_tasks(&marks);
        let started = marks
            .values()
            .any(|mark| matches!(mark, StatusMark::Done | StatusMark::Active));
        let state = if progress.total == 0 {
            RoadmapState::Cancelled
        } else if progress.done == progress.total {
            RoadmapState::Done
        } else if started {
            RoadmapState::InProgress
        } else {
            RoadmapState::Planned
        };
        Some(Self { state, progress })
    }

    /// The line below a spec's status line. The label is in italics, which
    /// no escaped title can produce.
    fn line(self, label: &str) -> String {
        format!("*{label}* · {} · {}", self.state.label(), self.progress.sentence())
    }

    /// The table row after a plan's phases.
    fn row(self, label: &str) -> String {
        format!("| *{label}* | {} | {} |", self.state.label(), self.progress.label())
    }
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
                if item.state == RoadmapState::Cancelled {
                    return RoadmapSpec { spec: item, plans: Vec::new(), tasks_outside_plans: None };
                }
                let plans = spec
                    .children
                    .iter()
                    .filter(|node| node.kind == "plan")
                    .map(roadmap_plan)
                    .collect();
                let tasks_outside_plans = RoadmapTasks::outside(&spec.children, "plan");
                RoadmapSpec { spec: item, plans, tasks_outside_plans }
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
            if let Some(tasks) = spec.tasks_outside_plans {
                blocks.push(tasks.line(TASKS_OUTSIDE_PLANS));
            }
            for plan in &spec.plans {
                blocks.push(format!("### {}", escape_markdown(&plan.plan.title)));
                blocks.push(plan.plan.status_line());
                let mut rows: Vec<String> = plan.phases.iter().map(RoadmapItem::row).collect();
                rows.extend(plan.tasks_outside_phases.map(|tasks| tasks.row(TASKS_OUTSIDE_PHASES)));
                if !rows.is_empty() {
                    blocks.push(table("Phase", rows));
                }
            }
        }
        if !self.releases.is_empty() {
            blocks.push("## Releases".to_owned());
            blocks.push(table("Release", self.releases.iter().map(RoadmapItem::row).collect()));
        }
        blocks.join("\n\n")
    }
}

/// Escape `text` for a Markdown heading or table cell: inline markup
/// characters and pipes everywhere, a leading `#`, `-`, `+` or `=`, and a
/// trailing run of `#` after a space, which a heading would drop as its
/// closing sequence. Line breaks and other control characters become spaces.
pub fn escape_markdown(text: &str) -> String {
    let characters: Vec<char> = text
        .chars()
        .map(|character| if character.is_control() { ' ' } else { character })
        .collect();
    let content_end = characters
        .iter()
        .rposition(|character| !character.is_whitespace())
        .map_or(0, |index| index + 1);
    let run_start = characters[..content_end]
        .iter()
        .rposition(|character| *character != '#')
        .map_or(0, |index| index + 1);
    let closing =
        (run_start < content_end && (run_start == 0 || characters[run_start - 1].is_whitespace())).then_some(run_start);
    let mut escaped = String::with_capacity(text.len());
    for (index, &character) in characters.iter().enumerate() {
        match character {
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '|' | '~' | '&' => escaped.push('\\'),
            '#' | '-' | '+' | '=' if index == 0 => escaped.push('\\'),
            '#' if closing == Some(index) => escaped.push('\\'),
            _ => {}
        }
        escaped.push(character);
    }
    escaped
}

/// A plan, its phases and the tasks directly under it. A cancelled plan
/// lists neither.
fn roadmap_plan(plan: &ConnectedTreeNode) -> RoadmapPlan {
    let item = RoadmapItem::of(plan);
    if item.state == RoadmapState::Cancelled {
        return RoadmapPlan { plan: item, phases: Vec::new(), tasks_outside_phases: None };
    }
    let phases = plan
        .children
        .iter()
        .filter(|node| node.kind == "phase")
        .map(RoadmapItem::of)
        .collect();
    let tasks_outside_phases = RoadmapTasks::outside(&plan.children, "phase");
    RoadmapPlan { plan: item, phases, tasks_outside_phases }
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
    let progress = count_tasks(&tasks);
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

/// Completed tasks over the tasks in `tasks` that are not cancelled.
fn count_tasks(tasks: &BTreeMap<&str, StatusMark>) -> RoadmapProgress {
    tasks
        .values()
        .fold(RoadmapProgress::default(), |progress, mark| match mark {
            StatusMark::Cancelled => progress,
            StatusMark::Done => RoadmapProgress { done: progress.done + 1, total: progress.total + 1 },
            _ => RoadmapProgress { total: progress.total + 1, ..progress },
        })
}

/// A Markdown table of phase or release `rows` under the first column
/// `heading`.
fn table(heading: &str, rows: Vec<String>) -> String {
    let mut lines = vec![
        format!("| {heading} | Status | Tasks |"),
        "| --- | --- | --- |".to_owned(),
    ];
    lines.extend(rows);
    lines.join("\n")
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
    fn escapes_a_trailing_run_of_hashes_after_a_space() {
        assert_eq!(escape_markdown("Plan ##"), "Plan \\##");
        assert_eq!(escape_markdown("Plan # "), "Plan \\# ");
        assert_eq!(escape_markdown("Plan\n#"), "Plan \\#");
        assert_eq!(escape_markdown("##"), "\\##");
        assert_eq!(escape_markdown("C#"), "C#");
        assert_eq!(escape_markdown("Issue #4"), "Issue #4");
    }

    #[test]
    fn rows_for_tasks_outside_plans_and_phases_add_up_to_each_total() {
        let phase = node(
            "phase",
            "ph-1",
            "Phase 1",
            StatusMark::Active,
            vec![task("t-1", StatusMark::Done), task("t-2", StatusMark::Ready)],
        );
        let plan = node(
            "plan",
            "pl-1",
            "Plan",
            StatusMark::Active,
            vec![
                phase,
                task("t-3", StatusMark::Active),
                task("t-4", StatusMark::Cancelled),
            ],
        );
        let finished = node(
            "plan",
            "pl-2",
            "Finished",
            StatusMark::Done,
            vec![task("t-5", StatusMark::Done)],
        );
        let nodes = vec![node(
            "spec",
            "s-1",
            "Spec",
            StatusMark::Active,
            vec![
                plan,
                finished,
                node(
                    "task",
                    "t-6",
                    "t-6",
                    StatusMark::Ready,
                    vec![task("t-7", StatusMark::Done)],
                ),
            ],
        )];
        let roadmap = Roadmap::new(&nodes, &[]);
        let spec = &roadmap.specs[0];
        let sum = |parts: &[RoadmapProgress]| {
            parts
                .iter()
                .fold((0, 0), |(done, total), part| (done + part.done, total + part.total))
        };
        let mut spec_parts: Vec<RoadmapProgress> = spec.plans.iter().map(|plan| plan.plan.progress).collect();
        spec_parts.extend(spec.tasks_outside_plans.map(|tasks| tasks.progress));
        assert_eq!(sum(&spec_parts), (spec.spec.progress.done, spec.spec.progress.total));
        assert_eq!(spec.spec.progress, RoadmapProgress { done: 3, total: 6 });
        for plan in &spec.plans {
            let mut parts: Vec<RoadmapProgress> = plan.phases.iter().map(|phase| phase.progress).collect();
            parts.extend(plan.tasks_outside_phases.map(|tasks| tasks.progress));
            assert_eq!(sum(&parts), (plan.plan.progress.done, plan.plan.progress.total));
        }
        assert_eq!(
            roadmap.markdown(),
            "# Roadmap

## Spec

`s-1` · in progress · 3/6 tasks

*Tasks outside plans* · in progress · 1/2 tasks

### Plan

`pl-1` · in progress · 1/3 tasks

| Phase | Status | Tasks |
| --- | --- | --- |
| Phase 1 | in progress | 1/2 |
| *Tasks outside phases* | in progress | 0/1 |

### Finished

`pl-2` · done · 1/1 tasks

| Phase | Status | Tasks |
| --- | --- | --- |
| *Tasks outside phases* | done | 1/1 |"
        );
    }

    #[test]
    fn a_plan_with_only_cancelled_direct_tasks_shows_them_cancelled() {
        let nodes = vec![node(
            "spec",
            "s-1",
            "Spec",
            StatusMark::Waiting,
            vec![node(
                "plan",
                "pl-1",
                "Plan",
                StatusMark::Waiting,
                vec![task("t-1", StatusMark::Cancelled)],
            )],
        )];
        let roadmap = Roadmap::new(&nodes, &[]);
        assert_eq!(
            roadmap.specs[0].plans[0].tasks_outside_phases,
            Some(RoadmapTasks { state: RoadmapState::Cancelled, progress: RoadmapProgress::default() })
        );
        assert_eq!(roadmap.specs[0].tasks_outside_plans, None);
    }

    #[test]
    fn renders_specs_plans_phases_and_releases() {
        let phases = vec![
            node(
                "phase",
                "ph-1",
                "Phase 1: Done",
                StatusMark::Done,
                vec![task("t-1", StatusMark::Done), task("t-2", StatusMark::Cancelled)],
            ),
            node(
                "phase",
                "ph-2",
                "Phase 2: A | B",
                StatusMark::Active,
                vec![task("t-3", StatusMark::Done), task("t-4", StatusMark::Ready)],
            ),
            node("phase", "ph-3", "Phase 3", StatusMark::Waiting, Vec::new()),
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

| Phase | Status | Tasks |
| --- | --- | --- |
| Phase 1: Done | done | 1/1 |
| Phase 2: A \\| B | in progress | 1/2 |
| Phase 3 | planned | no tasks |

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
