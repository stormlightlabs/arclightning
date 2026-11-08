//! The root query fields. Each `id` argument takes a full or short ID.

use async_graphql::{Context, ID, Object};

use super::types::{
    CaptureNode, ContextNode, GateNode, NoteNode, PhaseNode, PlanNode, RecordNode, RecordOrGate, ReleaseNode,
    SourceNode, SpecNode, TaskNode, TreeNode, WorkNode, many, one, reader,
};
use super::{Reader, coded, storage_error};
use arcl_core::prelude::*;
use arcl_repo::workspace::RecordKind;
use arcl_store::prelude::*;

/// The root of every query.
pub struct QueryRoot;

#[Object(name = "Query")]
impl QueryRoot {
    /// The record or gate `id` names, as `arcl show` reads it. Select fields
    /// with `... on Record` and `... on Gate`.
    async fn record(&self, ctx: &Context<'_>, id: ID) -> async_graphql::Result<Option<RecordOrGate>> {
        let reader = reader(ctx);
        let id = reader.resolve(&id, None)?;
        if id.starts_with(GateId::PREFIX) {
            let gate = reader.parse::<GateId>(&id)?;
            let gate = RecordOrGate::Gate(GateNode(reader.read(|database| database.gate(gate))?));
            return one(ctx, Some(gate));
        }
        let id = checked(reader, &id)?;
        let record = RecordNode::find(reader.graph()?, &id).ok_or_else(|| not_found(&id))?;
        one(ctx, Some(record.into()))
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
        one(ctx, Some(GateNode(reader.read(|database| database.gate(id))?)))
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
        #[graphql(default, desc = "Include archived tasks.")] all: bool,
    ) -> async_graphql::Result<Vec<RecordNode>> {
        let reader = reader(ctx);
        // Check the filters in the order `arcl list` does: the kind, then
        // priorities, then what the kind allows, then the record IDs.
        RecordFilter { kind: kind.clone(), ..RecordFilter::default() }
            .validate()
            .map_err(storage_error)?;
        let mut filter = RecordFilter {
            kind,
            statuses: status,
            priorities: priorities(&priority)?,
            assignee: assignee.map(|name| name.trim().to_owned()),
            archived: all,
            ..RecordFilter::default()
        };
        filter.validate().map_err(storage_error)?;
        filter.release_id = optional(reader, release)?;
        filter.spec_id = optional(reader, spec)?;
        filter.plan_id = optional(reader, plan)?;
        filter.phase_id = optional(reader, phase)?;
        filter.parent_id = optional(reader, parent)?;
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
        many(ctx, tasks.into_iter().map(TaskNode::new).collect())
    }

    /// The record tree, as `arcl tree` builds it: from the record `root`
    /// names, or of every record.
    async fn tree(
        &self, ctx: &Context<'_>, root: Option<ID>,
        #[graphql(default, desc = "Include promoted and discarded captures and archived tasks.")] all: bool,
    ) -> async_graphql::Result<Vec<TreeNode>> {
        let reader = reader(ctx);
        let root = root.map(|root| reader.resolve(&root, None)).transpose()?;
        let nodes = reader.read(|database| database.record_tree(root.as_deref(), all))?;
        many(ctx, nodes.into_iter().map(TreeNode::top).collect())
    }

    /// Every task with its readiness and effective phase, and every phase,
    /// as the web app's Work list reads them. Archived tasks appear only
    /// with `all`.
    async fn work(
        &self, ctx: &Context<'_>, #[graphql(default, desc = "Include archived tasks.")] all: bool,
    ) -> async_graphql::Result<WorkNode> {
        let reader = reader(ctx);
        reader.admit(1)?;
        let mut work = reader.read(Database::planning_work)?;
        if !all {
            work.tasks.retain(|item| item.task.archived_at.is_none());
        }
        Ok(WorkNode(work))
    }

    /// The context packet of the task `task` names, as `arcl context` prints it.
    async fn context(
        &self, ctx: &Context<'_>, task: ID,
        #[graphql(default, desc = "Include archived blockers, dependents and evidence.")] all: bool,
    ) -> async_graphql::Result<Option<ContextNode>> {
        let reader = reader(ctx);
        let id = reader.parse::<TaskId>(&task)?;
        let view = (*reader.task_view(id)?).clone();
        let mut context = PlanningContext::from_view(view);
        if !all {
            context.hide_archived();
        }
        one(ctx, Some(ContextNode(context)))
    }

    /// The workspace file of the record `id` names, with its revision.
    async fn source(&self, ctx: &Context<'_>, id: ID) -> async_graphql::Result<Option<SourceNode>> {
        let reader = reader(ctx);
        let id = reader.resolve(&id, None)?;
        let source = reader.source(&id)?.ok_or_else(|| source_not_found(&id))?;
        one(ctx, Some(SourceNode::from(source)))
    }
}

/// The record with the full ID `id`, or a not-found error.
fn typed(ctx: &Context<'_>, id: &str) -> async_graphql::Result<RecordNode> {
    let reader = reader(ctx);
    let record = RecordNode::find(reader.graph()?, id).ok_or_else(|| not_found(id))?;
    reader.admit(1)?;
    Ok(record)
}

/// The full ID `id`, once it parses as an ID of the kind its prefix names,
/// as `arcl show` checks it.
fn checked(reader: &Reader, id: &str) -> async_graphql::Result<String> {
    let kind = RecordKind::from_id(id).ok_or_else(|| coded(format!("unknown record ID `{id}`"), "INVALID_REQUEST"))?;
    let parsed = match kind {
        RecordKind::Capture => reader.parse::<CaptureId>(id)?.to_string(),
        RecordKind::Release => reader.parse::<ReleaseId>(id)?.to_string(),
        RecordKind::Spec => reader.parse::<SpecId>(id)?.to_string(),
        RecordKind::Plan => reader.parse::<PlanId>(id)?.to_string(),
        RecordKind::Phase => reader.parse::<PhaseId>(id)?.to_string(),
        RecordKind::Task => reader.parse::<TaskId>(id)?.to_string(),
        RecordKind::Note => reader.parse::<NoteId>(id)?.to_string(),
    };
    Ok(parsed)
}

/// A missing record, worded as `arcl show` words it for the record's kind.
fn not_found(id: &str) -> async_graphql::Error {
    let id = id.to_owned();
    let error = match RecordKind::from_id(&id) {
        Some(RecordKind::Capture) => StorageError::CaptureNotFound { id },
        Some(RecordKind::Release) => StorageError::ReleaseNotFound { id },
        Some(RecordKind::Spec) => StorageError::SpecNotFound { id },
        Some(RecordKind::Plan) => StorageError::PlanNotFound { id },
        Some(RecordKind::Phase) => StorageError::PhaseNotFound { id },
        Some(RecordKind::Task) => StorageError::PlanningTaskNotFound { id },
        Some(RecordKind::Note) => StorageError::NoteNotFound { id },
        None => return source_not_found(&id),
    };
    storage_error(error)
}

/// A record with no workspace file, worded as a raw edit words it.
fn source_not_found(id: &str) -> async_graphql::Error {
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
