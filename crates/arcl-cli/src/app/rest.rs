//! The reads behind `arcl serve`'s `GET /api` endpoints and `arcl web
//! export`. Each runs one named GraphQL query against the schema `arcl query`
//! runs, and maps the result into the envelope the CLI's `--json` output
//! gives for the same read, with the same `format_version`.
//!
//! The queries alias each field to the name the envelope's record type
//! serializes it under, so the result deserializes into that type and the
//! renderer writes it as the CLI does. A relation selected as `{ id }` alone,
//! such as `spec_id: spec { id }`, stands for the ID it holds. A GraphQL
//! error maps to the error envelope the CLI prints for the same failure,
//! through its `extensions.code`.

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::support::*;
use super::*;

/// The fields of a `PlanningTask`.
macro_rules! planning_task {
    () => {
        "fragment PlanningTask on Task {
            id project_id: projectId spec_id: specId plan_id: planId phase_id: phaseId parent_id: parentId
            plan_key: planKey title body status priority position handoff evidence assignee
        }"
    };
}

/// The fields of a `Spec`.
macro_rules! spec {
    () => {
        "fragment Spec on Spec {
            id project_id: projectId title body acceptance_criteria: acceptanceCriteria status
            source_capture_id: sourceCapture { id }
        }"
    };
}

/// The fields of a `Plan`.
macro_rules! plan {
    () => {
        "fragment Plan on Plan { id project_id: projectId spec_id: spec { id } title body status }"
    };
}

/// The fields of a `Phase`.
macro_rules! phase {
    () => {
        "fragment Phase on Phase {
            id project_id: projectId plan_id: plan { id } plan_key: planKey title body status position
        }"
    };
}

/// The fields of a `Gate`.
macro_rules! gate {
    () => {
        "fragment Gate on Gate {
            id task_id: taskId kind target status created_at: createdAt met_at: metAt
        }"
    };
}

/// The fields of a `PlanningReadiness`. Uses the `Gate` fragment.
macro_rules! readiness {
    () => {
        "fragment Readiness on Readiness {
            ready blocked reasons { kind message task ancestor blocker container path gate { ...Gate } }
        }"
    };
}

/// The fields of a `PlanningBlockerView`. Uses the `PlanningTask` fragment.
macro_rules! blocker {
    () => {
        "fragment Blocker on Blocker {
            task { ...PlanningTask } completed evidence ancestor { id }
        }"
    };
}

/// The fields of a `TaskNote`.
macro_rules! task_note {
    () => {
        "fragment TaskNote on TaskNote { task_id: taskId created_at: createdAt kind text }"
    };
}

/// The fields of a `PlanningTaskView`, but for `task`, whose fields sit
/// beside the others.
macro_rules! view {
    () => {
        concat!(
            "fragment View on Task {
                ...PlanningTask
                readiness { ...Readiness }
                blockers { ...Blocker }
                inherited_blockers: inheritedBlockers { ...Blocker }
                dependents { ...PlanningTask }
                children { ...PlanningTask }
                ancestors { ...PlanningTask }
                spec { ...Spec }
                plan { ...Plan }
                phase { ...Phase }
                notes { ...TaskNote }
                gates { ...Gate }
                source_task_id: sourceTask { id }
            }",
            planning_task!(),
            readiness!(),
            blocker!(),
            spec!(),
            plan!(),
            phase!(),
            task_note!(),
            gate!(),
        )
    };
}

/// `GET /api/records`: `arcl list`.
const RECORDS: &str = "query Records(
    $kind: String, $status: [String!]!, $priority: [String!]!, $release: ID, $spec: ID, $plan: ID, $phase: ID,
    $parent: ID
) {
    records(
        kind: $kind, status: $status, priority: $priority, release: $release, spec: $spec, plan: $plan,
        phase: $phase, parent: $parent
    ) { kind id title status }
}";

/// `GET /api/records/{id}`: `arcl show`.
const RECORD: &str = concat!(
    "query Record($id: ID!) {
        record(id: $id) {
            __typename
            ... on Capture {
                id project_id: projectId title body status created_at: createdAt
                source_task_id: sourceTask { id }
            }
            ... on Release { id title description status }
            ... on Spec { ...Spec source_task_id: sourceTask { id } }
            ... on Plan { ...Plan }
            ... on Phase { ...Phase }
            ... on Task { ...View }
            ... on Note { id project_id: projectId title body source_task_id: sourceTask { id } }
            ... on Gate { ...Gate }
        }
    }",
    view!(),
);

