//! The schema's object types. Each wraps a stored record or read model;
//! statuses and kinds are the snake_case strings JSON output uses.

use std::sync::OnceLock;

use async_graphql::{Context, ID, Interface, Object, SimpleObject};

use super::Reader;
use arcl_core::prelude::*;
use arcl_repo::workspace::RecordSource;
use arcl_store::prelude::*;
use arcl_store::{PlanningWorkItem, RecordTreeNode, list_records};

/// Any record a project holds. Every record has an ID, a kind, a title and a
/// status; a note's status is `available`.
// Clippy reads the repeated `ty` keys of the `field(...)` lists as duplicates.
#[allow(clippy::duplicated_attributes)]
#[derive(Interface)]
#[graphql(
    name = "Record",
    field(name = "id", ty = "ID", desc = "The full record ID."),
    field(
        name = "short_id",
        ty = "String",
        desc = "The shortest unique ID suffix, as human output shows it."
    ),
    field(
        name = "kind",
        ty = "String",
        desc = "`capture`, `release`, `spec`, `plan`, `phase`, `task` or `note`."
    ),
    field(name = "title", ty = "String"),
    field(name = "status", ty = "String"),
    field(name = "source", ty = "Option<SourceNode>", desc = "The record's workspace file.")
)]
pub enum RecordNode {
    Capture(CaptureNode),
    Release(ReleaseNode),
    Spec(SpecNode),
    Plan(PlanNode),
    Phase(PhaseNode),
    Task(TaskNode),
    Note(NoteNode),
}

impl RecordNode {
    /// The record with the full ID `id` in the graph, if there is one.
    pub(crate) fn find(graph: &ConnectedGraph, id: &str) -> Option<Self> {
        if id.starts_with(CaptureId::PREFIX) {
            graph
                .captures
                .iter()
                .find(|record| record.id.to_string() == id)
                .cloned()
                .map(|record| Self::Capture(CaptureNode(record)))
        } else if id.starts_with(ReleaseId::PREFIX) {
            graph
                .releases
                .iter()
                .find(|record| record.id.to_string() == id)
                .cloned()
                .map(|record| Self::Release(ReleaseNode(record)))
        } else if id.starts_with(SpecId::PREFIX) {
            graph
                .specs
                .iter()
                .find(|record| record.id.to_string() == id)
                .cloned()
                .map(|record| Self::Spec(SpecNode(record)))
        } else if id.starts_with(PlanId::PREFIX) {
            graph
                .plans
                .iter()
                .find(|record| record.id.to_string() == id)
                .cloned()
                .map(|record| Self::Plan(PlanNode(record)))
        } else if id.starts_with(PhaseId::PREFIX) {
            graph
                .phases
                .iter()
                .find(|record| record.id.to_string() == id)
                .cloned()
                .map(|record| Self::Phase(PhaseNode(record)))
        } else if id.starts_with(TaskId::PREFIX) {
            graph
                .tasks
                .iter()
                .find(|record| record.id.to_string() == id)
                .cloned()
                .map(|record| Self::Task(TaskNode::new(record)))
        } else if id.starts_with(NoteId::PREFIX) {
            graph
                .notes
                .iter()
                .find(|record| record.id.to_string() == id)
                .cloned()
                .map(|record| Self::Note(NoteNode(record)))
        } else {
            None
        }
    }

    /// The records `filter` matches, in `arcl list` order.
    pub(crate) fn list(reader: &Reader, filter: &RecordFilter) -> async_graphql::Result<Vec<Self>> {
        let graph = reader.graph()?;
        let summaries = list_records(graph, filter).map_err(super::storage_error)?;
        Ok(summaries
            .iter()
            .filter_map(|summary| Self::find(graph, &summary.id))
            .collect())
    }
}

/// An inbox capture.
pub struct CaptureNode(pub Capture);

