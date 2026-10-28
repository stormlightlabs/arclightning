use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use arcl_core::domain::{
    CaptureId, CaptureStatus, ContainerStatus, GateId, GateKind, GateStatus, NoteId, PhaseId, PlanId, ReleaseId,
    SpecId, TaskId, TaskNoteKind, TaskPriority, TaskStatus,
};

use super::{OLDEST_WORKSPACE_FORMAT_VERSION, Result, WORKSPACE_FORMAT_VERSION, WorkspaceError};

const FRONT_MATTER_DELIMITER: &str = "+++";

/// The stable kind and directory name of a workspace record.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RecordKind {
    /// An inbox capture.
    Capture,
    /// A release grouping explicit records.
    Release,
    /// An owned Markdown specification.
    Spec,
    /// A persistent implementation plan.
    Plan,
    /// An ordered phase belonging to a plan.
    Phase,
    /// A task at any supported planning level.
    Task,
    /// A Markdown note.
    Note,
}

impl RecordKind {
    /// Return the workspace directory for this record kind.
    pub const fn directory(self) -> &'static str {
        match self {
            Self::Capture => "captures",
            Self::Release => "releases",
            Self::Spec => "specs",
            Self::Plan => "plans",
            Self::Phase => "phases",
            Self::Task => "tasks",
            Self::Note => "notes",
        }
    }

    /// Return the stable kind name used in references and errors.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Capture => "capture",
            Self::Release => "release",
            Self::Spec => "spec",
            Self::Plan => "plan",
            Self::Phase => "phase",
            Self::Task => "task",
            Self::Note => "note",
        }
    }

    pub(crate) fn from_directory(directory: &str) -> Option<Self> {
        match directory {
            "captures" => Some(Self::Capture),
            "releases" => Some(Self::Release),
            "specs" => Some(Self::Spec),
            "plans" => Some(Self::Plan),
            "phases" => Some(Self::Phase),
            "tasks" => Some(Self::Task),
            "notes" => Some(Self::Note),
            _ => None,
        }
    }
}

/// A reference to another workspace record.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceReference {
    /// The referenced record kind.
    pub kind: String,
    /// The referenced record ID.
    pub id: String,
}

impl WorkspaceReference {
    /// Construct a reference with a stable kind and ID.
    pub fn new(kind: impl Into<String>, id: impl Into<String>) -> Self {
        Self { kind: kind.into(), id: id.into() }
    }
}

/// One note in a task file's `notes` array, which keeps notes oldest first.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct TaskNoteRecord {
    /// When the note was added, as an RFC 3339 UTC timestamp.
    pub created_at: String,
    /// What the note records, when given.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<TaskNoteKind>,
    /// The plain-text note, which may span lines.
    pub text: String,
}

/// One gate in a task file's `gates` array, which keeps gates oldest first.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct TaskGateRecord {
    /// The gate's ID, `arcl-g-…`.
    pub id: GateId,
    /// What the gate waits for.
    pub kind: GateKind,
    /// The pull request or run URL, or the date as `YYYY-MM-DD`.
    pub target: String,
    /// Whether the gate is still open.
    pub status: GateStatus,
    /// When the gate was added, as an RFC 3339 UTC timestamp.
    pub created_at: String,
    /// When the gate was found met. Present exactly when it is met.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub met_at: Option<String>,
}

/// The workspace manifest's stable format metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceManifest {
    #[serde(rename = "format-version")]
    pub format_version: u32,
}

/// Backwards-friendly short name for the workspace manifest.
pub type Manifest = WorkspaceManifest;

/// The default manifest carries [`OLDEST_WORKSPACE_FORMAT_VERSION`], the
/// version an empty workspace needs, so an older arcl reads a new workspace
/// until a claim moves it to a later version.
impl Default for WorkspaceManifest {
    fn default() -> Self {
        Self { format_version: OLDEST_WORKSPACE_FORMAT_VERSION }
    }
}

impl WorkspaceManifest {
    /// Parse and validate a workspace manifest.
    pub fn parse(input: &str) -> Result<Self> {
        let manifest = toml::from_str::<Self>(input).map_err(WorkspaceError::ManifestParse)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validate that this manifest uses a workspace format this crate reads.
    pub fn validate(&self) -> Result<()> {
        if !(OLDEST_WORKSPACE_FORMAT_VERSION..=WORKSPACE_FORMAT_VERSION).contains(&self.format_version) {
            return Err(WorkspaceError::UnsupportedVersion {
                found: self.format_version,
                expected: WORKSPACE_FORMAT_VERSION,
            });
        }
        Ok(())
    }

    /// Render the canonical manifest with one LF-terminated line.
    pub fn render(&self) -> Result<String> {
        self.validate()?;
        Ok(format!("format-version = {}\n", self.format_version))
    }
}

/// An inbox capture encoded in a workspace record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureRecord {
    pub id: CaptureId,
    pub title: String,
    pub status: CaptureStatus,
    pub created_at: String,
    /// The single promotion edge, when the capture has been promoted.
    pub promoted_to: Option<WorkspaceReference>,
    pub links: Vec<WorkspaceReference>,
    pub body: String,
}

/// A release encoded in a workspace record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseRecord {
    pub id: ReleaseId,
    pub title: String,
    pub status: ContainerStatus,
    pub body: String,
    /// Explicit members. Descendants are never implied by this list.
    pub members: Vec<WorkspaceReference>,
    /// Explicit outgoing record links.
    pub links: Vec<WorkspaceReference>,
}

/// An owned specification encoded in a workspace record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpecRecord {
    pub id: SpecId,
    pub title: String,
    pub status: ContainerStatus,
    pub source_capture_id: Option<CaptureId>,
    pub acceptance_criteria: String,
    pub links: Vec<WorkspaceReference>,
    pub body: String,
}

/// A persistent implementation plan encoded in a workspace record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanRecord {
    pub id: PlanId,
    pub spec_id: SpecId,
    pub title: String,
    pub status: ContainerStatus,
    pub links: Vec<WorkspaceReference>,
    pub body: String,
}

