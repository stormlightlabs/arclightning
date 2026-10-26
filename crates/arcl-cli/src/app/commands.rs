use super::*;
use super::{query::task_ancestry, support::*};

pub(super) enum CaptureResult {
    Mutation { action: &'static str, capture: Capture },
    Promotion(Box<arcl_store::CapturePromotionResult>),
    List(Vec<Capture>),
}

pub(super) enum PlanResult {
    Mutation { action: &'static str, plan: Plan },
    Detail(PlanDetail),
    Diff(arcl_store::PlanDiff),
    Applied(arcl_store::PlanApplyResult),
    List(Vec<Plan>),
}

pub(super) fn exec_capture(command: CaptureCommand) -> CResult<CaptureResult> {
    match command {
        CaptureCommand::Create { title, body } => {
            validate_title(&title)?;
            let body = resolve_markdown(body)?.unwrap_or_default();
            let mut database = open_database()?;
            Ok(CaptureResult::Mutation { action: "created", capture: database.create_capture(title, body)? })
        }
        CaptureCommand::Show { id } => {
            let id = parse_capture_id(&id)?;
            let database = open_database()?;
            let capture = database
                .capture(id)?
                .ok_or_else(|| StorageError::CaptureNotFound { id: id.to_string() })?;
            Ok(CaptureResult::Mutation { action: "shown", capture })
        }
        CaptureCommand::List => Ok(CaptureResult::List(open_database()?.captures()?)),
        CaptureCommand::Update { id, title, body } => {
            let id = parse_capture_id(&id)?;
            if let Some(title) = &title {
                validate_title(title)?;
            }
            let body = resolve_markdown(body)?;
            let mut database = open_database()?;
            Ok(CaptureResult::Mutation {
                action: "updated",
                capture: database.update_capture(id, CaptureUpdate { title, body })?,
            })
        }
        CaptureCommand::Discard { id } => {
            let id = parse_capture_id(&id)?;
            let mut database = open_database()?;
            Ok(CaptureResult::Mutation { action: "discarded", capture: database.discard_capture(id)? })
        }
        CaptureCommand::Promote {
            id,
            target,
            to,
            title,
            body,
            acceptance_criteria,
            acceptance_criteria_file,
            spec,
            plan,
            phase,
            parent,
            priority,
            position,
        } => {
            let id = parse_capture_id(&id)?;
            let target = target.or(to).ok_or_else(|| CommandError::InvalidFilter {
                message: "capture promotion requires a target: spec, task, or note".to_owned(),
            })?;
            let mut database = open_database()?;
            let capture = database
                .capture(id)?
                .ok_or_else(|| StorageError::CaptureNotFound { id: id.to_string() })?;
            let title = title.unwrap_or_else(|| capture.title.clone());
            let body = resolve_markdown(body)?.unwrap_or_else(|| capture.body.clone());
            let acceptance_criteria =
                resolve_optional_value(acceptance_criteria, acceptance_criteria_file)?.unwrap_or_default();
            let result = match CapturePromotionKind::parse(&target) {
                Some(CapturePromotionKind::Spec) => {
                    reject_promotion_task_fields(spec, plan, phase, parent)?;
                    database.promote_capture_to_spec(id, title, body, acceptance_criteria)?
                }
                Some(CapturePromotionKind::Task) => {
                    let priority = TaskPriority::parse(&priority)?;
                    let input = CaptureTaskPromotion {
                        spec_id: parse_optional_id::<SpecId>(spec)?,
                        plan_id: parse_optional_id::<PlanId>(plan)?,
                        phase_id: parse_optional_id::<PhaseId>(phase)?,
                        parent_id: parse_optional_id::<TaskId>(parent)?,
                        title,
                        body,
                        priority,
                        position,
                    };
                    database.promote_capture_to_task(id, input)?
                }
                Some(CapturePromotionKind::Note) => {
                    reject_promotion_task_fields(spec, plan, phase, parent)?;
                    database.promote_capture_to_note(id, title, body)?
                }
                None => {
                    return Err(CommandError::InvalidFilter {
                        message: format!(
                            "unknown capture promotion target `{}`; use spec, task, or note",
                            target.to_ascii_lowercase()
                        ),
                    });
                }
            };
            Ok(CaptureResult::Promotion(Box::new(result)))
        }
    }
}

/// Run one `arcl capture` command and return what it prints, so `arcl serve`
/// can answer with the same output and errors.
pub(super) fn run_capture(command: CaptureCommand, renderer: &Renderer) -> Result<Option<String>, ApplicationError> {
    let result = exec_capture(command).map_err(ApplicationError::from_command)?;
    render_output(match result {
        CaptureResult::Mutation { action, capture } => renderer.render_capture(action, &capture),
        CaptureResult::Promotion(result) => renderer.render_capture_promotion(&result),
        CaptureResult::List(captures) => renderer.render_captures(&captures),
    })
}

pub(super) fn exec_release(command: ReleaseCommand, renderer: &Renderer) -> anyhow::Result<()> {
    let result: Result<Option<String>, ApplicationError> = (|| match command {
        ReleaseCommand::Create { title, body } => {
            validate_title(&title).map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            let body = resolve_markdown(body)
                .map_err(ApplicationError::from_command)?
                .unwrap_or_default();
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            let release = database
                .create_release(title, body)
                .map_err(ApplicationError::from_command)?;
            Ok(renderer
                .render_connected_release("created", &release)
                .map_err(|error| {
                    ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
                })?)
        }
        ReleaseCommand::Show { id } => {
            let id = parse_release_id(&id).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            let release = database
                .release(id)
                .map_err(ApplicationError::from_command)?
                .ok_or_else(|| {
                    ApplicationError::from_command(CommandError::Storage(StorageError::ReleaseNotFound {
                        id: id.to_string(),
                    }))
                })?;
            Ok(renderer.render_connected_release("shown", &release).map_err(|error| {
                ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
            })?)
        }
        ReleaseCommand::List => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            Ok(renderer
                .render_releases(&database.releases().map_err(ApplicationError::from_command)?)
                .map_err(|error| {
                    ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
                })?)
        }
        ReleaseCommand::Update { id, title, body } => {
            let id = parse_release_id(&id).map_err(ApplicationError::from_command)?;
            if let Some(title) = &title {
                validate_title(title).map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            }
            let body = resolve_markdown(body).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            let release = database
                .update_release(id, title, body)
                .map_err(ApplicationError::from_command)?;
            Ok(renderer
                .render_connected_release("updated", &release)
                .map_err(|error| {
                    ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
                })?)
        }
        ReleaseCommand::Complete { id, allow_open_children } => {
            transition_release(id, ContainerAction::Complete, allow_open_children, renderer)
        }
        ReleaseCommand::Cancel { id, allow_open_children } => {
            transition_release(id, ContainerAction::Cancel, allow_open_children, renderer)
        }
        ReleaseCommand::Member { command } => exec_membership(command, renderer),
    })();
    result.map(write_output).unwrap_or_else(|error| Err(error.into()))
}

pub(super) fn exec_spec(command: SpecCommand, renderer: &Renderer) -> anyhow::Result<()> {
    write_output(run_spec(command, renderer)?)
}

/// Run one `arcl spec` command and return what it prints, so `arcl serve`
/// can answer with the same output and errors.
pub(super) fn run_spec(command: SpecCommand, renderer: &Renderer) -> Result<Option<String>, ApplicationError> {
    let message = match command {
        SpecCommand::Create { title, body, acceptance } => {
            validate_title(&title).map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            let body = resolve_markdown(body)
                .map_err(ApplicationError::from_command)?
                .unwrap_or_default();
            let acceptance = resolve_acceptance(acceptance)
                .map_err(ApplicationError::from_command)?
                .unwrap_or_default();
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            let spec = database
                .create_spec(title, body, acceptance)
                .map_err(ApplicationError::from_command)?;
            renderer.render_spec("created", &spec).map_err(|error| {
                ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
            })?
        }
        SpecCommand::Show { id } => {
            let id = parse_spec_id(&id).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            let spec = database
                .spec(id)
                .map_err(ApplicationError::from_command)?
                .ok_or_else(|| {
                    ApplicationError::from_command(CommandError::Storage(StorageError::SpecNotFound {
                        id: id.to_string(),
                    }))
                })?;
            renderer.render_spec("shown", &spec).map_err(|error| {
                ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
            })?
        }
        SpecCommand::List => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            renderer
                .render_specs(&database.specs().map_err(ApplicationError::from_command)?)
                .map_err(|error| {
                    ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
                })?
        }
        SpecCommand::Update { id, title, body, acceptance } => {
            let id = parse_spec_id(&id).map_err(ApplicationError::from_command)?;
            if let Some(title) = &title {
                validate_title(title).map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            }
            let body = resolve_markdown(body).map_err(ApplicationError::from_command)?;
            let acceptance = resolve_acceptance(acceptance).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            let spec = database
                .update_spec(id, SpecUpdate { title, body, acceptance_criteria: acceptance })
                .map_err(ApplicationError::from_command)?;
            renderer.render_spec("updated", &spec).map_err(|error| {
                ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
            })?
        }
        SpecCommand::Complete { id, allow_open_children } => {
            transition_spec(id, ContainerAction::Complete, allow_open_children, renderer)?
        }
        SpecCommand::Cancel { id, allow_open_children } => {
            transition_spec(id, ContainerAction::Cancel, allow_open_children, renderer)?
        }
    };
    Ok(message)
}