/// `GET /api/records/{id}/source`: the record's workspace file.
const SOURCE: &str = "query Source($id: ID!) { source(id: $id) { id kind path content revision } }";

/// `GET /api/ready`: `arcl ready`.
const READY: &str = concat!(
    "query Ready($priority: [String!]!, $spec: ID, $plan: ID, $phase: ID, $parent: ID) {
        ready(priority: $priority, spec: $spec, plan: $plan, phase: $phase, parent: $parent) { ...PlanningTask }
    }",
    planning_task!(),
);

/// `GET /api/tasks/{id}/explain`: `arcl explain`.
const EXPLAIN: &str = concat!("query Explain($id: ID!) { task(id: $id) { ...View } }", view!());

/// `GET /api/tasks/{id}/context`: `arcl context`.
const CONTEXT: &str = concat!(
    "query Context($id: ID!) {
        context(task: $id) {
            task { ...PlanningTask }
            ancestors { ...PlanningTask }
            spec { ...Spec }
            plan { ...Plan }
            phase { ...Phase }
            blockers { ...Blocker }
            inherited_blockers: inheritedBlockers { ...Blocker }
            dependents { ...PlanningTask }
            readiness { ...Readiness }
            completion_evidence: completionEvidence { ...Blocker }
            notes { ...TaskNote }
            omitted_notes: omittedNotes
            gates { ...Gate }
        }
    }",
    planning_task!(),
    spec!(),
    plan!(),
    phase!(),
    blocker!(),
    readiness!(),
    task_note!(),
    gate!(),
);

/// `GET /api/tree`: `arcl tree` of every record. `descendants` reads a tree
/// of any depth in one query.
const TREE: &str = "query Tree {
    tree { ...TreeFields descendants { ...TreeFields depth } }
}
fragment TreeFields on TreeNode { kind id title status blocked_by: blockedBy }";

/// `GET /api/work`: every task with its readiness and phase, and every phase.
const WORK: &str = concat!(
    "query Work {
        work {
            tasks { task { ...PlanningTask } readiness { ...Readiness } phase_id: phaseId }
            phases { ...Phase }
        }
    }",
    planning_task!(),
    readiness!(),
    gate!(),
    phase!(),
);

/// A record `arcl list` lists.
#[derive(Deserialize)]
struct Summary {
    kind: String,
    id: String,
    title: String,
    status: String,
}

/// A task as [`EXPLAIN`] reads it: a [`PlanningTaskView`] whose task fields
/// sit beside the others.
#[derive(Deserialize)]
struct View {
    #[serde(flatten)]
    task: PlanningTask,
    readiness: arcl_store::PlanningReadiness,
    blockers: Vec<arcl_store::PlanningBlockerView>,
    inherited_blockers: Vec<arcl_store::PlanningBlockerView>,
    dependents: Vec<PlanningTask>,
    children: Vec<PlanningTask>,
    ancestors: Vec<PlanningTask>,
    spec: Option<Spec>,
    plan: Option<Plan>,
    phase: Option<Phase>,
    notes: Vec<TaskNote>,
    gates: Vec<Gate>,
    source_task_id: Option<TaskId>,
}

impl View {
    fn into_view(self) -> arcl_store::PlanningTaskView {
        arcl_store::PlanningTaskView {
            task: self.task,
            readiness: self.readiness,
            blockers: self.blockers,
            inherited_blockers: self.inherited_blockers,
            dependents: self.dependents,
            children: self.children,
            ancestors: self.ancestors,
            spec: self.spec,
            plan: self.plan,
            phase: self.phase,
            notes: self.notes,
            gates: self.gates,
            source_task_id: self.source_task_id,
        }
    }
}

/// A record `arcl show` shows: its fields, and the task a promoted spec or
/// note came from.
#[derive(Deserialize)]
struct Sourced<T> {
    #[serde(flatten)]
    record: T,
    source_task_id: Option<TaskId>,
}

/// A record's workspace file.
#[derive(Deserialize)]
struct Source {
    id: String,
    kind: String,
    path: String,
    content: String,
    revision: String,
}

/// A node of the record tree, with its depth below the top for a
/// descendant.
#[derive(Deserialize)]
struct TreeEntry {
    kind: String,
    id: String,
    title: String,
    status: String,
    blocked_by: Vec<String>,
    #[serde(default)]
    depth: usize,
    #[serde(default)]
    descendants: Vec<TreeEntry>,
}