/// An optional ordered phase encoded in a workspace record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhaseRecord {
    pub id: PhaseId,
    pub plan_id: PlanId,
    pub plan_key: Option<String>,
    pub title: String,
    pub status: ContainerStatus,
    pub position: i64,
    pub links: Vec<WorkspaceReference>,
    pub body: String,
}

/// A flexible planning task encoded in a workspace record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskRecord {
    pub id: TaskId,
    pub spec_id: Option<SpecId>,
    pub plan_id: Option<PlanId>,
    pub phase_id: Option<PhaseId>,
    pub parent_id: Option<TaskId>,
    pub plan_key: Option<String>,
    pub title: String,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub position: i64,
    pub blocked_by: Vec<TaskId>,
    pub handoff: Option<String>,
    pub evidence: Option<String>,
    /// Who claimed the task when starting it.
    pub assignee: Option<String>,
    pub links: Vec<WorkspaceReference>,
    /// Notes on the task, oldest first.
    pub notes: Vec<TaskNoteRecord>,
    /// Gates on the task, oldest first.
    pub gates: Vec<TaskGateRecord>,
    pub body: String,
}

/// A Markdown note encoded in a workspace record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoteRecord {
    pub id: NoteId,
    pub title: String,
    pub links: Vec<WorkspaceReference>,
    pub body: String,
}

/// A typed record in the versioned workspace projection.
// A task is the largest record. Records are read one per file and kept in
// maps, so boxing it would save little memory and change every match.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkspaceRecord {
    /// An inbox capture.
    Capture(CaptureRecord),
    /// A release.
    Release(ReleaseRecord),
    /// An owned specification.
    Spec(SpecRecord),
    /// A persistent plan.
    Plan(PlanRecord),
    /// A plan phase.
    Phase(PhaseRecord),
    /// A planning task.
    Task(TaskRecord),
    /// A Markdown note.
    Note(NoteRecord),
}

impl WorkspaceRecord {
    /// Return the record's typed kind.
    pub const fn kind(&self) -> RecordKind {
        match self {
            Self::Capture(_) => RecordKind::Capture,
            Self::Release(_) => RecordKind::Release,
            Self::Spec(_) => RecordKind::Spec,
            Self::Plan(_) => RecordKind::Plan,
            Self::Phase(_) => RecordKind::Phase,
            Self::Task(_) => RecordKind::Task,
            Self::Note(_) => RecordKind::Note,
        }
    }

    /// Return the canonical ID string used by the record filename.
    pub fn id_string(&self) -> String {
        match self {
            Self::Capture(record) => record.id.to_string(),
            Self::Release(record) => record.id.to_string(),
            Self::Spec(record) => record.id.to_string(),
            Self::Plan(record) => record.id.to_string(),
            Self::Phase(record) => record.id.to_string(),
            Self::Task(record) => record.id.to_string(),
            Self::Note(record) => record.id.to_string(),
        }
    }

    /// Return the canonical workspace-relative path for this record.
    pub fn path(&self) -> PathBuf {
        PathBuf::from(self.kind().directory()).join(format!("{}.md", self.id_string()))
    }

    /// Render this record with its canonical path and Markdown body.
    pub fn render(&self) -> Result<String> {
        render_record_document(self)
    }

    /// Render this record after checking its kind and ID against `path`.
    pub fn render_at(&self, path: &Path) -> Result<String> {
        validate_record_path(path, self.kind(), &self.id_string())?;
        self.render()
    }
}

/// Parse the canonical manifest from a workspace manifest file.
pub fn decode_manifest(input: &str) -> Result<WorkspaceManifest> {
    WorkspaceManifest::parse(input)
}

/// Encode a validated manifest in canonical TOML form.
pub fn encode_manifest(manifest: &WorkspaceManifest) -> Result<String> {
    manifest.render()
}

/// Parse a record, using its path to select and validate its typed kind.
pub fn decode_record(path: &Path, input: &str) -> Result<WorkspaceRecord> {
    let (kind, filename_id) = parse_record_path(path)?;
    let (front_matter, body) = split_record_document(input)?;
    let record = match kind {
        RecordKind::Capture => {
            WorkspaceRecord::Capture(CaptureRecord::from_wire(parse_front_matter(&front_matter)?, body)?)
        }
        RecordKind::Release => {
            WorkspaceRecord::Release(ReleaseRecord::from_wire(parse_front_matter(&front_matter)?, body)?)
        }
        RecordKind::Spec => WorkspaceRecord::Spec(SpecRecord::from_wire(parse_front_matter(&front_matter)?, body)?),
        RecordKind::Plan => WorkspaceRecord::Plan(PlanRecord::from_wire(parse_front_matter(&front_matter)?, body)?),
        RecordKind::Phase => WorkspaceRecord::Phase(PhaseRecord::from_wire(parse_front_matter(&front_matter)?, body)?),
        RecordKind::Task => WorkspaceRecord::Task(TaskRecord::from_wire(parse_front_matter(&front_matter)?, body)?),
        RecordKind::Note => WorkspaceRecord::Note(NoteRecord::from_wire(parse_front_matter(&front_matter)?, body)?),
    };

    if record.id_string() != filename_id {
        return Err(WorkspaceError::FilenameIdMismatch { id: record.id_string(), filename: filename_id });
    }
    Ok(record)
}