pub(super) fn exec_plan(command: PlanCommand, renderer: &Renderer) -> anyhow::Result<()> {
    write_output(run_plan(command, renderer)?)
}

/// Run one `arcl plan` command and return what it prints, so `arcl serve`
/// can answer with the same output and errors.
pub(super) fn run_plan(command: PlanCommand, renderer: &Renderer) -> Result<Option<String>, ApplicationError> {
    let result = match command {
        PlanCommand::Create { title, spec, body, input, no_input: _ } => {
            validate_title(&title).map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            let spec_id = parse_spec_id(&spec).map_err(ApplicationError::from_command)?;
            let body = resolve_markdown(body)
                .map_err(ApplicationError::from_command)?
                .unwrap_or_default();
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            match input {
                Some(path) => {
                    let document = read_plan_document(&path).map_err(ApplicationError::from_command)?;
                    PlanResult::Applied(
                        database
                            .create_and_apply_plan(spec_id, title, body, &document)
                            .map_err(ApplicationError::from_command)?,
                    )
                }
                None => PlanResult::Mutation {
                    action: "created",
                    plan: database
                        .create_plan(spec_id, title, body)
                        .map_err(ApplicationError::from_command)?,
                },
            }
        }
        PlanCommand::Show { id } => {
            let id = parse_plan_id(&id).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            let plan = database
                .plan(id)
                .map_err(ApplicationError::from_command)?
                .ok_or_else(|| {
                    ApplicationError::from_command(CommandError::Storage(StorageError::PlanNotFound {
                        id: id.to_string(),
                    }))
                })?;
            let graph = database.connected_graph().map_err(ApplicationError::from_command)?;
            let tasks = graph
                .tasks
                .iter()
                .map(|task| {
                    task_ancestry(&graph, task.id).map(|(_, plan_id, _)| (plan_id == Some(id)).then(|| task.clone()))
                })
                .collect::<CResult<Vec<_>>>()
                .map_err(ApplicationError::from_command)?
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
            let task_ids = tasks
                .iter()
                .map(|task| task.id)
                .collect::<std::collections::HashSet<_>>();
            PlanResult::Detail(PlanDetail {
                plan,
                phases: graph.phases.into_iter().filter(|phase| phase.plan_id == id).collect(),
                tasks,
                dependencies: graph
                    .dependencies
                    .into_iter()
                    .filter(|dependency| task_ids.contains(&dependency.task_id))
                    .collect(),
            })
        }
        PlanCommand::List => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            PlanResult::List(database.plans().map_err(ApplicationError::from_command)?)
        }
        PlanCommand::Update { id, title, body } => {
            let id = parse_plan_id(&id).map_err(ApplicationError::from_command)?;
            if let Some(title) = &title {
                validate_title(title).map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            }
            let body = resolve_markdown(body).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            PlanResult::Mutation {
                action: "updated",
                plan: database
                    .update_plan(id, PlanUpdate { title, body })
                    .map_err(ApplicationError::from_command)?,
            }
        }
        PlanCommand::Check { id, file } => {
            let id = parse_plan_id(&id).map_err(ApplicationError::from_command)?;
            let document = read_plan_document(&file).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            PlanResult::Diff(
                database
                    .check_plan(id, &document)
                    .map_err(ApplicationError::from_command)?,
            )
        }
        PlanCommand::Diff { id, file } => {
            let id = parse_plan_id(&id).map_err(ApplicationError::from_command)?;
            let document = read_plan_document(&file).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            PlanResult::Diff(
                database
                    .diff_plan(id, &document)
                    .map_err(ApplicationError::from_command)?,
            )
        }
        PlanCommand::Apply { id, file } => {
            let id = parse_plan_id(&id).map_err(ApplicationError::from_command)?;
            let document = read_plan_document(&file).map_err(ApplicationError::from_command)?;
            return apply_plan_document(id, &document, renderer);
        }
        PlanCommand::Complete { id, allow_open_children } => {
            let id = parse_plan_id(&id).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            PlanResult::Mutation {
                action: "completed",
                plan: database
                    .transition_plan(id, ContainerAction::Complete, allow_open_children)
                    .map_err(ApplicationError::from_command)?,
            }
        }
        PlanCommand::Cancel { id, allow_open_children } => {
            let id = parse_plan_id(&id).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            PlanResult::Mutation {
                action: "cancelled",
                plan: database
                    .transition_plan(id, ContainerAction::Cancel, allow_open_children)
                    .map_err(ApplicationError::from_command)?,
            }
        }
        PlanCommand::Phase { command } => return run_phase(command, renderer),
    };
    let message = match result {
        PlanResult::Mutation { action, plan } => renderer.render_plan(action, &plan),
        PlanResult::Detail(detail) => renderer.render_plan_detail(&detail),
        PlanResult::Diff(diff) => renderer.render_plan_diff(&diff),
        PlanResult::Applied(result) => renderer.render_plan_apply(&result),
        PlanResult::List(plans) => renderer.render_plans(&plans),
    };
    render_output(message)
}