/// `arcl list` with `filters`.
pub(super) fn records(filters: &ListArgs, renderer: &Renderer) -> anyhow::Result<Option<String>> {
    let variables = json!({
        "kind": filters.kind,
        "status": filters.status,
        "priority": filters.priority,
        "release": filters.release,
        "spec": filters.spec,
        "plan": filters.plan,
        "phase": filters.phase,
        "parent": filters.parent,
    });
    let summaries = run::<Vec<Summary>>(RECORDS, variables, "records")?
        .into_iter()
        .map(|summary| {
            Ok(ConnectedSummary::new(
                record_kind(&summary.kind)?,
                summary.id,
                summary.title,
                &summary.status,
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(renderer.render_connected_summaries(&summaries)?)
}

/// `arcl show <id>`.
pub(super) fn record(id: &str, renderer: &Renderer) -> anyhow::Result<Option<String>> {
    let record = run::<Value>(RECORD, json!({ "id": id }), "record")?;
    let ids = ShortIds::default();
    let from = |source: Option<TaskId>| source.map(|id| SourceTask { id, ids: &ids, beside_record: true });
    let rendered = match record["__typename"].as_str().unwrap_or_default() {
        "Capture" => {
            let capture = decode::<Capture>(record)?;
            let source = capture
                .source_task_id
                .map(|id| SourceTask { id, ids: &ids, beside_record: false });
            renderer.render_shown_from("capture", &capture.id.to_string(), &capture.title, &capture, source)
        }
        "Release" => {
            let release = decode::<Release>(record)?;
            renderer.render_shown("release", &release.id.to_string(), &release.title, &release)
        }
        "Spec" => {
            let spec = decode::<Sourced<Spec>>(record)?;
            let (id, title) = (spec.record.id.to_string(), &spec.record.title);
            renderer.render_shown_from("spec", &id, title, &spec.record, from(spec.source_task_id))
        }
        "Plan" => {
            let plan = decode::<Plan>(record)?;
            renderer.render_shown("plan", &plan.id.to_string(), &plan.title, &plan)
        }
        "Phase" => {
            let phase = decode::<Phase>(record)?;
            renderer.render_shown("phase", &phase.id.to_string(), &phase.title, &phase)
        }
        "Task" => renderer.render_planning_task_view(&decode::<View>(record)?.into_view(), &ids),
        "Note" => {
            let note = decode::<Sourced<Note>>(record)?;
            let (id, title) = (note.record.id.to_string(), &note.record.title);
            renderer.render_shown_from("note", &id, title, &note.record, from(note.source_task_id))
        }
        "Gate" => renderer.render_shown_gate(&decode::<Gate>(record)?, &ids),
        other => anyhow::bail!("`record` answered an unknown type `{other}`"),
    };
    Ok(rendered?)
}

/// The workspace file of the record `id`, a full ID.
pub(super) fn source(id: &str, renderer: &Renderer) -> anyhow::Result<Option<String>> {
    // The raw editor names records by full ID; a short one names no file.
    if !is_full_id(id) {
        return Err(ApplicationError::from_command(RecordEditError::NotFound { id: id.to_owned() }).into());
    }
    let source = run::<Source>(SOURCE, json!({ "id": id }), "source")?;
    let kind = [
        RecordKind::Capture,
        RecordKind::Release,
        RecordKind::Spec,
        RecordKind::Plan,
        RecordKind::Phase,
        RecordKind::Task,
        RecordKind::Note,
    ]
    .into_iter()
    .find(|kind| kind.as_str() == source.kind)
    .with_context(|| format!("`source` answered an unknown kind `{}`", source.kind))?;
    let source = RecordSource {
        id: source.id,
        kind,
        path: PathBuf::from(source.path),
        content: source.content,
        revision: source.revision,
    };
    Ok(renderer.render_record_source(&source)?)
}

/// `arcl ready` with `filters`.
pub(super) fn ready(filters: &ReadyArgs, renderer: &Renderer) -> anyhow::Result<Option<String>> {
    let variables = json!({
        "priority": filters.priority,
        "spec": filters.spec,
        "plan": filters.plan,
        "phase": filters.phase,
        "parent": filters.parent,
    });
    let tasks = run::<Vec<PlanningTask>>(READY, variables, "ready")?;
    Ok(renderer.render_planning_ready_tasks(&tasks)?)
}

/// `arcl explain <task-id>`.
pub(super) fn explain(task_id: &str, renderer: &Renderer) -> anyhow::Result<Option<String>> {
    let view = run::<View>(EXPLAIN, json!({ "id": task_id }), "task")?.into_view();
    Ok(renderer.render_planning_task_view(&view, &ShortIds::default())?)
}

/// `arcl context <task-id>`.
pub(super) fn context(task_id: &str, renderer: &Renderer) -> anyhow::Result<Option<String>> {
    let context = run::<arcl_store::PlanningContext>(CONTEXT, json!({ "id": task_id }), "context")?;
    Ok(renderer.render_planning_context(&context)?)
}

/// `arcl tree` of every record.
pub(super) fn tree(renderer: &Renderer) -> anyhow::Result<Option<String>> {
    let nodes = run::<Vec<TreeEntry>>(TREE, json!({}), "tree")?
        .into_iter()
        .map(nest)
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(renderer.render_connected_tree(&nodes, TreeView::default())?)
}

/// Every task with its readiness and effective phase, and every phase.
pub(super) fn work(renderer: &Renderer) -> anyhow::Result<Option<String>> {
    let work = run::<arcl_store::PlanningWork>(WORK, json!({}), "work")?;
    Ok(renderer.render_planning_work(&work)?)
}

/// Run `document` with `variables` and decode the root field `field` of the
/// result. A GraphQL error comes back as the [`ApplicationError`] the CLI
/// gives for the same failure.
fn run<T: DeserializeOwned>(document: &str, variables: Value, field: &str) -> anyhow::Result<T> {
    let database = open_database().map_err(ApplicationError::from_command)?;
    let request = arcl_graphql::Request::new(document).variables(arcl_graphql::Variables::from_json(variables));
    let response = arcl_graphql::execute_unbounded(database, request);
    if let Some(error) = response.errors.first() {
        let code = error
            .extensions
            .as_ref()
            .and_then(|extensions| extensions.get("code"))
            .and_then(|code| match code {
                arcl_graphql::Value::String(code) => Some(code.as_str()),
                _ => None,
            });
        return Err(ApplicationError::from_query(code, &error.message).into());
    }
    let mut data = response.data.into_json().context("reading the GraphQL response")?;
    decode(data[field].take())
}

/// Decode a query result into `T`, each relation selected as `{ id }` alone
/// read as its ID.
fn decode<T: DeserializeOwned>(value: Value) -> anyhow::Result<T> {
    serde_json::from_value(relations_as_ids(value)).context("reading the GraphQL response")
}

/// Replace each object whose only field is `id` with that ID.
fn relations_as_ids(value: Value) -> Value {
    match value {
        Value::Object(fields) if fields.len() == 1 && fields.contains_key("id") => {
            fields.into_iter().next().map_or(Value::Null, |(_, id)| id)
        }
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(name, value)| (name, relations_as_ids(value)))
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.into_iter().map(relations_as_ids).collect()),
        value => value,
    }
}

/// The tree node `entry` heads, with its descendants nested under it by
/// depth.
fn nest(mut entry: TreeEntry) -> anyhow::Result<ConnectedTreeNode> {
    let descendants = std::mem::take(&mut entry.descendants);
    let top = tree_node(&entry)?;
    // `stack[d]` is the open node at depth `d`; each descendant closes the
    // nodes at its depth or deeper and opens itself.
    let mut stack = vec![top];
    for descendant in descendants {
        anyhow::ensure!(descendant.depth > 0, "`tree` answered a descendant at depth 0");
        while stack.len() > descendant.depth {
            close(&mut stack);
        }
        stack.push(tree_node(&descendant)?);
    }
    while stack.len() > 1 {
        close(&mut stack);
    }
    stack.pop().context("the tree has a top node")
}

/// Move the deepest open node into its parent's children.
fn close(stack: &mut Vec<ConnectedTreeNode>) {
    if let Some(node) = stack.pop()
        && let Some(parent) = stack.last_mut()
    {
        parent.children.push(node);
    }
}

/// A tree node without its children.
fn tree_node(entry: &TreeEntry) -> anyhow::Result<ConnectedTreeNode> {
    Ok(ConnectedTreeNode {
        kind: record_kind(&entry.kind)?,
        id: entry.id.clone(),
        title: entry.title.clone(),
        status: entry.status.clone(),
        // JSON output carries `status`, not the mark.
        mark: StatusMark::Waiting,
        blocked_by: entry.blocked_by.clone(),
        children: Vec::new(),
    })
}

/// The kind name `kind`, as one of [`RECORD_KINDS`](arcl_store::RECORD_KINDS).
fn record_kind(kind: &str) -> anyhow::Result<&'static str> {
    arcl_store::RECORD_KINDS
        .into_iter()
        .find(|known| *known == kind)
        .with_context(|| format!("the query answered an unknown record kind `{kind}`"))
}