/// Encode a record after checking its kind and ID against `path`.
pub fn encode_record(path: &Path, record: &WorkspaceRecord) -> Result<String> {
    validate_record_path(path, record.kind(), &record.id_string())?;
    record.render()
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct CaptureFrontMatter {
    id: CaptureId,
    title: String,
    status: CaptureStatus,
    created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    promoted_to: Option<WorkspaceReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    links: Vec<WorkspaceReference>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct ReleaseFrontMatter {
    id: ReleaseId,
    title: String,
    status: ContainerStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    members: Vec<WorkspaceReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    links: Vec<WorkspaceReference>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct SpecFrontMatter {
    id: SpecId,
    title: String,
    status: ContainerStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_capture: Option<CaptureId>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    acceptance_criteria: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    links: Vec<WorkspaceReference>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct PlanFrontMatter {
    id: PlanId,
    spec: SpecId,
    title: String,
    status: ContainerStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    links: Vec<WorkspaceReference>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct PhaseFrontMatter {
    id: PhaseId,
    plan: PlanId,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan_key: Option<String>,
    title: String,
    status: ContainerStatus,
    position: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    links: Vec<WorkspaceReference>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct TaskFrontMatter {
    id: TaskId,
    #[serde(skip_serializing_if = "Option::is_none")]
    spec: Option<SpecId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan: Option<PlanId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    phase: Option<PhaseId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent: Option<TaskId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan_key: Option<String>,
    title: String,
    status: TaskStatus,
    priority: TaskPriority,
    position: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    blocked_by: Vec<TaskId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    handoff: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    evidence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assignee: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    links: Vec<WorkspaceReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    notes: Vec<TaskNoteRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    gates: Vec<TaskGateRecord>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct NoteFrontMatter {
    id: NoteId,
    title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    links: Vec<WorkspaceReference>,
}

impl CaptureRecord {
    fn from_wire(front_matter: CaptureFrontMatter, body: String) -> Result<Self> {
        validate_title(&front_matter.title)?;
        if front_matter.created_at.trim().is_empty() {
            return Err(WorkspaceError::EmptyRecordCreatedAt);
        }
        Ok(Self {
            id: front_matter.id,
            title: front_matter.title,
            status: front_matter.status,
            created_at: normalize_line_endings(&front_matter.created_at),
            promoted_to: front_matter.promoted_to,
            links: normalize_references(front_matter.links),
            body,
        })
    }

    fn front_matter(&self) -> Result<CaptureFrontMatter> {
        validate_title(&self.title)?;
        if self.created_at.trim().is_empty() {
            return Err(WorkspaceError::EmptyRecordCreatedAt);
        }
        Ok(CaptureFrontMatter {
            id: self.id,
            title: self.title.clone(),
            status: self.status,
            created_at: normalize_line_endings(&self.created_at),
            promoted_to: self.promoted_to.clone(),
            links: {
                let mut links = self.links.clone();
                links.sort();
                links
            },
        })
    }
}

impl ReleaseRecord {
    fn from_wire(front_matter: ReleaseFrontMatter, body: String) -> Result<Self> {
        validate_title(&front_matter.title)?;
        Ok(Self {
            id: front_matter.id,
            title: front_matter.title,
            status: front_matter.status,
            body,
            members: normalize_references(front_matter.members),
            links: normalize_references(front_matter.links),
        })
    }

    fn front_matter(&self) -> Result<ReleaseFrontMatter> {
        validate_title(&self.title)?;
        let mut members = self.members.clone();
        let mut links = self.links.clone();
        members.sort();
        links.sort();
        Ok(ReleaseFrontMatter { id: self.id, title: self.title.clone(), status: self.status, members, links })
    }
}

impl SpecRecord {
    fn from_wire(front_matter: SpecFrontMatter, body: String) -> Result<Self> {
        validate_title(&front_matter.title)?;
        Ok(Self {
            id: front_matter.id,
            title: front_matter.title,
            status: front_matter.status,
            source_capture_id: front_matter.source_capture,
            acceptance_criteria: normalize_body(&front_matter.acceptance_criteria),
            links: normalize_references(front_matter.links),
            body,
        })
    }

    fn front_matter(&self) -> Result<SpecFrontMatter> {
        validate_title(&self.title)?;
        let mut links = self.links.clone();
        links.sort();
        Ok(SpecFrontMatter {
            id: self.id,
            title: self.title.clone(),
            status: self.status,
            source_capture: self.source_capture_id,
            acceptance_criteria: normalize_body(&self.acceptance_criteria),
            links,
        })
    }
}

impl PlanRecord {
    fn from_wire(front_matter: PlanFrontMatter, body: String) -> Result<Self> {
        validate_title(&front_matter.title)?;
        Ok(Self {
            id: front_matter.id,
            spec_id: front_matter.spec,
            title: front_matter.title,
            status: front_matter.status,
            links: normalize_references(front_matter.links),
            body,
        })
    }

    fn front_matter(&self) -> Result<PlanFrontMatter> {
        validate_title(&self.title)?;
        let mut links = self.links.clone();
        links.sort();
        Ok(PlanFrontMatter { id: self.id, spec: self.spec_id, title: self.title.clone(), status: self.status, links })
    }
}

impl PhaseRecord {
    fn from_wire(front_matter: PhaseFrontMatter, body: String) -> Result<Self> {
        validate_title(&front_matter.title)?;
        validate_position(front_matter.position)?;
        Ok(Self {
            id: front_matter.id,
            plan_id: front_matter.plan,
            plan_key: normalize_optional(front_matter.plan_key),
            title: front_matter.title,
            status: front_matter.status,
            position: front_matter.position,
            links: normalize_references(front_matter.links),
            body,
        })
    }

    fn front_matter(&self) -> Result<PhaseFrontMatter> {
        validate_title(&self.title)?;
        validate_position(self.position)?;
        let mut links = self.links.clone();
        links.sort();
        Ok(PhaseFrontMatter {
            id: self.id,
            plan: self.plan_id,
            plan_key: normalize_optional(self.plan_key.clone()),
            title: self.title.clone(),
            status: self.status,
            position: self.position,
            links,
        })
    }
}

impl TaskRecord {
    fn from_wire(front_matter: TaskFrontMatter, body: String) -> Result<Self> {
        validate_title(&front_matter.title)?;
        validate_position(front_matter.position)?;
        let mut blocked_by = front_matter.blocked_by;
        blocked_by.sort();
        Ok(Self {
            id: front_matter.id,
            spec_id: front_matter.spec,
            plan_id: front_matter.plan,
            phase_id: front_matter.phase,
            parent_id: front_matter.parent,
            plan_key: normalize_optional(front_matter.plan_key),
            title: front_matter.title,
            status: front_matter.status,
            priority: front_matter.priority,
            position: front_matter.position,
            blocked_by,
            handoff: normalize_optional_markdown(front_matter.handoff),
            evidence: normalize_optional_markdown(front_matter.evidence),
            assignee: normalize_assignee(front_matter.assignee, front_matter.status)?,
            links: normalize_references(front_matter.links),
            notes: normalize_task_notes(front_matter.notes)?,
            gates: normalize_task_gates(front_matter.gates)?,
            body,
        })
    }

    fn front_matter(&self) -> Result<TaskFrontMatter> {
        validate_title(&self.title)?;
        validate_position(self.position)?;
        let mut blocked_by = self.blocked_by.clone();
        let mut links = self.links.clone();
        blocked_by.sort();
        links.sort();
        Ok(TaskFrontMatter {
            id: self.id,
            spec: self.spec_id,
            plan: self.plan_id,
            phase: self.phase_id,
            parent: self.parent_id,
            plan_key: normalize_optional(self.plan_key.clone()),
            title: self.title.clone(),
            status: self.status,
            priority: self.priority,
            position: self.position,
            blocked_by,
            handoff: normalize_optional_markdown(self.handoff.clone()),
            evidence: normalize_optional_markdown(self.evidence.clone()),
            assignee: normalize_assignee(self.assignee.clone(), self.status)?,
            links,
            notes: normalize_task_notes(self.notes.clone())?,
            gates: normalize_task_gates(self.gates.clone())?,
        })
    }
}

impl NoteRecord {
    fn from_wire(front_matter: NoteFrontMatter, body: String) -> Result<Self> {
        validate_title(&front_matter.title)?;
        Ok(Self {
            id: front_matter.id,
            title: front_matter.title,
            links: normalize_references(front_matter.links),
            body,
        })
    }

    fn front_matter(&self) -> Result<NoteFrontMatter> {
        validate_title(&self.title)?;
        let mut links = self.links.clone();
        links.sort();
        Ok(NoteFrontMatter { id: self.id, title: self.title.clone(), links })
    }
}

fn render_record_document(record: &WorkspaceRecord) -> Result<String> {
    let front_matter = match record {
        WorkspaceRecord::Capture(record) => render_front_matter(&record.front_matter()?)?,
        WorkspaceRecord::Release(record) => render_front_matter(&record.front_matter()?)?,
        WorkspaceRecord::Spec(record) => render_front_matter(&record.front_matter()?)?,
        WorkspaceRecord::Plan(record) => render_front_matter(&record.front_matter()?)?,
        WorkspaceRecord::Phase(record) => render_front_matter(&record.front_matter()?)?,
        WorkspaceRecord::Task(record) => render_front_matter(&record.front_matter()?)?,
        WorkspaceRecord::Note(record) => render_front_matter(&record.front_matter()?)?,
    };
    let body = match record {
        WorkspaceRecord::Capture(record) => normalize_body(&record.body),
        WorkspaceRecord::Release(record) => normalize_body(&record.body),
        WorkspaceRecord::Spec(record) => normalize_body(&record.body),
        WorkspaceRecord::Plan(record) => normalize_body(&record.body),
        WorkspaceRecord::Phase(record) => normalize_body(&record.body),
        WorkspaceRecord::Task(record) => normalize_body(&record.body),
        WorkspaceRecord::Note(record) => normalize_body(&record.body),
    };

    if body.is_empty() {
        Ok(format!(
            "{FRONT_MATTER_DELIMITER}\n{front_matter}{FRONT_MATTER_DELIMITER}\n"
        ))
    } else {
        Ok(format!(
            "{FRONT_MATTER_DELIMITER}\n{front_matter}{FRONT_MATTER_DELIMITER}\n\n{body}\n"
        ))
    }
}

fn render_front_matter<T: Serialize>(front_matter: &T) -> Result<String> {
    let rendered = toml::to_string(front_matter)?;
    Ok(rendered.trim_end_matches('\n').to_owned() + "\n")
}

fn parse_front_matter<T: for<'de> Deserialize<'de>>(input: &str) -> Result<T> {
    toml::from_str(input).map_err(WorkspaceError::FrontMatterParse)
}

fn split_record_document(input: &str) -> Result<(String, String)> {
    let normalized = normalize_line_endings(input);
    let mut lines = normalized.split('\n');
    if lines.next() != Some(FRONT_MATTER_DELIMITER) {
        return Err(WorkspaceError::InvalidRecordFormat(
            "the document must begin with an exact `+++` delimiter line".to_owned(),
        ));
    }

    let mut front_matter_lines = Vec::new();
    let mut closing_index = None;
    for (index, line) in lines.clone().enumerate() {
        if line == FRONT_MATTER_DELIMITER {
            closing_index = Some(index + 1);
            break;
        }
        front_matter_lines.push(line);
    }
    let closing_index = closing_index.ok_or_else(|| {
        WorkspaceError::InvalidRecordFormat("front matter has no exact closing `+++` delimiter line".to_owned())
    })?;

    let mut body_lines: Vec<&str> = normalized.split('\n').skip(closing_index + 1).collect();
    if body_lines.first() == Some(&"") {
        body_lines.remove(0);
    }
    Ok((front_matter_lines.join("\n"), normalize_body(&body_lines.join("\n"))))
}

fn parse_record_path(path: &Path) -> Result<(RecordKind, String)> {
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
        return Err(WorkspaceError::InvalidRecordPath { path: path.to_owned() });
    };
    let Some(kind) = RecordKind::from_directory(directory) else {
        return Err(WorkspaceError::KindPathMismatch { kind: directory.to_owned(), path: path.to_owned() });
    };
    let Some(filename_id) = filename.strip_suffix(".md") else {
        return Err(WorkspaceError::InvalidRecordPath { path: path.to_owned() });
    };
    if filename_id.is_empty() {
        return Err(WorkspaceError::InvalidRecordPath { path: path.to_owned() });
    }
    Ok((kind, filename_id.to_owned()))
}

fn validate_record_path(path: &Path, kind: RecordKind, id: &str) -> Result<()> {
    let (path_kind, filename_id) = parse_record_path(path)?;
    if path_kind != kind {
        return Err(WorkspaceError::KindPathMismatch { kind: kind.as_str().to_owned(), path: path.to_owned() });
    }
    if filename_id != id {
        return Err(WorkspaceError::FilenameIdMismatch { id: id.to_owned(), filename: filename_id });
    }
    Ok(())
}

fn validate_title(title: &str) -> Result<()> {
    if title.trim().is_empty() { Err(WorkspaceError::EmptyRecordTitle) } else { Ok(()) }
}

fn validate_position(position: i64) -> Result<()> {
    if position < 0 { Err(WorkspaceError::InvalidRecordPosition { position }) } else { Ok(()) }
}

/// Trim a task's `assignee`, or refuse one that is empty, holds a control
/// character, or sits on a task that is not in progress. Parking, handing
/// off, completing and cancelling release a claim, so only an in-progress
/// task holds one.
fn normalize_assignee(value: Option<String>, status: TaskStatus) -> Result<Option<String>> {
    let Some(value) = value else { return Ok(None) };
    if status != TaskStatus::InProgress {
        return Err(WorkspaceError::AssigneeNotInProgress { status: status.as_str().to_owned() });
    }
    arcl_core::domain::validate_assignee(&value)
        .map(Some)
        .map_err(|_| WorkspaceError::InvalidRecordAssignee { value })
}

/// Validate each task note's text as [`validate_task_note_text`] does,
/// keeping the normalized text, and normalize `created-at` as
/// [`normalize_note_created_at`] does. The order is kept: it is the order the
/// notes were added.
///
/// [`validate_task_note_text`]: arcl_core::domain::validate_task_note_text
fn normalize_task_notes(notes: Vec<TaskNoteRecord>) -> Result<Vec<TaskNoteRecord>> {
    notes
        .into_iter()
        .map(|note| {
            let created_at = normalize_note_created_at(&note.created_at)?;
            let text = arcl_core::domain::validate_task_note_text(&note.text)
                .map_err(|_| WorkspaceError::InvalidRecordNote { value: note.text.clone() })?;
            Ok(TaskNoteRecord { created_at, kind: note.kind, text })
        })
        .collect()
}

/// Parse an RFC 3339 timestamp and write it in UTC with millisecond
/// precision and a `Z` suffix, the form `task note` stores
/// (`2026-10-27T10:00:00.000Z`). Digits past the millisecond are dropped, as
/// SQLite's `%f` does. Leap seconds and years outside 0000 to 9999 after the
/// shift to UTC are refused.
fn normalize_note_created_at(value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(WorkspaceError::EmptyRecordNoteCreatedAt);
    }
    parse_rfc3339_millis(trimmed)
        .and_then(format_utc_millis)
        .ok_or_else(|| WorkspaceError::InvalidRecordNoteCreatedAt { value: value.to_owned() })
}

/// Milliseconds since the Unix epoch for an RFC 3339
/// `date-time`, or `None` when `value` is not one.
fn parse_rfc3339_millis(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    let digits = |range: std::ops::Range<usize>| -> Option<i64> {
        let part = bytes.get(range)?;
        if part.is_empty() || !part.iter().all(u8::is_ascii_digit) {
            return None;
        }
        Some(part.iter().fold(0, |sum, digit| sum * 10 + i64::from(digit - b'0')))
    };
    let at = |index: usize, expected: &[u8]| bytes.get(index).is_some_and(|byte| expected.contains(byte));
    if !(at(4, b"-") && at(7, b"-") && at(10, b"Tt") && at(13, b":") && at(16, b":")) {
        return None;
    }
    let (year, month, day) = (digits(0..4)?, digits(5..7)?, digits(8..10)?);
    let (hour, minute, second) = (digits(11..13)?, digits(14..16)?, digits(17..19)?);
    if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) {
        return None;
    }
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let mut index = 19;
    let mut millis = 0;
    if at(index, b".") {
        let start = index + 1;
        let end = start + bytes[start..].iter().take_while(|byte| byte.is_ascii_digit()).count();
        if end == start {
            return None;
        }
        let kept = &value[start..end.min(start + 3)];
        millis = format!("{kept:0<3}").parse::<i64>().ok()?;
        index = end;
    }
    let offset_minutes = match bytes.get(index)? {
        b'Z' | b'z' if index + 1 == bytes.len() => 0,
        sign @ (b'+' | b'-') if index + 6 == bytes.len() && at(index + 3, b":") => {
            let (hours, minutes) = (digits(index + 1..index + 3)?, digits(index + 4..index + 6)?);
            if hours > 23 || minutes > 59 {
                return None;
            }
            let total = hours * 60 + minutes;
            if *sign == b'-' { -total } else { total }
        }
        _ => return None,
    };
    let days = days_from_civil(year, month, day);
    let seconds = days * 86_400 + hour * 3_600 + minute * 60 + second - offset_minutes * 60;
    Some(seconds * 1_000 + millis)
}

/// Write milliseconds since the Unix epoch as `YYYY-MM-DDTHH:MM:SS.mmmZ`, or
/// `None` when the year falls outside 0000 to 9999.
fn format_utc_millis(millis: i64) -> Option<String> {
    let seconds = millis.div_euclid(1_000);
    let (days, second_of_day) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    let (year, month, day) = civil_from_days(days);
    if !(0..=9999).contains(&year) {
        return None;
    }
    Some(format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        second_of_day / 3_600,
        second_of_day % 3_600 / 60,
        second_of_day % 60,
        millis.rem_euclid(1_000)
    ))
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days from 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The proleptic Gregorian date `days` after 1970-01-01, the inverse of
/// [`days_from_civil`].
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// Check each task gate's target as [`validate_gate_target`] does, keeping
/// the normalized target, write `created-at` and `met-at` in UTC to the
/// millisecond as task notes do, and refuse a timestamp that is not RFC 3339
/// or a `met-at` that is missing on a met gate or present on an open one.
/// The order is kept.
///
/// [`validate_gate_target`]: arcl_core::domain::validate_gate_target
fn normalize_task_gates(gates: Vec<TaskGateRecord>) -> Result<Vec<TaskGateRecord>> {
    gates
        .into_iter()
        .map(|gate| {
            let invalid = |reason: String| WorkspaceError::InvalidRecordGate { id: gate.id.to_string(), reason };
            let target = arcl_core::domain::validate_gate_target(gate.kind, &gate.target)
                .map_err(|error| invalid(error.to_string()))?;
            let timestamp = |field: &str, value: &str| {
                parse_rfc3339_millis(value.trim())
                    .and_then(format_utc_millis)
                    .ok_or_else(|| invalid(format!("`{field}` must be an RFC 3339 timestamp, not `{value}`")))
            };
            let created_at = timestamp("created-at", &gate.created_at)?;
            let met_at = gate
                .met_at
                .as_deref()
                .map(|value| timestamp("met-at", value))
                .transpose()?;
            if (gate.status == GateStatus::Met) != met_at.is_some() {
                return Err(invalid(
                    "`met-at` must be set on a met gate and only on a met gate".to_owned(),
                ));
            }
            Ok(TaskGateRecord { target, created_at, met_at, ..gate })
        })
        .collect()
}

fn normalize_line_endings(value: &str) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}

fn normalize_body(value: &str) -> String {
    normalize_line_endings(value).trim_end_matches('\n').to_owned()
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = normalize_line_endings(&value);
        (!value.is_empty()).then_some(value)
    })
}

fn normalize_optional_markdown(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = normalize_body(&value);
        (!value.is_empty()).then_some(value)
    })
}