/// `arcl plan apply`: apply `document` to plan `id` in one transaction. The
/// CLI reads the document from `--file`; `arcl serve` from the request.
pub(super) fn apply_plan_document(
    id: PlanId, document: &arcl_core::plan::PlanDocument, renderer: &Renderer,
) -> Result<Option<String>, ApplicationError> {
    let mut database = open_database().map_err(ApplicationError::from_command)?;
    let result = database
        .apply_plan(id, document)
        .map_err(ApplicationError::from_command)?;
    render_output(renderer.render_plan_apply(&result))
}

pub(super) fn exec_task(command: TaskCommand, renderer: &Renderer) -> anyhow::Result<()> {
    write_output(run_task(command, renderer)?)
}

/// Run one `arcl task` command and return what it prints, so `arcl serve`
/// can answer with the same output and errors.
pub(super) fn run_task(command: TaskCommand, renderer: &Renderer) -> Result<Option<String>, ApplicationError> {
    match command {
        TaskCommand::Create { title, spec, plan, phase, parent, priority, position, blocked_by, body } => {
            validate_title(&title).map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            let priority = TaskPriority::parse(&priority)
                .map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            let project_id = database.project().map_err(ApplicationError::from_command)?.id;
            let input = PlanningTaskCreate {
                project_id,
                spec_id: parse_optional_id::<SpecId>(spec).map_err(ApplicationError::from_command)?,
                plan_id: parse_optional_id::<PlanId>(plan).map_err(ApplicationError::from_command)?,
                phase_id: parse_optional_id::<PhaseId>(phase).map_err(ApplicationError::from_command)?,
                parent_id: parse_optional_id::<TaskId>(parent).map_err(ApplicationError::from_command)?,
                title,
                body: resolve_markdown(body)
                    .map_err(ApplicationError::from_command)?
                    .unwrap_or_default(),
                priority,
                position,
            };
            let blockers = blocked_by
                .iter()
                .map(|blocker| parse_task_id(blocker))
                .collect::<CResult<Vec<_>>>()
                .map_err(ApplicationError::from_command)?;
            let task = database
                .create_planning_task_with_dependencies(input, &blockers)
                .map_err(ApplicationError::from_command)?;
            render_output(renderer.render_planning_task("created", &task))
        }
        TaskCommand::Show { id } => {
            let id = parse_task_id(&id).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            let ids = display_ids(&database, renderer).map_err(ApplicationError::from_command)?;
            render_output(
                renderer.render_planning_task_view(
                    &database
                        .planning_task_view(id)
                        .map_err(ApplicationError::from_command)?,
                    &ids,
                ),
            )
        }
        TaskCommand::List => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            render_output(
                renderer.render_planning_tasks(&database.planning_tasks().map_err(ApplicationError::from_command)?),
            )
        }
        TaskCommand::Update {
            id,
            title,
            body,
            priority,
            position,
            spec,
            no_spec,
            plan,
            no_plan,
            phase,
            no_phase,
            parent,
            no_parent,
        } => {
            let id = parse_task_id(&id).map_err(ApplicationError::from_command)?;
            if let Some(title) = &title {
                validate_title(title).map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            }
            let priority = priority
                .as_deref()
                .map(TaskPriority::parse)
                .transpose()
                .map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            let update = PlanningTaskUpdate {
                title,
                body: resolve_markdown(body).map_err(ApplicationError::from_command)?,
                priority,
                position,
                spec_id: relation_change::<SpecId>(spec, no_spec).map_err(ApplicationError::from_command)?,
                plan_id: relation_change::<PlanId>(plan, no_plan).map_err(ApplicationError::from_command)?,
                phase_id: relation_change::<PhaseId>(phase, no_phase).map_err(ApplicationError::from_command)?,
                parent_id: relation_change::<TaskId>(parent, no_parent).map_err(ApplicationError::from_command)?,
            };
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            render_output(
                renderer.render_planning_task(
                    "updated",
                    &database
                        .update_planning_task(id, update)
                        .map_err(ApplicationError::from_command)?,
                ),
            )
        }
        TaskCommand::Start { id, force } => {
            let id = parse_task_id(&id).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            render_output(
                renderer.render_planning_task(
                    "started",
                    &database
                        .start_planning_task(id, force)
                        .map_err(ApplicationError::from_command)?,
                ),
            )
        }
        TaskCommand::Park { id } => transition_task(id, TaskAction::Park, false, renderer),
        TaskCommand::Unpark { id } => transition_task(id, TaskAction::Unpark, false, renderer),
        TaskCommand::Handoff { id, note, note_file } => {
            let note = resolve_optional_value(note, note_file)
                .map_err(ApplicationError::from_command)?
                .ok_or_else(|| {
                    ApplicationError::from_command(CommandError::Domain(DomainError::NoFieldsToUpdate {
                        entity: "handoff",
                    }))
                })?;
            let id = parse_task_id(&id).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            render_output(
                renderer.render_planning_task(
                    "handed_off",
                    &database
                        .handoff_planning_task(id, note)
                        .map_err(ApplicationError::from_command)?,
                ),
            )
        }
        TaskCommand::Complete { id, allow_open_children, with_subtasks, evidence, evidence_file } => {
            let evidence = resolve_optional_value(evidence, evidence_file).map_err(ApplicationError::from_command)?;
            let id = parse_task_id(&id).map_err(ApplicationError::from_command)?;
            let open_subtasks = match (with_subtasks, allow_open_children) {
                (true, _) => arcl_store::OpenSubtasks::Complete,
                (false, true) => arcl_store::OpenSubtasks::Leave,
                (false, false) => arcl_store::OpenSubtasks::Refuse,
            };
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            render_output(
                renderer.render_planning_completion(
                    &database
                        .complete_planning_task(id, open_subtasks, evidence)
                        .map_err(ApplicationError::from_command)?,
                ),
            )
        }
        TaskCommand::Cancel { id, allow_open_children } => {
            transition_task(id, TaskAction::Cancel, allow_open_children, renderer)
        }
        TaskCommand::Explain { id } => {
            let id = parse_task_id(&id).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            let ids = display_ids(&database, renderer).map_err(ApplicationError::from_command)?;
            render_output(
                renderer.render_planning_task_view(
                    &database
                        .planning_task_view(id)
                        .map_err(ApplicationError::from_command)?,
                    &ids,
                ),
            )
        }
        TaskCommand::Context { id } => {
            let id = parse_task_id(&id).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            render_output(
                renderer
                    .render_planning_context(&database.planning_context(id).map_err(ApplicationError::from_command)?),
            )
        }
    }
}

