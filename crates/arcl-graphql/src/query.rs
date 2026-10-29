//! The root query fields. Each `id` argument takes a full or short ID.

use async_graphql::{Context, ID, Object};

use super::types::{
    CaptureNode, ContextNode, GateNode, NoteNode, PhaseNode, PlanNode, RecordNode, ReleaseNode, SourceNode, SpecNode,
    TaskNode, TreeNode, WorkNode, reader,
};
use super::{Reader, coded};
use arcl_core::prelude::*;
use arcl_store::prelude::*;

/// The root of every query.
pub struct QueryRoot;

#[Object(name = "Query")]
impl QueryRoot {
    /// The record `id` names, as `arcl show` reads it.
    async fn record(&self, ctx: &Context<'_>, id: ID) -> async_graphql::Result<Option<RecordNode>> {
        let reader = reader(ctx);
        let id = reader.resolve(&id, None)?;
        RecordNode::find(reader.graph()?, &id)
            .map(Some)
            .ok_or_else(|| not_found(&id))
    }

    /// The capture `id` names.
    async fn capture(&self, ctx: &Context<'_>, id: ID) -> async_graphql::Result<Option<CaptureNode>> {
        let id = reader(ctx).parse::<CaptureId>(&id)?;
        match typed(ctx, &id.to_string())? {
            RecordNode::Capture(node) => Ok(Some(node)),
            _ => Err(not_found(&id.to_string())),
        }
    }

    /// The release `id` names.
    async fn release(&self, ctx: &Context<'_>, id: ID) -> async_graphql::Result<Option<ReleaseNode>> {
        let id = reader(ctx).parse::<ReleaseId>(&id)?;
        match typed(ctx, &id.to_string())? {
            RecordNode::Release(node) => Ok(Some(node)),
            _ => Err(not_found(&id.to_string())),
        }
    }

    /// The spec `id` names.
    async fn spec(&self, ctx: &Context<'_>, id: ID) -> async_graphql::Result<Option<SpecNode>> {
        let id = reader(ctx).parse::<SpecId>(&id)?;
        match typed(ctx, &id.to_string())? {
            RecordNode::Spec(node) => Ok(Some(node)),
            _ => Err(not_found(&id.to_string())),
        }
    }

    /// The plan `id` names.
    async fn plan(&self, ctx: &Context<'_>, id: ID) -> async_graphql::Result<Option<PlanNode>> {
        let id = reader(ctx).parse::<PlanId>(&id)?;
        match typed(ctx, &id.to_string())? {
            RecordNode::Plan(node) => Ok(Some(node)),
            _ => Err(not_found(&id.to_string())),
        }
    }

    /// The phase `id` names.
    async fn phase(&self, ctx: &Context<'_>, id: ID) -> async_graphql::Result<Option<PhaseNode>> {
        let id = reader(ctx).parse::<PhaseId>(&id)?;
        match typed(ctx, &id.to_string())? {
            RecordNode::Phase(node) => Ok(Some(node)),
            _ => Err(not_found(&id.to_string())),
        }
    }

    /// The task `id` names, as `arcl explain` reads it.
    async fn task(&self, ctx: &Context<'_>, id: ID) -> async_graphql::Result<Option<TaskNode>> {
        let id = reader(ctx).parse::<TaskId>(&id)?;
        match typed(ctx, &id.to_string())? {
            RecordNode::Task(node) => Ok(Some(node)),
            _ => Err(not_found(&id.to_string())),
        }
    }

    /// The note `id` names.
    async fn note(&self, ctx: &Context<'_>, id: ID) -> async_graphql::Result<Option<NoteNode>> {
        let id = reader(ctx).parse::<NoteId>(&id)?;
        match typed(ctx, &id.to_string())? {
            RecordNode::Note(node) => Ok(Some(node)),
            _ => Err(not_found(&id.to_string())),
        }
    }

    /// The gate `id` names.
    async fn gate(&self, ctx: &Context<'_>, id: ID) -> async_graphql::Result<Option<GateNode>> {
        let reader = reader(ctx);
        let id = reader.parse::<GateId>(&id)?;
        Ok(Some(GateNode(reader.read(|database| database.gate(id))?)))
    }