fn normalize_references(mut references: Vec<WorkspaceReference>) -> Vec<WorkspaceReference> {
    for reference in &mut references {
        reference.kind = reference.kind.to_ascii_lowercase();
        reference.id = reference.id.trim().to_owned();
    }
    references.sort();
    references
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPTURE_ID: &str = "arcl-c-01K0B3N4QSC9R7K6W8X2M5YH1Z";
    const SPEC_ID: &str = "arcl-s-01K0B2ZWTX7JX9PH7W5G1S6A9Q";
    const PLAN_ID: &str = "arcl-pl-01K0B2ZWTX7JX9PH7W5G1S6A9Q";
    const PHASE_ID: &str = "arcl-ph-01K0B2ZWTX7JX9PH7W5G1S6A9Q";
    const TASK_ID: &str = "arcl-t-01K0B31M6VGK4YH8VKT4C0D2DR";
    const OTHER_TASK_ID: &str = "arcl-t-01K0B31M6VGK4YH8VKT4C0D2DS";
    const NOTE_ID: &str = "arcl-n-01K0B2ZWTX7JX9PH7W5G1S6A9Q";
    const GATE_ID: &str = "arcl-g-01K0B2ZWTX7JX9PH7W5G1S6A9R";
    const OTHER_GATE_ID: &str = "arcl-g-01K0B2ZWTX7JX9PH7W5G1S6A9S";

    #[test]
    fn manifest_reads_and_renders_versions_one_to_three() {
        assert_eq!(
            WorkspaceManifest::default().format_version,
            OLDEST_WORKSPACE_FORMAT_VERSION
        );
        for version in 1..=4 {
            let manifest = WorkspaceManifest::parse(&format!("format-version = {version}")).expect("manifest");
            assert_eq!(
                manifest.render().expect("render"),
                format!(
                    "format-version = {version}
"
                )
            );
        }
        assert_eq!(WORKSPACE_FORMAT_VERSION, 4);
        for version in [0, 5] {
            assert!(matches!(
                WorkspaceManifest::parse(&format!("format-version = {version}")),
                Err(WorkspaceError::UnsupportedVersion { found, expected: 4 }) if found == version
            ));
        }
    }

    #[test]
    fn all_new_record_kinds_round_trip() {
        let records = [
            WorkspaceRecord::Capture(CaptureRecord {
                id: CaptureId::parse(CAPTURE_ID).unwrap(),
                title: "Capture".to_owned(),
                status: CaptureStatus::Captured,
                created_at: "2025-01-01T00:00:00.000Z".to_owned(),
                promoted_to: None,
                links: vec![],
                body: "Capture **body**".to_owned(),
            }),
            WorkspaceRecord::Spec(SpecRecord {
                id: SpecId::parse(SPEC_ID).unwrap(),
                title: "Spec".to_owned(),
                status: ContainerStatus::Open,
                source_capture_id: Some(CaptureId::parse(CAPTURE_ID).unwrap()),
                acceptance_criteria: "- [ ] criterion".to_owned(),
                links: vec![],
                body: "# Spec".to_owned(),
            }),
            WorkspaceRecord::Plan(PlanRecord {
                id: PlanId::parse(PLAN_ID).unwrap(),
                spec_id: SpecId::parse(SPEC_ID).unwrap(),
                title: "Plan".to_owned(),
                status: ContainerStatus::Open,
                links: vec![],
                body: "Plan body".to_owned(),
            }),
            WorkspaceRecord::Phase(PhaseRecord {
                id: PhaseId::parse(PHASE_ID).unwrap(),
                plan_id: PlanId::parse(PLAN_ID).unwrap(),
                plan_key: Some("build".to_owned()),
                title: "Build".to_owned(),
                status: ContainerStatus::Open,
                position: 0,
                links: vec![],
                body: "Phase body".to_owned(),
            }),
            WorkspaceRecord::Task(TaskRecord {
                id: TaskId::parse(TASK_ID).unwrap(),
                spec_id: Some(SpecId::parse(SPEC_ID).unwrap()),
                plan_id: Some(PlanId::parse(PLAN_ID).unwrap()),
                phase_id: Some(PhaseId::parse(PHASE_ID).unwrap()),
                parent_id: None,
                plan_key: Some("build/task".to_owned()),
                title: "Task".to_owned(),
                status: TaskStatus::InProgress,
                priority: TaskPriority::High,
                position: 0,
                blocked_by: vec![TaskId::parse(OTHER_TASK_ID).unwrap()],
                handoff: Some("Resume here".to_owned()),
                evidence: Some("Evidence".to_owned()),
                assignee: Some("codex".to_owned()),
                links: vec![WorkspaceReference::new("spec", SPEC_ID)],
                notes: vec![
                    TaskNoteRecord {
                        created_at: "2026-10-27T10:00:00.000Z".to_owned(),
                        kind: Some(TaskNoteKind::Decision),
                        text: "Use SQLite".to_owned(),
                    },
                    TaskNoteRecord {
                        created_at: "2026-10-27T10:05:00.000Z".to_owned(),
                        kind: None,
                        text: "line one\n+++ two\nline \"three\"".to_owned(),
                    },
                ],
                gates: vec![
                    TaskGateRecord {
                        id: GateId::parse(GATE_ID).unwrap(),
                        kind: GateKind::Pr,
                        target: "https://github.com/o/r/pull/7".to_owned(),
                        status: GateStatus::Met,
                        created_at: "2026-10-27T10:00:00.000Z".to_owned(),
                        met_at: Some("2026-10-28T10:00:00.000Z".to_owned()),
                    },
                    TaskGateRecord {
                        id: GateId::parse(OTHER_GATE_ID).unwrap(),
                        kind: GateKind::Date,
                        target: "2026-11-01".to_owned(),
                        status: GateStatus::Open,
                        created_at: "2026-10-27T10:05:00.000Z".to_owned(),
                        met_at: None,
                    },
                ],
                body: "Task body".to_owned(),
            }),
            WorkspaceRecord::Note(NoteRecord {
                id: NoteId::parse(NOTE_ID).unwrap(),
                title: "Note".to_owned(),
                links: vec![WorkspaceReference::new("task", TASK_ID)],
                body: "Note body".to_owned(),
            }),
        ];

        for record in records {
            let path = record.path();
            let rendered = record.render().expect("render");
            let decoded = decode_record(&path, &rendered).expect("decode");
            assert_eq!(decoded, record);
            assert_eq!(decoded.render().expect("re-render"), rendered);
        }
    }

    #[test]
    fn task_metadata_is_sorted_and_empty_markdown_is_omitted() {
        let record = WorkspaceRecord::Task(TaskRecord {
            id: TaskId::parse(TASK_ID).unwrap(),
            spec_id: None,
            plan_id: None,
            phase_id: None,
            parent_id: None,
            plan_key: Some(String::new()),
            title: "Task".to_owned(),
            status: TaskStatus::Pending,
            priority: TaskPriority::Normal,
            position: 0,
            blocked_by: vec![TaskId::parse(OTHER_TASK_ID).unwrap(), TaskId::parse(TASK_ID).unwrap()],
            handoff: Some(String::new()),
            evidence: None,
            assignee: None,
            links: vec![],
            notes: vec![],
            gates: vec![],
            body: String::new(),
        });
        let rendered = record.render().expect("render");
        assert!(rendered.contains(&format!("blocked-by = [\"{TASK_ID}\", \"{OTHER_TASK_ID}\"]")));
        assert!(!rendered.contains("plan-key"));
        assert!(!rendered.contains("handoff"));
    }

    #[test]
    fn only_an_in_progress_task_holds_an_assignee() {
        let mut task = TaskRecord {
            id: TaskId::parse(TASK_ID).unwrap(),
            spec_id: None,
            plan_id: None,
            phase_id: None,
            parent_id: None,
            plan_key: None,
            title: "Task".to_owned(),
            status: TaskStatus::InProgress,
            priority: TaskPriority::Normal,
            position: 0,
            blocked_by: vec![],
            handoff: None,
            evidence: None,
            assignee: Some("codex".to_owned()),
            links: vec![],
            notes: vec![],
            gates: vec![],
            body: String::new(),
        };
        let path = WorkspaceRecord::Task(task.clone()).path();
        let rendered = WorkspaceRecord::Task(task.clone()).render().expect("render");
        assert!(rendered.contains("status = \"in_progress\""));
        for status in [
            TaskStatus::Pending,
            TaskStatus::Parked,
            TaskStatus::Completed,
            TaskStatus::Cancelled,
        ] {
            let input = rendered.replace("\"in_progress\"", &format!("\"{}\"", status.as_str()));
            assert!(
                matches!(
                    decode_record(&path, &input),
                    Err(WorkspaceError::AssigneeNotInProgress { status: ref found }) if found == status.as_str()
                ),
                "{status:?}"
            );
            task.status = status;
            assert!(matches!(
                WorkspaceRecord::Task(task.clone()).render(),
                Err(WorkspaceError::AssigneeNotInProgress { .. })
            ));
        }
    }

    #[test]
    fn task_notes_render_as_an_array_of_tables_and_refuse_invalid_text() {
        let record = WorkspaceRecord::Task(TaskRecord {
            id: TaskId::parse(TASK_ID).unwrap(),
            spec_id: None,
            plan_id: None,
            phase_id: None,
            parent_id: None,
            plan_key: None,
            title: "Task".to_owned(),
            status: TaskStatus::Completed,
            priority: TaskPriority::Normal,
            position: 0,
            blocked_by: vec![],
            handoff: None,
            evidence: None,
            assignee: None,
            links: vec![],
            notes: vec![TaskNoteRecord {
                created_at: "2026-10-27T10:00:00.000Z".to_owned(),
                kind: Some(TaskNoteKind::Result),
                text: "Done".to_owned(),
            }],
            gates: vec![],
            body: String::new(),
        });
        let path = record.path();
        let rendered = record.render().expect("render");
        assert!(
            rendered
                .contains("[[notes]]\ncreated-at = \"2026-10-27T10:00:00.000Z\"\nkind = \"result\"\ntext = \"Done\"\n"),
            "{rendered}"
        );

        let padded = rendered.replace("text = \"Done\"", "text = \"  Done  \"");
        assert_eq!(decode_record(&path, &padded).expect("decode"), record);
        for (from, to) in [
            ("text = \"Done\"", "text = \"  \""),
            ("text = \"Done\"", "text = \"bell\\u0007\""),
        ] {
            assert!(matches!(
                decode_record(&path, &rendered.replace(from, to)),
                Err(WorkspaceError::InvalidRecordNote { .. })
            ));
        }
        assert!(matches!(
            decode_record(&path, &rendered.replace("2026-10-27T10:00:00.000Z", " ")),
            Err(WorkspaceError::EmptyRecordNoteCreatedAt)
        ));
        assert!(matches!(
            decode_record(&path, &rendered.replace("\"result\"", "\"idea\"")),
            Err(WorkspaceError::FrontMatterParse(_))
        ));
    }

    #[test]
    fn task_note_creation_times_normalize_to_utc_milliseconds() {
        for (input, stored) in [
            ("2026-10-27T10:00:00.000Z", "2026-10-27T10:00:00.000Z"),
            ("2026-10-27T15:30:00+05:30", "2026-10-27T10:00:00.000Z"),
            ("2026-10-27t02:00:00.5-08:00", "2026-10-27T10:00:00.500Z"),
            ("2026-10-27T10:00:00.123456789z", "2026-10-27T10:00:00.123Z"),
            ("2024-03-01T00:30:00+01:00", "2024-02-29T23:30:00.000Z"),
            ("1999-12-31T23:00:00-01:00", "2000-01-01T00:00:00.000Z"),
        ] {
            assert_eq!(normalize_note_created_at(input).expect(input), stored, "{input}");
        }
        for input in [
            "yesterday",
            "+05:00",
            "2026-10-27",
            "2026-10-27T10:00:00",
            "2026-10-27 10:00:00Z",
            "2026-13-01T00:00:00Z",
            "2026-02-29T00:00:00Z",
            "2026-10-27T24:00:00Z",
            "2026-10-27T23:59:60Z",
            "2026-10-27T10:00:00.Z",
            "2026-10-27T10:00:00+0500",
            "2026-10-27T10:00:00+24:00",
            "2026-10-27T10:00:00Zjunk",
            "0000-01-01T00:00:00+01:00",
        ] {
            assert!(
                matches!(
                    normalize_note_created_at(input),
                    Err(WorkspaceError::InvalidRecordNoteCreatedAt { .. })
                ),
                "{input}"
            );
        }
    }

    #[test]
    fn task_note_with_an_offset_decodes_to_utc_and_renders_stably() {
        let input = format!(
            "+++\nid = \"{TASK_ID}\"\ntitle = \"Task\"\nstatus = \"completed\"\npriority = \"normal\"\nposition = 0\n\n\
             [[notes]]\ncreated-at = \"2026-10-27T15:30:00.25+05:30\"\ntext = \"Done\"\n+++\n"
        );
        let path = PathBuf::from(format!("tasks/{TASK_ID}.md"));
        let WorkspaceRecord::Task(task) = decode_record(&path, &input).expect("decode") else {
            panic!("a task record");
        };
        assert_eq!(task.notes[0].created_at, "2026-10-27T10:00:00.250Z");
        let rendered = WorkspaceRecord::Task(task).render().expect("render");
        let again = decode_record(&path, &rendered)
            .expect("decode rendered")
            .render()
            .expect("render again");
        assert_eq!(again, rendered);
        assert!(matches!(
            decode_record(&path, &input.replace("2026-10-27T15:30:00.25+05:30", "yesterday")),
            Err(WorkspaceError::InvalidRecordNoteCreatedAt { .. })
        ));
    }

    #[test]
    fn task_gates_render_as_an_array_of_tables_and_refuse_invalid_fields() {
        let record = WorkspaceRecord::Task(TaskRecord {
            id: TaskId::parse(TASK_ID).unwrap(),
            spec_id: None,
            plan_id: None,
            phase_id: None,
            parent_id: None,
            plan_key: None,
            title: "Task".to_owned(),
            status: TaskStatus::Pending,
            priority: TaskPriority::Normal,
            position: 0,
            blocked_by: vec![],
            handoff: None,
            evidence: None,
            assignee: None,
            links: vec![],
            notes: vec![],
            gates: vec![TaskGateRecord {
                id: GateId::parse(GATE_ID).unwrap(),
                kind: GateKind::Run,
                target: "https://github.com/o/r/actions/runs/9".to_owned(),
                status: GateStatus::Open,
                created_at: "2026-10-27T10:00:00.000Z".to_owned(),
                met_at: None,
            }],
            body: String::new(),
        });
        let path = record.path();
        let rendered = record.render().expect("render");
        assert!(
            rendered.contains(&format!(
                "[[gates]]\nid = \"{GATE_ID}\"\nkind = \"run\"\ntarget = \"https://github.com/o/r/actions/runs/9\"\n\
                 status = \"open\"\ncreated-at = \"2026-10-27T10:00:00.000Z\"\n"
            )),
            "{rendered}"
        );
        let padded = rendered.replace("runs/9\"", "runs/9/\"");
        assert_eq!(decode_record(&path, &padded).expect("decode"), record);
        for (from, to) in [
            ("actions/runs/9", "pull/9"),
            ("2026-10-27T10:00:00.000Z", " "),
            ("status = \"open\"", "status = \"met\""),
            (
                "status = \"open\"",
                "status = \"open\"\nmet-at = \"2026-10-28T00:00:00.000Z\"",
            ),
        ] {
            assert!(
                matches!(
                    decode_record(&path, &rendered.replace(from, to)),
                    Err(WorkspaceError::InvalidRecordGate { .. })
                ),
                "{to}"
            );
        }
        assert!(matches!(
            decode_record(&path, &rendered.replace("\"run\"", "\"issue\"")),
            Err(WorkspaceError::FrontMatterParse(_))
        ));
    }
}