pub(super) fn exec_dependency(command: DependencyCommand, renderer: &Renderer) -> anyhow::Result<()> {
    let message = match command {
        DependencyCommand::Add { task_id, blocker_id } => {
            let task_id = parse_task_id(&task_id).map_err(ApplicationError::from_command)?;
            let blocker_id = parse_task_id(&blocker_id).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            let dependency = database
                .add_planning_dependency(task_id, blocker_id)
                .map_err(ApplicationError::from_command)?;
            renderer
                .render_dependency(crate::output::DependencyMutation::Added, &dependency)
                .map_err(|error| {
                    ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
                })?
        }
        DependencyCommand::Remove { task_id, blocker_id } => {
            let task_id = parse_task_id(&task_id).map_err(ApplicationError::from_command)?;
            let blocker_id = parse_task_id(&blocker_id).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            let dependency = database
                .remove_planning_dependency(task_id, blocker_id)
                .map_err(ApplicationError::from_command)?;
            renderer
                .render_dependency(crate::output::DependencyMutation::Removed, &dependency)
                .map_err(|error| {
                    ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
                })?
        }
        DependencyCommand::List => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            let dependencies = database
                .planning_dependencies()
                .map_err(ApplicationError::from_command)?;
            let lines = dependencies
                .iter()
                .map(|item| format!("{}\t{}", item.task_id, item.blocker_id))
                .collect::<Vec<_>>();
            renderer
                .render_relationships("dependencies", &dependencies, &lines, "No dependencies.")
                .map_err(|error| {
                    ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
                })?
        }
    };
    write_output(message)
}