#[Object(name = "Capture")]
impl CaptureNode {
    async fn id(&self) -> ID {
        ID(self.0.id.to_string())
    }
    async fn short_id(&self, ctx: &Context<'_>) -> async_graphql::Result<String> {
        short_id(ctx, &self.0.id.to_string())
    }
    async fn kind(&self) -> String {
        "capture".to_owned()
    }
    async fn title(&self) -> String {
        self.0.title.clone()
    }
    async fn status(&self) -> String {
        self.0.status.as_str().to_owned()
    }
    async fn source(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<SourceNode>> {
        source(ctx, &self.0.id.to_string())
    }
    /// The Markdown body.
    async fn body(&self) -> &str {
        &self.0.body
    }
    /// When the capture was filed, in RFC 3339.
    async fn created_at(&self) -> &str {
        &self.0.created_at
    }
    /// The task the capture was filed from.
    async fn source_task(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<TaskNode>> {
        task_by_id(ctx, self.0.source_task_id)
    }
    /// The spec, task or note the capture was promoted to.
    async fn promoted_to(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<RecordNode>> {
        let graph = reader(ctx).graph()?;
        let target = graph
            .capture_promotions
            .iter()
            .find(|promotion| promotion.capture_id == self.0.id)
            .map(|promotion| match promotion.target {
                CapturePromotionTarget::Spec(id) => id.to_string(),
                CapturePromotionTarget::Task(id) => id.to_string(),
                CapturePromotionTarget::Note(id) => id.to_string(),
            });
        Ok(target.and_then(|id| RecordNode::find(graph, &id)))
    }
}

/// A named release and the records that belong to it.
pub struct ReleaseNode(pub Release);

#[Object(name = "Release")]
impl ReleaseNode {
    async fn id(&self) -> ID {
        ID(self.0.id.to_string())
    }
    async fn short_id(&self, ctx: &Context<'_>) -> async_graphql::Result<String> {
        short_id(ctx, &self.0.id.to_string())
    }
    async fn kind(&self) -> String {
        "release".to_owned()
    }
    async fn title(&self) -> String {
        self.0.title.clone()
    }
    async fn status(&self) -> String {
        self.0.status.as_str().to_owned()
    }
    async fn source(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<SourceNode>> {
        source(ctx, &self.0.id.to_string())
    }
    /// The Markdown description.
    async fn description(&self) -> &str {
        &self.0.description
    }
    /// The specs, plans, tasks and notes in the release, as
    /// `arcl list --release` lists them.
    async fn members(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<RecordNode>> {
        let filter = RecordFilter { release_id: Some(self.0.id), ..RecordFilter::default() };
        Ok(RecordNode::list(reader(ctx), &filter)?
            .into_iter()
            .filter(|record| !matches!(record, RecordNode::Release(_)))
            .collect())
    }
}

/// A specification.
pub struct SpecNode(pub Spec);

#[Object(name = "Spec")]
impl SpecNode {
    async fn id(&self) -> ID {
        ID(self.0.id.to_string())
    }
    async fn short_id(&self, ctx: &Context<'_>) -> async_graphql::Result<String> {
        short_id(ctx, &self.0.id.to_string())
    }
    async fn kind(&self) -> String {
        "spec".to_owned()
    }
    async fn title(&self) -> String {
        self.0.title.clone()
    }
    async fn status(&self) -> String {
        self.0.status.as_str().to_owned()
    }
    async fn source(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<SourceNode>> {
        source(ctx, &self.0.id.to_string())
    }
    /// The Markdown body.
    async fn body(&self) -> &str {
        &self.0.body
    }
    /// The Markdown acceptance criteria.
    async fn acceptance_criteria(&self) -> &str {
        &self.0.acceptance_criteria
    }
    /// The capture promoted to this spec.
    async fn source_capture(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<CaptureNode>> {
        let graph = reader(ctx).graph()?;
        Ok(self
            .0
            .source_capture_id
            .and_then(|id| graph.captures.iter().find(|capture| capture.id == id))
            .cloned()
            .map(CaptureNode))
    }
    /// The task the capture promoted to this spec was filed from.
    async fn source_task(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<TaskNode>> {
        promoted_source_task(ctx, &self.0.id.to_string())
    }
    /// The spec's plans, in ID order.
    async fn plans(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<PlanNode>> {
        let graph = reader(ctx).graph()?;
        Ok(graph
            .plans
            .iter()
            .filter(|plan| plan.spec_id == self.0.id)
            .cloned()
            .map(PlanNode)
            .collect())
    }
    /// Every task under the spec, as `arcl list --kind task --spec` lists them.
    async fn tasks(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<TaskNode>> {
        tasks(ctx, RecordFilter { spec_id: Some(self.0.id), ..task_filter() })
    }
}

/// A plan under a spec.
pub struct PlanNode(pub Plan);

#[Object(name = "Plan")]
impl PlanNode {
    async fn id(&self) -> ID {
        ID(self.0.id.to_string())
    }
    async fn short_id(&self, ctx: &Context<'_>) -> async_graphql::Result<String> {
        short_id(ctx, &self.0.id.to_string())
    }
    async fn kind(&self) -> String {
        "plan".to_owned()
    }
    async fn title(&self) -> String {
        self.0.title.clone()
    }
    async fn status(&self) -> String {
        self.0.status.as_str().to_owned()
    }
    async fn source(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<SourceNode>> {
        source(ctx, &self.0.id.to_string())
    }
    /// The Markdown body.
    async fn body(&self) -> &str {
        &self.0.body
    }
    /// The spec the plan belongs to.
    async fn spec(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<SpecNode>> {
        let graph = reader(ctx).graph()?;
        Ok(graph
            .specs
            .iter()
            .find(|spec| spec.id == self.0.spec_id)
            .cloned()
            .map(SpecNode))
    }
    /// The plan's phases, in position order.
    async fn phases(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<PhaseNode>> {
        let graph = reader(ctx).graph()?;
        let mut phases = graph
            .phases
            .iter()
            .filter(|phase| phase.plan_id == self.0.id)
            .cloned()
            .collect::<Vec<_>>();
        phases.sort_by(|left, right| left.position.cmp(&right.position).then_with(|| left.id.cmp(&right.id)));
        Ok(phases.into_iter().map(PhaseNode).collect())
    }
    /// Every task under the plan, as `arcl list --kind task --plan` lists them.
    async fn tasks(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<TaskNode>> {
        tasks(ctx, RecordFilter { plan_id: Some(self.0.id), ..task_filter() })
    }
}

/// A phase of a plan.
pub struct PhaseNode(pub Phase);

#[Object(name = "Phase")]
impl PhaseNode {
    async fn id(&self) -> ID {
        ID(self.0.id.to_string())
    }
    async fn short_id(&self, ctx: &Context<'_>) -> async_graphql::Result<String> {
        short_id(ctx, &self.0.id.to_string())
    }
    async fn kind(&self) -> String {
        "phase".to_owned()
    }
    async fn title(&self) -> String {
        self.0.title.clone()
    }
    async fn status(&self) -> String {
        self.0.status.as_str().to_owned()
    }
    async fn source(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<SourceNode>> {
        source(ctx, &self.0.id.to_string())
    }
    /// The Markdown body.
    async fn body(&self) -> &str {
        &self.0.body
    }
    /// The phase's order within its plan.
    async fn position(&self) -> i64 {
        self.0.position
    }
    /// The key a structured plan document gave the phase.
    async fn plan_key(&self) -> Option<&str> {
        self.0.plan_key.as_deref()
    }
    /// The plan the phase belongs to.
    async fn plan(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<PlanNode>> {
        let graph = reader(ctx).graph()?;
        Ok(graph
            .plans
            .iter()
            .find(|plan| plan.id == self.0.plan_id)
            .cloned()
            .map(PlanNode))
    }
    /// Every task under the phase, as `arcl list --kind task --phase` lists them.
    async fn tasks(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<TaskNode>> {
        tasks(ctx, RecordFilter { phase_id: Some(self.0.id), ..task_filter() })
    }
}

/// A task. Its readiness, blockers, gates and notes come from the same view
/// `arcl explain` shows.
pub struct TaskNode {
    task: PlanningTask,
    view: OnceLock<Box<PlanningTaskView>>,
}

impl TaskNode {
    pub(crate) fn new(task: PlanningTask) -> Self {
        Self { task, view: OnceLock::new() }
    }

    /// The task's `arcl explain` view, read on first use.
    fn view(&self, ctx: &Context<'_>) -> async_graphql::Result<&PlanningTaskView> {
        if let Some(view) = self.view.get() {
            return Ok(view);
        }
        let view = reader(ctx).read(|database| database.planning_task_view(self.task.id))?;
        Ok(self.view.get_or_init(|| Box::new(view)))
    }
}

#[Object(name = "Task")]
impl TaskNode {
    async fn id(&self) -> ID {
        ID(self.task.id.to_string())
    }
    async fn short_id(&self, ctx: &Context<'_>) -> async_graphql::Result<String> {
        short_id(ctx, &self.task.id.to_string())
    }
    async fn kind(&self) -> String {
        "task".to_owned()
    }
    async fn title(&self) -> String {
        self.task.title.clone()
    }
    async fn status(&self) -> String {
        self.task.status.as_str().to_owned()
    }
    async fn source(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<SourceNode>> {
        source(ctx, &self.task.id.to_string())
    }
    /// The Markdown body.
    async fn body(&self) -> &str {
        &self.task.body
    }
    /// `urgent`, `high`, `normal` or `low`.
    async fn priority(&self) -> &str {
        self.task.priority.as_str()
    }
    /// The task's order among its siblings.
    async fn position(&self) -> i64 {
        self.task.position
    }
    /// The key a structured plan document gave the task.
    async fn plan_key(&self) -> Option<&str> {
        self.task.plan_key.as_deref()
    }
    /// The latest handoff, in Markdown.
    async fn handoff(&self) -> &str {
        &self.task.handoff
    }
    /// The completion evidence, in Markdown.
    async fn evidence(&self) -> &str {
        &self.task.evidence
    }
    /// The name that claimed the task.
    async fn assignee(&self) -> Option<&str> {
        self.task.assignee.as_deref()
    }
    /// The parent task.
    async fn parent(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<TaskNode>> {
        task_by_id(ctx, self.task.parent_id)
    }
    /// Parent tasks from the oldest ancestor to the immediate parent.
    async fn ancestors(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<TaskNode>> {
        Ok(task_nodes(&self.view(ctx)?.ancestors))
    }
    /// Direct subtasks.
    async fn children(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<TaskNode>> {
        Ok(task_nodes(&self.view(ctx)?.children))
    }
    /// The effective spec: the task's own, or the one it sits under.
    async fn spec(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<SpecNode>> {
        Ok(self.view(ctx)?.spec.clone().map(SpecNode))
    }
    /// The effective plan.
    async fn plan(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<PlanNode>> {
        Ok(self.view(ctx)?.plan.clone().map(PlanNode))
    }
    /// The effective phase.
    async fn phase(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<PhaseNode>> {
        Ok(self.view(ctx)?.phase.clone().map(PhaseNode))
    }
    /// Whether the task is ready, and why not, as `arcl explain` reports it.
    async fn readiness(&self, ctx: &Context<'_>) -> async_graphql::Result<ReadinessNode> {
        Ok(ReadinessNode(self.view(ctx)?.readiness.clone()))
    }
    /// Direct blockers, with their evidence.
    async fn blockers(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<BlockerNode>> {
        Ok(blocker_nodes(&self.view(ctx)?.blockers))
    }
    /// Blockers held by ancestor tasks, nearest ancestor first.
    async fn inherited_blockers(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<BlockerNode>> {
        Ok(blocker_nodes(&self.view(ctx)?.inherited_blockers))
    }
    /// Tasks that the task blocks directly.
    async fn dependents(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<TaskNode>> {
        Ok(task_nodes(&self.view(ctx)?.dependents))
    }
    /// Every note on the task, newest first.
    async fn notes(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<TaskNoteNode>> {
        Ok(self.view(ctx)?.notes.iter().cloned().map(TaskNoteNode).collect())
    }
    /// Every gate on the task, open or met, oldest first.
    async fn gates(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<GateNode>> {
        Ok(self.view(ctx)?.gates.iter().cloned().map(GateNode).collect())
    }
    /// The task the capture promoted to this task was filed from.
    async fn source_task(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<TaskNode>> {
        task_by_id(ctx, self.view(ctx)?.source_task_id)
    }
    /// The task's context packet, as `arcl context` prints it.
    async fn context(&self, ctx: &Context<'_>) -> async_graphql::Result<ContextNode> {
        let context = reader(ctx).read(|database| database.planning_context(self.task.id))?;
        Ok(ContextNode(context))
    }
}

/// A Markdown note.
pub struct NoteNode(pub Note);

#[Object(name = "Note")]
impl NoteNode {
    async fn id(&self) -> ID {
        ID(self.0.id.to_string())
    }
    async fn short_id(&self, ctx: &Context<'_>) -> async_graphql::Result<String> {
        short_id(ctx, &self.0.id.to_string())
    }
    async fn kind(&self) -> String {
        "note".to_owned()
    }
    async fn title(&self) -> String {
        self.0.title.clone()
    }
    async fn status(&self) -> String {
        "available".to_owned()
    }
    async fn source(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<SourceNode>> {
        source(ctx, &self.0.id.to_string())
    }
    /// The Markdown body.
    async fn body(&self) -> &str {
        &self.0.body
    }
    /// The task the capture promoted to this note was filed from.
    async fn source_task(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<TaskNode>> {
        promoted_source_task(ctx, &self.0.id.to_string())
    }
}

/// A gate a task waits on.
pub struct GateNode(pub Gate);

#[Object(name = "Gate")]
impl GateNode {
    async fn id(&self) -> ID {
        ID(self.0.id.to_string())
    }
    async fn short_id(&self, ctx: &Context<'_>) -> async_graphql::Result<String> {
        short_id(ctx, &self.0.id.to_string())
    }
    /// `pr`, `ci` or another gate kind.
    async fn kind(&self) -> &str {
        self.0.kind.as_str()
    }
    /// What the gate waits on, such as a pull request URL.
    async fn target(&self) -> &str {
        &self.0.target
    }
    /// `open` or `met`.
    async fn status(&self) -> &str {
        self.0.status.as_str()
    }
    /// When the gate was added, in RFC 3339.
    async fn created_at(&self) -> &str {
        &self.0.created_at
    }
    /// When the gate was met, in RFC 3339.
    async fn met_at(&self) -> Option<&str> {
        self.0.met_at.as_deref()
    }
    /// The task the gate is on.
    async fn task(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<TaskNode>> {
        task_by_id(ctx, Some(self.0.task_id))
    }
}

/// A note on a task.
pub struct TaskNoteNode(pub TaskNote);

#[Object(name = "TaskNote")]
impl TaskNoteNode {
    /// When the note was added, in RFC 3339.
    async fn created_at(&self) -> &str {
        &self.0.created_at
    }
    /// The note's kind, such as `decision`, if it has one.
    async fn kind(&self) -> Option<&str> {
        self.0.kind.map(TaskNoteKind::as_str)
    }
    /// The note text.
    async fn text(&self) -> &str {
        &self.0.text
    }
}

/// A task's readiness.
pub struct ReadinessNode(pub PlanningReadiness);

#[Object(name = "Readiness")]
impl ReadinessNode {
    /// Whether the task can start now.
    async fn ready(&self) -> bool {
        self.0.ready
    }
    /// Whether a blocker holds the task.
    async fn blocked(&self) -> bool {
        self.0.blocked
    }
    /// Why the task is not ready, one reason each.
    async fn reasons(&self) -> Vec<ReasonNode> {
        self.0.reasons.iter().cloned().map(ReasonNode).collect()
    }
}

/// One reason a task is not ready.
pub struct ReasonNode(pub ReadinessReason);

#[Object(name = "ReadinessReason")]
impl ReasonNode {
    /// The reason kind in snake_case, such as `direct_blocker`.
    async fn kind(&self) -> String {
        serde_json::to_value(self.0.kind)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default()
    }
    /// The reason as `arcl explain` words it.
    async fn message(&self) -> &str {
        &self.0.message
    }
    /// The task the reason is about.
    async fn task(&self) -> Option<ID> {
        self.0.task.map(|id| ID(id.to_string()))
    }
    /// The ancestor task that holds the reason.
    async fn ancestor(&self) -> Option<ID> {
        self.0.ancestor.map(|id| ID(id.to_string()))
    }
    /// The blocking task.
    async fn blocker(&self) -> Option<ID> {
        self.0.blocker.map(|id| ID(id.to_string()))
    }
    /// The closed spec, plan or phase.
    async fn container(&self) -> Option<ID> {
        self.0.container.clone().map(ID)
    }
    /// The tasks along an invalid blocker edge.
    async fn path(&self) -> Vec<ID> {
        self.0.path.iter().map(|id| ID(id.to_string())).collect()
    }
    /// The open gate.
    async fn gate(&self) -> Option<GateNode> {
        self.0.gate.clone().map(GateNode)
    }
}

/// A task that blocks another, with its completion evidence.
pub struct BlockerNode(pub PlanningBlockerView);

#[Object(name = "Blocker")]
impl BlockerNode {
    /// The blocking task.
    async fn task(&self) -> TaskNode {
        TaskNode::new(self.0.task.clone())
    }
    /// Whether the blocker has completed.
    async fn completed(&self) -> bool {
        self.0.completed
    }
    /// The blocker's completion evidence, in Markdown.
    async fn evidence(&self) -> &str {
        &self.0.evidence
    }
    /// The ancestor task that holds the dependency, for an inherited blocker.
    async fn ancestor(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<TaskNode>> {
        task_by_id(ctx, self.0.ancestor)
    }
}

/// A task's context packet, as `arcl context` prints it.
pub struct ContextNode(pub PlanningContext);

#[Object(name = "TaskContext")]
impl ContextNode {
    /// The task.
    async fn task(&self) -> TaskNode {
        TaskNode::new(self.0.task.clone())
    }
    /// Parent tasks from the oldest ancestor to the immediate parent.
    async fn ancestors(&self) -> Vec<TaskNode> {
        task_nodes(&self.0.ancestors)
    }
    /// The effective spec.
    async fn spec(&self) -> Option<SpecNode> {
        self.0.spec.clone().map(SpecNode)
    }
    /// The effective plan.
    async fn plan(&self) -> Option<PlanNode> {
        self.0.plan.clone().map(PlanNode)
    }
    /// The effective phase.
    async fn phase(&self) -> Option<PhaseNode> {
        self.0.phase.clone().map(PhaseNode)
    }
    /// Direct blockers and their evidence.
    async fn blockers(&self) -> Vec<BlockerNode> {
        blocker_nodes(&self.0.blockers)
    }
    /// Blockers held by ancestor tasks, nearest ancestor first.
    async fn inherited_blockers(&self) -> Vec<BlockerNode> {
        blocker_nodes(&self.0.inherited_blockers)
    }
    /// Tasks that the task blocks directly.
    async fn dependents(&self) -> Vec<TaskNode> {
        task_nodes(&self.0.dependents)
    }
    /// The task's readiness.
    async fn readiness(&self) -> ReadinessNode {
        ReadinessNode(self.0.readiness.clone())
    }
    /// Completed blockers that supplied evidence, one per blocker task.
    async fn completion_evidence(&self) -> Vec<BlockerNode> {
        blocker_nodes(&self.0.completion_evidence)
    }
    /// The most recent notes, newest first.
    async fn notes(&self) -> Vec<TaskNoteNode> {
        self.0.notes.iter().cloned().map(TaskNoteNode).collect()
    }
    /// How many older notes the packet leaves out.
    async fn omitted_notes(&self) -> usize {
        self.0.omitted_notes
    }
    /// Every gate on the task, open or met, oldest first.
    async fn gates(&self) -> Vec<GateNode> {
        self.0.gates.iter().cloned().map(GateNode).collect()
    }
}

/// Every task with its readiness, and every phase, as the web app's Work
/// list reads them.
pub struct WorkNode(pub PlanningWork);

#[Object(name = "Work")]
impl WorkNode {
    /// Every task, ordered by position, then ID.
    async fn tasks(&self) -> Vec<WorkItemNode> {
        self.0.tasks.iter().cloned().map(WorkItemNode).collect()
    }
    /// Every phase, ordered by plan, then position, then ID.
    async fn phases(&self) -> Vec<PhaseNode> {
        self.0.phases.iter().cloned().map(PhaseNode).collect()
    }
}

/// One task in the work list.
pub struct WorkItemNode(pub PlanningWorkItem);

#[Object(name = "WorkItem")]
impl WorkItemNode {
    /// The task.
    async fn task(&self) -> TaskNode {
        TaskNode::new(self.0.task.clone())
    }
    /// The task's readiness.
    async fn readiness(&self) -> ReadinessNode {
        ReadinessNode(self.0.readiness.clone())
    }
    /// The task's effective phase: its own, or its nearest ancestor's.
    async fn phase_id(&self) -> Option<ID> {
        self.0.phase_id.map(|id| ID(id.to_string()))
    }
}

/// A node of the record tree, as `arcl tree` builds it.
pub struct TreeNode(pub RecordTreeNode);

#[Object(name = "TreeNode")]
impl TreeNode {
    /// The full record ID.
    async fn id(&self) -> ID {
        ID(self.0.id.clone())
    }
    /// The record kind.
    async fn kind(&self) -> &str {
        self.0.kind
    }
    /// The record title.
    async fn title(&self) -> &str {
        &self.0.title
    }
    /// The stored status; a note's is `available`.
    async fn status(&self) -> &str {
        &self.0.status
    }
    /// For a pending task, whether it is ready.
    async fn ready(&self) -> bool {
        matches!(self.0.tree_status, TreeStatus::Task { ready: true, .. })
    }
    /// The unfinished tasks that block this task directly. A rooted tree's
    /// root task also lists those it inherits.
    async fn blocked_by(&self) -> Vec<ID> {
        self.0.blocked_by.iter().cloned().map(ID).collect()
    }
    /// Child records.
    async fn children(&self) -> Vec<TreeNode> {
        self.0.children.iter().cloned().map(TreeNode).collect()
    }
    /// The record itself.
    async fn record(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<RecordNode>> {
        Ok(RecordNode::find(reader(ctx).graph()?, &self.0.id))
    }
}

/// A record's workspace file, for the web app's raw editor.
#[derive(SimpleObject)]
#[graphql(name = "Source")]
pub struct SourceNode {
    /// The record ID.
    pub id: ID,
    /// The record kind.
    pub kind: String,
    /// The path relative to the workspace root, such as `tasks/<id>.md`.
    pub path: String,
    /// TOML front matter between `+++` lines, then the Markdown body.
    pub content: String,
    /// The SHA-256 of the content, in lowercase hex. An edit names the
    /// revision it started from.
    pub revision: String,
}

impl From<RecordSource> for SourceNode {
    fn from(source: RecordSource) -> Self {
        Self {
            id: ID(source.id),
            kind: source.kind.as_str().to_owned(),
            path: source.path.to_string_lossy().replace('\\', "/"),
            content: source.content,
            revision: source.revision,
        }
    }
}

/// The reader the query runs against.
pub(crate) fn reader<'a>(ctx: &Context<'a>) -> &'a Reader {
    ctx.data_unchecked::<Reader>()
}

/// A filter for every task.
pub(crate) fn task_filter() -> RecordFilter {
    RecordFilter { kind: Some("task".to_owned()), ..RecordFilter::default() }
}

/// The tasks `filter` matches, in `arcl list` order.
pub(crate) fn tasks(ctx: &Context<'_>, filter: RecordFilter) -> async_graphql::Result<Vec<TaskNode>> {
    Ok(RecordNode::list(reader(ctx), &filter)?
        .into_iter()
        .filter_map(|record| match record {
            RecordNode::Task(task) => Some(task),
            _ => None,
        })
        .collect())
}

fn short_id(ctx: &Context<'_>, id: &str) -> async_graphql::Result<String> {
    reader(ctx).short_id(id).map(str::to_owned)
}

fn source(ctx: &Context<'_>, id: &str) -> async_graphql::Result<Option<SourceNode>> {
    Ok(reader(ctx).source(id)?.map(SourceNode::from))
}

fn task_by_id(ctx: &Context<'_>, id: Option<TaskId>) -> async_graphql::Result<Option<TaskNode>> {
    let Some(id) = id else { return Ok(None) };
    let graph = reader(ctx).graph()?;
    Ok(graph
        .tasks
        .iter()
        .find(|task| task.id == id)
        .cloned()
        .map(TaskNode::new))
}

fn promoted_source_task(ctx: &Context<'_>, id: &str) -> async_graphql::Result<Option<TaskNode>> {
    let source = reader(ctx).read(|database| database.promoted_source_task(id))?;
    task_by_id(ctx, source)
}

fn task_nodes(tasks: &[PlanningTask]) -> Vec<TaskNode> {
    tasks.iter().cloned().map(TaskNode::new).collect()
}

fn blocker_nodes(blockers: &[PlanningBlockerView]) -> Vec<BlockerNode> {
    blockers.iter().cloned().map(BlockerNode).collect()
}
