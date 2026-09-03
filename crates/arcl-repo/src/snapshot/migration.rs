use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};

use serde::Deserialize;

use arcl_core::domain::*;

use super::{
    CaptureRecord, PhaseRecord, PlanRecord, ReleaseRecord, SnapshotError, SnapshotRecord, SnapshotReference,
    SpecRecord, TaskRecord,
};

/// A record from the pre-connected snapshot format.
#[derive(Clone, Debug)]
pub(crate) enum LegacyRecord {
    Idea(LegacyIdea),
    Release(LegacyRelease),
    Epic(LegacyEpic),
    Milestone(LegacyMilestone),
    Task(LegacyTask),
}

#[derive(Clone, Debug)]
pub(crate) struct LegacyIdea {
    pub id: IdeaId,
    pub title: String,
    pub status: CaptureStatus,
    pub promoted_to: Option<EpicId>,
    pub body: String,
}

#[derive(Clone, Debug)]
pub(crate) struct LegacyRelease {
    pub id: ReleaseId,
    pub title: String,
    pub status: ContainerStatus,
    pub body: String,
}

#[derive(Clone, Debug)]
pub(crate) struct LegacyEpic {
    pub id: EpicId,
    pub title: String,
    pub status: ContainerStatus,
    pub _spec_path: String,
    pub release: Option<ReleaseId>,
    pub source_idea: Option<IdeaId>,
    pub body: String,
}

#[derive(Clone, Debug)]
pub(crate) struct LegacyMilestone {
    pub id: MilestoneId,
    pub title: String,
    pub status: ContainerStatus,
    pub epic: EpicId,
    pub position: i64,
    pub plan_key: Option<String>,
    pub body: String,
}