pub(super) fn exec_note(command: NoteCommand, renderer: &Renderer) -> anyhow::Result<()> {
    let message = match command {
        NoteCommand::Create { title, body } => {
            validate_title(&title).map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            let body = resolve_markdown(body)
                .map_err(ApplicationError::from_command)?
                .unwrap_or_default();
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            renderer.render_note(
                "created",
                &database
                    .create_note(title, body)
                    .map_err(ApplicationError::from_command)?,
            )
        }
        NoteCommand::Show { id } => {
            let id = parse_note_id(&id).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            let note = database
                .note(id)
                .map_err(ApplicationError::from_command)?
                .ok_or_else(|| {
                    ApplicationError::from_command(CommandError::Storage(StorageError::NoteNotFound {
                        id: id.to_string(),
                    }))
                })?;
            renderer.render_note("shown", &note)
        }
        NoteCommand::List => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            renderer.render_notes(&database.notes().map_err(ApplicationError::from_command)?)
        }
        NoteCommand::Update { id, title, body } => {
            let id = parse_note_id(&id).map_err(ApplicationError::from_command)?;
            if let Some(title) = &title {
                validate_title(title).map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            }
            let body = resolve_markdown(body).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            renderer.render_note(
                "updated",
                &database
                    .update_note(id, NoteUpdate { title, body })
                    .map_err(ApplicationError::from_command)?,
            )
        }
        NoteCommand::Link { command } => return exec_note_link(command, renderer),
    }
    .map_err(|error| ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() }))?;
    write_output(message)
}