    /// The records the filters match, as `arcl list` lists them: by kind,
    /// then ID. Every set filter must match.
    #[allow(clippy::too_many_arguments)]
    async fn records(
        &self, ctx: &Context<'_>,
        #[graphql(desc = "`capture`, `release`, `spec`, `plan`, `phase`, `task` or `note`.")] kind: Option<String>,
        #[graphql(default, desc = "Stored statuses, such as `pending`.")] status: Vec<String>,
        #[graphql(default, desc = "Task priorities, such as `high`.")] priority: Vec<String>,
        #[graphql(desc = "Records in this release.")] release: Option<ID>,
        #[graphql(desc = "Plans and tasks under this spec.")] spec: Option<ID>,
        #[graphql(desc = "Phases and tasks under this plan.")] plan: Option<ID>,
        #[graphql(desc = "Tasks under this phase.")] phase: Option<ID>,
        #[graphql(desc = "Direct subtasks of this task.")] parent: Option<ID>,
        #[graphql(desc = "Tasks claimed by this name.")] assignee: Option<String>,
    ) -> async_graphql::Result<Vec<RecordNode>> {
        let reader = reader(ctx);
        let filter = RecordFilter {
            kind,
            statuses: status,
            priorities: priorities(&priority)?,
            release_id: optional(reader, release)?,
            spec_id: optional(reader, spec)?,
            plan_id: optional(reader, plan)?,
            phase_id: optional(reader, phase)?,
            parent_id: optional(reader, parent)?,
            assignee: assignee.map(|name| name.trim().to_owned()),
        };
        RecordNode::list(reader, &filter)
    }

    /// The tasks ready to start, as `arcl ready` lists them: by priority,
    /// then position, then ID. Every set filter must match.
    async fn ready(
        &self, ctx: &Context<'_>, #[graphql(default, desc = "Task priorities, such as `high`.")] priority: Vec<String>,
        #[graphql(desc = "Tasks under this spec.")] spec: Option<ID>,
        #[graphql(desc = "Tasks under this plan.")] plan: Option<ID>,
        #[graphql(desc = "Tasks under this phase.")] phase: Option<ID>,
        #[graphql(desc = "Direct subtasks of this task.")] parent: Option<ID>,
    ) -> async_graphql::Result<Vec<TaskNode>> {
        let reader = reader(ctx);
        let filter = PlanningReadyFilter {
            priorities: priorities(&priority)?,
            spec_id: optional(reader, spec)?,
            plan_id: optional(reader, plan)?,
            phase_id: optional(reader, phase)?,
            parent_id: optional(reader, parent)?,
        };
        let tasks = reader.read(|database| database.ready_planning_tasks_filtered(&filter))?;
        Ok(tasks.into_iter().map(TaskNode::new).collect())
    }

    /// The record tree, as `arcl tree` builds it: from the record `root`
    /// names, or of every record.
    async fn tree(
        &self, ctx: &Context<'_>, root: Option<ID>,
        #[graphql(default, desc = "Include promoted and discarded captures.")] all: bool,
    ) -> async_graphql::Result<Vec<TreeNode>> {
        let reader = reader(ctx);
        let root = root.map(|root| reader.resolve(&root, None)).transpose()?;
        let nodes = reader.read(|database| database.record_tree(root.as_deref(), all))?;
        Ok(nodes.into_iter().map(TreeNode).collect())
    }

    /// Every task with its readiness and effective phase, and every phase,
    /// as the web app's Work list reads them.
    async fn work(&self, ctx: &Context<'_>) -> async_graphql::Result<WorkNode> {
        Ok(WorkNode(reader(ctx).read(Database::planning_work)?))
    }

    /// The context packet of the task `task` names, as `arcl context` prints it.
    async fn context(&self, ctx: &Context<'_>, task: ID) -> async_graphql::Result<Option<ContextNode>> {
        let reader = reader(ctx);
        let id = reader.parse::<TaskId>(&task)?;
        Ok(Some(ContextNode(
            reader.read(|database| database.planning_context(id))?,
        )))
    }

    /// The workspace file of the record `id` names, with its revision.
    async fn source(&self, ctx: &Context<'_>, id: ID) -> async_graphql::Result<Option<SourceNode>> {
        let reader = reader(ctx);
        let id = reader.resolve(&id, None)?;
        reader
            .source(&id)?
            .map(|source| Some(SourceNode::from(source)))
            .ok_or_else(|| not_found(&id))
    }
}

/// The record with the full ID `id`, or a not-found error.
fn typed(ctx: &Context<'_>, id: &str) -> async_graphql::Result<RecordNode> {
    RecordNode::find(reader(ctx).graph()?, id).ok_or_else(|| not_found(id))
}

fn not_found(id: &str) -> async_graphql::Error {
    coded(format!("record `{id}` was not found"), "NOT_FOUND")
}

fn priorities(values: &[String]) -> async_graphql::Result<Vec<TaskPriority>> {
    values
        .iter()
        .map(|value| TaskPriority::parse(value).map_err(|error| coded(error.to_string(), "INVALID_REQUEST")))
        .collect()
}

fn optional<T: PrefixedId>(reader: &Reader, id: Option<ID>) -> async_graphql::Result<Option<T>> {
    id.map(|id| reader.parse::<T>(&id)).transpose()
}