#[derive(Clone, Debug)]
pub(crate) struct LegacyTask {
    pub id: TaskId,
    pub title: String,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub milestone: MilestoneId,
    pub position: i64,
    pub parent: Option<TaskId>,
    pub plan_key: Option<String>,
    pub blocked_by: Vec<TaskId>,
    pub handoff: Option<String>,
    pub evidence: Option<String>,
    pub body: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct LegacyIdeaFrontMatter {
    id: IdeaId,
    title: String,
    status: CaptureStatus,
    promoted_to: Option<EpicId>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct LegacyReleaseFrontMatter {
    id: ReleaseId,
    title: String,
    status: ContainerStatus,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct LegacyEpicFrontMatter {
    id: EpicId,
    title: String,
    status: ContainerStatus,
    spec_path: String,
    release: Option<ReleaseId>,
    source_idea: Option<IdeaId>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct LegacyMilestoneFrontMatter {
    id: MilestoneId,
    title: String,
    status: ContainerStatus,
    epic: EpicId,
    position: i64,
    plan_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct LegacyTaskFrontMatter {
    id: TaskId,
    title: String,
    status: TaskStatus,
    priority: TaskPriority,
    milestone: MilestoneId,
    position: i64,
    parent: Option<TaskId>,
    plan_key: Option<String>,
    #[serde(default)]
    blocked_by: Vec<TaskId>,
    handoff: Option<String>,
    evidence: Option<String>,
}

/// Decode one record from the format-version 1 workspace.
pub(crate) fn decode_legacy_record(path: &Path, input: &str) -> Result<LegacyRecord, SnapshotError> {
    let (directory, filename_id) = legacy_record_path(path)?;
    let (front_matter, body) = split_document(input)?;
    let record = match directory {
        "ideas" => {
            let front_matter: LegacyIdeaFrontMatter = parse_front_matter(&front_matter)?;
            validate_legacy_title(&front_matter.title)?;
            LegacyRecord::Idea(LegacyIdea {
                id: front_matter.id,
                title: front_matter.title,
                status: front_matter.status,
                promoted_to: front_matter.promoted_to,
                body,
            })
        }
        "releases" => {
            let front_matter: LegacyReleaseFrontMatter = parse_front_matter(&front_matter)?;
            validate_legacy_title(&front_matter.title)?;
            LegacyRecord::Release(LegacyRelease {
                id: front_matter.id,
                title: front_matter.title,
                status: front_matter.status,
                body,
            })
        }
        "epics" => {
            let front_matter: LegacyEpicFrontMatter = parse_front_matter(&front_matter)?;
            validate_legacy_title(&front_matter.title)?;
            if front_matter.spec_path.is_empty() {
                return Err(SnapshotError::EmptyRecordSpecPath);
            }
            LegacyRecord::Epic(LegacyEpic {
                id: front_matter.id,
                title: front_matter.title,
                status: front_matter.status,
                _spec_path: front_matter.spec_path,
                release: front_matter.release,
                source_idea: front_matter.source_idea,
                body,
            })
        }
        "milestones" => {
            let front_matter: LegacyMilestoneFrontMatter = parse_front_matter(&front_matter)?;
            validate_legacy_title(&front_matter.title)?;
            validate_legacy_position(front_matter.position)?;
            LegacyRecord::Milestone(LegacyMilestone {
                id: front_matter.id,
                title: front_matter.title,
                status: front_matter.status,
                epic: front_matter.epic,
                position: front_matter.position,
                plan_key: front_matter.plan_key.filter(|key| !key.is_empty()),
                body,
            })
        }
        "tasks" => {
            let front_matter: LegacyTaskFrontMatter = parse_front_matter(&front_matter)?;
            validate_legacy_title(&front_matter.title)?;
            validate_legacy_position(front_matter.position)?;
            LegacyRecord::Task(LegacyTask {
                id: front_matter.id,
                title: front_matter.title,
                status: front_matter.status,
                priority: front_matter.priority,
                milestone: front_matter.milestone,
                position: front_matter.position,
                parent: front_matter.parent,
                plan_key: front_matter.plan_key.filter(|key| !key.is_empty()),
                blocked_by: front_matter.blocked_by,
                handoff: normalize_optional_markdown(front_matter.handoff),
                evidence: normalize_optional_markdown(front_matter.evidence),
                body,
            })
        }
        _ => unreachable!("legacy_record_path only returns legacy directories"),
    };

    if legacy_id_string(&record) != filename_id {
        return Err(SnapshotError::FilenameIdMismatch { id: legacy_id_string(&record), filename: filename_id });
    }
    Ok(record)
}

/// Convert a version-1 graph into the connected planning record format.
pub(crate) fn migrate_legacy_records(
    records: &BTreeMap<PathBuf, LegacyRecord>, _project_id: ProjectId,
) -> Result<Vec<SnapshotRecord>, SnapshotError> {
    let mut ideas = Vec::new();
    let mut releases = Vec::new();
    let mut epics = Vec::new();
    let mut milestones = Vec::new();
    let mut tasks = Vec::new();

    for record in records.values() {
        match record {
            LegacyRecord::Idea(record) => ideas.push(record),
            LegacyRecord::Release(record) => releases.push(record),
            LegacyRecord::Epic(record) => epics.push(record),
            LegacyRecord::Milestone(record) => milestones.push(record),
            LegacyRecord::Task(record) => tasks.push(record),
        }
    }

    let milestone_by_id = milestones
        .iter()
        .map(|record| (record.id, *record))
        .collect::<BTreeMap<_, _>>();
    let mut migrated = Vec::new();

    for record in ideas {
        migrated.push(SnapshotRecord::Capture(CaptureRecord {
            id: CaptureId::from_ulid(record.id.ulid()),
            title: record.title.clone(),
            status: record.status,
            created_at: "1970-01-01T00:00:00.000Z".to_owned(),
            promoted_to: record
                .promoted_to
                .map(|id| SnapshotReference::new("spec", SpecId::from_ulid(id.ulid()).to_string())),
            links: Vec::new(),
            body: record.body.clone(),
        }));
    }

    for record in releases {
        let members = epics
            .iter()
            .filter(|epic| epic.release == Some(record.id))
            .map(|epic| SnapshotReference::new("spec", spec_id(epic.id).to_string()))
            .collect();
        migrated.push(SnapshotRecord::Release(ReleaseRecord {
            id: record.id,
            title: record.title.clone(),
            status: record.status,
            body: record.body.clone(),
            members,
            links: Vec::new(),
        }));
    }

    for record in epics {
        migrated.push(SnapshotRecord::Spec(SpecRecord {
            id: SpecId::from_ulid(record.id.ulid()),
            title: record.title.clone(),
            status: record.status,
            source_capture_id: record.source_idea.map(|id| CaptureId::from_ulid(id.ulid())),
            acceptance_criteria: String::new(),
            links: Vec::new(),
            body: record.body.clone(),
        }));
        migrated.push(SnapshotRecord::Plan(PlanRecord {
            id: plan_id(record.id),
            spec_id: spec_id(record.id),
            title: format!("{} implementation plan", record.title),
            status: record.status,
            links: Vec::new(),
            body: String::new(),
        }));
    }

    for record in milestones {
        migrated.push(SnapshotRecord::Phase(PhaseRecord {
            id: PhaseId::from_ulid(record.id.ulid()),
            plan_id: plan_id(record.epic),
            plan_key: record.plan_key.clone(),
            title: record.title.clone(),
            status: record.status,
            position: record.position,
            links: Vec::new(),
            body: record.body.clone(),
        }));
    }

    for record in tasks {
        let milestone = milestone_by_id.get(&record.milestone).ok_or_else(|| {
            SnapshotError::InvalidRecordRelationship(format!(
                "task `{}` refers to missing milestone `{}`",
                record.id, record.milestone
            ))
        })?;
        let spec_id_value = Some(spec_id(milestone.epic));
        let plan_id_value = Some(plan_id(milestone.epic));
        let phase_id_value = Some(PhaseId::from_ulid(milestone.id.ulid()));
        migrated.push(SnapshotRecord::Task(TaskRecord {
            id: record.id,
            spec_id: spec_id_value,
            plan_id: plan_id_value,
            phase_id: phase_id_value,
            parent_id: record.parent,
            plan_key: record.plan_key.clone(),
            title: record.title.clone(),
            status: record.status,
            priority: record.priority,
            position: record.position,
            blocked_by: record.blocked_by.clone(),
            handoff: record.handoff.clone(),
            evidence: record.evidence.clone(),
            links: Vec::new(),
            body: record.body.clone(),
        }));
    }

    // Version 1 had no project record. Import attaches every converted record
    // to the target operational project before writing it.
    Ok(migrated)
}

fn spec_id(id: EpicId) -> SpecId {
    SpecId::from_ulid(id.ulid())
}
fn plan_id(id: EpicId) -> PlanId {
    PlanId::from_ulid(id.ulid())
}

fn legacy_id_string(record: &LegacyRecord) -> String {
    match record {
        LegacyRecord::Idea(record) => record.id.to_string(),
        LegacyRecord::Release(record) => record.id.to_string(),
        LegacyRecord::Epic(record) => record.id.to_string(),
        LegacyRecord::Milestone(record) => record.id.to_string(),
        LegacyRecord::Task(record) => record.id.to_string(),
    }
}

fn legacy_record_path(path: &Path) -> Result<(&str, String), SnapshotError> {
    let mut components = path.components();
    let directory = match components.next() {
        Some(Component::Normal(value)) => value.to_str(),
        _ => None,
    };
    let filename = match components.next() {
        Some(Component::Normal(value)) if components.next().is_none() => value.to_str(),
        _ => None,
    };
    let (Some(directory), Some(filename)) = (directory, filename) else {
        return Err(SnapshotError::InvalidRecordPath { path: path.to_owned() });
    };
    if !["ideas", "releases", "epics", "milestones", "tasks"].contains(&directory) {
        return Err(SnapshotError::KindPathMismatch { kind: directory.to_owned(), path: path.to_owned() });
    }
    let Some(filename_id) = filename.strip_suffix(".md") else {
        return Err(SnapshotError::InvalidRecordPath { path: path.to_owned() });
    };
    if filename_id.is_empty() {
        return Err(SnapshotError::InvalidRecordPath { path: path.to_owned() });
    }
    Ok((directory, filename_id.to_owned()))
}

fn split_document(input: &str) -> Result<(String, String), SnapshotError> {
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = normalized.split('\n');
    if lines.next() != Some("+++") {
        return Err(SnapshotError::InvalidRecordFormat(
            "the document must begin with an exact `+++` delimiter line".to_owned(),
        ));
    }
    let mut front_matter = Vec::new();
    let mut closing_index = None;
    for (index, line) in lines.clone().enumerate() {
        if line == "+++" {
            closing_index = Some(index + 1);
            break;
        }
        front_matter.push(line);
    }
    let closing_index = closing_index.ok_or_else(|| {
        SnapshotError::InvalidRecordFormat("front matter has no exact closing `+++` delimiter line".to_owned())
    })?;
    let mut body = normalized.split('\n').skip(closing_index + 1).collect::<Vec<_>>();
    if body.first() == Some(&"") {
        body.remove(0);
    }
    Ok((
        front_matter.join("\n"),
        body.join("\n").trim_end_matches('\n').to_owned(),
    ))
}

fn parse_front_matter<T: for<'de> serde::Deserialize<'de>>(input: &str) -> Result<T, SnapshotError> {
    toml::from_str(input).map_err(SnapshotError::FrontMatterParse)
}

fn validate_legacy_title(title: &str) -> Result<(), SnapshotError> {
    if title.trim().is_empty() { Err(SnapshotError::EmptyRecordTitle) } else { Ok(()) }
}

fn validate_legacy_position(position: i64) -> Result<(), SnapshotError> {
    if position < 0 { Err(SnapshotError::InvalidRecordPosition { position }) } else { Ok(()) }
}

fn normalize_optional_markdown(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.replace("\r\n", "\n").replace('\r', "\n");
        let value = value.trim_end_matches('\n').to_owned();
        (!value.is_empty()).then_some(value)
    })
}