fn reject_promotion_task_fields(
    spec: Option<String>, plan: Option<String>, phase: Option<String>, parent: Option<String>,
) -> CResult<()> {
    if spec.is_some() || plan.is_some() || phase.is_some() || parent.is_some() {
        return Err(CommandError::InvalidFilter {
            message: "task placement options apply only when promoting to a task".to_owned(),
        });
    }
    Ok(())
}

fn transition_release(
    id: String, action: ContainerAction, allow_open_children: bool, renderer: &Renderer,
) -> Result<Option<String>, ApplicationError> {
    let id = parse_release_id(&id).map_err(ApplicationError::from_command)?;
    let mut database = open_database().map_err(ApplicationError::from_command)?;
    let release = database
        .transition_release(id, action, allow_open_children)
        .map_err(ApplicationError::from_command)?;
    let action = match action {
        ContainerAction::Complete => "completed",
        ContainerAction::Cancel => "cancelled",
    };
    renderer
        .render_connected_release(action, &release)
        .map_err(|error| ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() }))
}

fn exec_membership(command: MembershipCommand, renderer: &Renderer) -> Result<Option<String>, ApplicationError> {
    match command {
        MembershipCommand::Add { release_id, kind, record_id } => {
            let release_id = parse_release_id(&release_id).map_err(ApplicationError::from_command)?;
            let kind = parse_member_kind(&kind).map_err(ApplicationError::from_command)?;
            let record_id =
                resolve_record_id(&record_id, Some(member_prefix(kind))).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            let membership = database
                .add_release_membership(release_id, kind, record_id)
                .map_err(ApplicationError::from_command)?;
            renderer
                .render_membership("added", &membership, &membership.record_id)
                .map_err(|error| {
                    ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
                })
        }
        MembershipCommand::Remove { release_id, kind, record_id } => {
            let release_id = parse_release_id(&release_id).map_err(ApplicationError::from_command)?;
            let kind = parse_member_kind(&kind).map_err(ApplicationError::from_command)?;
            let record_id =
                resolve_record_id(&record_id, Some(member_prefix(kind))).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            let membership = database
                .remove_release_membership(release_id, kind, &record_id)
                .map_err(ApplicationError::from_command)?;
            renderer
                .render_membership("removed", &membership, &membership.record_id)
                .map_err(|error| {
                    ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
                })
        }
        MembershipCommand::List { release_id } => {
            let release_id = parse_release_id(&release_id).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            if database
                .release(release_id)
                .map_err(ApplicationError::from_command)?
                .is_none()
            {
                return Err(ApplicationError::from_command(CommandError::Storage(
                    StorageError::ReleaseNotFound { id: release_id.to_string() },
                )));
            }
            let values = database
                .release_memberships()
                .map_err(ApplicationError::from_command)?
                .into_iter()
                .filter(|item| item.release_id == release_id)
                .collect::<Vec<_>>();
            let lines = values
                .iter()
                .map(|item| format!("{}\t{}\t{}", item.record_kind.as_str(), item.release_id, item.record_id))
                .collect::<Vec<_>>();
            renderer
                .render_relationships("memberships", &values, &lines, "No release members.")
                .map_err(|error| {
                    ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
                })
        }
    }
}

fn transition_spec(
    id: String, action: ContainerAction, allow_open_children: bool, renderer: &Renderer,
) -> Result<Option<String>, ApplicationError> {
    let id = parse_spec_id(&id).map_err(ApplicationError::from_command)?;
    let mut database = open_database().map_err(ApplicationError::from_command)?;
    let spec = database
        .transition_spec(id, action, allow_open_children)
        .map_err(ApplicationError::from_command)?;
    let action = match action {
        ContainerAction::Complete => "completed",
        ContainerAction::Cancel => "cancelled",
    };
    renderer
        .render_spec(action, &spec)
        .map_err(|error| ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() }))
}

fn run_phase(command: PhaseCommand, renderer: &Renderer) -> Result<Option<String>, ApplicationError> {
    match command {
        PhaseCommand::Create { title, plan, position, body } => {
            validate_title(&title).map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            let plan_id = parse_plan_id(&plan).map_err(ApplicationError::from_command)?;
            let body = resolve_markdown(body)
                .map_err(ApplicationError::from_command)?
                .unwrap_or_default();
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            let phase = database
                .create_phase(plan_id, title, body, position)
                .map_err(ApplicationError::from_command)?;
            render_output(renderer.render_phase("created", &phase))
        }
        PhaseCommand::Show { id } => {
            let id = parse_phase_id(&id).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            let phase = database
                .phase(id)
                .map_err(ApplicationError::from_command)?
                .ok_or_else(|| {
                    ApplicationError::from_command(CommandError::Storage(StorageError::PhaseNotFound {
                        id: id.to_string(),
                    }))
                })?;
            render_output(renderer.render_phase("shown", &phase))
        }
        PhaseCommand::List => {
            let database = open_database().map_err(ApplicationError::from_command)?;
            render_output(renderer.render_phases(&database.phases().map_err(ApplicationError::from_command)?))
        }
        PhaseCommand::Update { id, title, position, body } => {
            let id = parse_phase_id(&id).map_err(ApplicationError::from_command)?;
            if let Some(title) = &title {
                validate_title(title).map_err(|error| ApplicationError::from_command(CommandError::Domain(error)))?;
            }
            let body = resolve_markdown(body).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            render_output(
                renderer.render_phase(
                    "updated",
                    &database
                        .update_phase(id, PhaseUpdate { title, body, position })
                        .map_err(ApplicationError::from_command)?,
                ),
            )
        }
        PhaseCommand::Complete { id, allow_open_children } => {
            transition_phase(id, ContainerAction::Complete, allow_open_children, renderer)
        }
        PhaseCommand::Cancel { id, allow_open_children } => {
            transition_phase(id, ContainerAction::Cancel, allow_open_children, renderer)
        }
    }
}

fn transition_phase(
    id: String, action: ContainerAction, allow_open_children: bool, renderer: &Renderer,
) -> Result<Option<String>, ApplicationError> {
    let id = parse_phase_id(&id).map_err(ApplicationError::from_command)?;
    let mut database = open_database().map_err(ApplicationError::from_command)?;
    let phase = database
        .transition_phase(id, action, allow_open_children)
        .map_err(ApplicationError::from_command)?;
    let action = match action {
        ContainerAction::Complete => "completed",
        ContainerAction::Cancel => "cancelled",
    };
    render_output(renderer.render_phase(action, &phase))
}

fn transition_task(
    id: String, action: TaskAction, allow_open_children: bool, renderer: &Renderer,
) -> Result<Option<String>, ApplicationError> {
    let id = parse_task_id(&id).map_err(ApplicationError::from_command)?;
    let mut database = open_database().map_err(ApplicationError::from_command)?;
    let task = database
        .transition_planning_task(id, action, allow_open_children)
        .map_err(ApplicationError::from_command)?;
    let action = match action {
        TaskAction::Start => "started",
        TaskAction::Park => "parked",
        TaskAction::Unpark => "unparked",
        TaskAction::Complete => "completed",
        TaskAction::Cancel => "cancelled",
    };
    render_output(renderer.render_planning_task(action, &task))
}

fn render_output(result: Result<Option<String>, OutputError>) -> Result<Option<String>, ApplicationError> {
    result.map_err(|error| ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() }))
}

fn exec_note_link(command: NoteLinkCommand, renderer: &Renderer) -> anyhow::Result<()> {
    let message = match command {
        NoteLinkCommand::Add { note_id, kind, record_id } => {
            let note_id = parse_note_id(&note_id).map_err(ApplicationError::from_command)?;
            let kind = parse_linked_kind(&kind).map_err(ApplicationError::from_command)?;
            let record_id =
                resolve_record_id(&record_id, Some(linked_prefix(kind))).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            let link = database
                .add_note_link(note_id, kind, record_id)
                .map_err(ApplicationError::from_command)?;
            renderer
                .render_note_link("added", &link, &link.record_id)
                .map_err(|error| {
                    ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
                })?
        }
        NoteLinkCommand::Remove { note_id, kind, record_id } => {
            let note_id = parse_note_id(&note_id).map_err(ApplicationError::from_command)?;
            let kind = parse_linked_kind(&kind).map_err(ApplicationError::from_command)?;
            let record_id =
                resolve_record_id(&record_id, Some(linked_prefix(kind))).map_err(ApplicationError::from_command)?;
            let mut database = open_database().map_err(ApplicationError::from_command)?;
            let link = database
                .remove_note_link(note_id, kind, &record_id)
                .map_err(ApplicationError::from_command)?;
            renderer
                .render_note_link("removed", &link, &link.record_id)
                .map_err(|error| {
                    ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
                })?
        }
        NoteLinkCommand::List { note_id } => {
            let note_id = parse_note_id(&note_id).map_err(ApplicationError::from_command)?;
            let database = open_database().map_err(ApplicationError::from_command)?;
            if database
                .note(note_id)
                .map_err(ApplicationError::from_command)?
                .is_none()
            {
                return Err(
                    ApplicationError::from_command(CommandError::Storage(StorageError::NoteNotFound {
                        id: note_id.to_string(),
                    }))
                    .into(),
                );
            }
            let links = database
                .note_links()
                .map_err(ApplicationError::from_command)?
                .into_iter()
                .filter(|link| link.note_id == note_id)
                .collect::<Vec<_>>();
            let lines = links
                .iter()
                .map(|link| format!("{}\t{}", link.record_kind.as_str(), link.record_id))
                .collect::<Vec<_>>();
            renderer
                .render_relationships("links", &links, &lines, "No note links.")
                .map_err(|error| {
                    ApplicationError::from_command(CommandError::InvalidFilter { message: error.to_string() })
                })?
        }
    };
    write_output(message)
}
