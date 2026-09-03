use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use arcl_core::domain::{
    CaptureId, CaptureStatus, ContainerStatus, NoteId, PhaseId, PlanId, ReleaseId, SpecId, TaskId, TaskPriority,
    TaskStatus,
};

use super::{Result, SNAPSHOT_FORMAT_VERSION, SnapshotError};

const FRONT_MATTER_DELIMITER: &str = "+++";

/// The stable kind and directory name of a snapshot record.
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
    /// Return the snapshot directory for this record kind.
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

/// A reference to another snapshot record.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotReference {
    /// The referenced record kind.
    pub kind: String,
    /// The referenced record ID.
    pub id: String,
}

impl SnapshotReference {
    /// Construct a reference with a stable kind and ID.
    pub fn new(kind: impl Into<String>, id: impl Into<String>) -> Self {
        Self { kind: kind.into(), id: id.into() }
    }
}

/// The snapshot manifest's stable format metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotManifest {
    #[serde(rename = "format-version")]
    pub format_version: u32,
}

/// Backwards-friendly short name for the snapshot manifest.
pub type Manifest = SnapshotManifest;

impl Default for SnapshotManifest {
    fn default() -> Self {
        Self { format_version: SNAPSHOT_FORMAT_VERSION }
    }
}

impl SnapshotManifest {
    /// Parse and validate a snapshot manifest.
    pub fn parse(input: &str) -> Result<Self> {
        let manifest = toml::from_str::<Self>(input).map_err(SnapshotError::ManifestParse)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validate that this manifest uses the supported snapshot format.
    pub fn validate(&self) -> Result<()> {
        if self.format_version != SNAPSHOT_FORMAT_VERSION {
            return Err(SnapshotError::UnsupportedVersion {
                found: self.format_version,
                expected: SNAPSHOT_FORMAT_VERSION,
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

/// An inbox capture encoded in a snapshot record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureRecord {
    pub id: CaptureId,
    pub title: String,
    pub status: CaptureStatus,
    pub created_at: String,
    /// The single promotion edge, when the capture has been promoted.
    pub promoted_to: Option<SnapshotReference>,
    pub links: Vec<SnapshotReference>,
    pub body: String,
}

/// A release encoded in a snapshot record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseRecord {
    pub id: ReleaseId,
    pub title: String,
    pub status: ContainerStatus,
    pub body: String,
    /// Explicit members. Descendants are never implied by this list.
    pub members: Vec<SnapshotReference>,
    /// Explicit outgoing record links.
    pub links: Vec<SnapshotReference>,
}

/// An owned specification encoded in a snapshot record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpecRecord {
    pub id: SpecId,
    pub title: String,
    pub status: ContainerStatus,
    pub source_capture_id: Option<CaptureId>,
    pub acceptance_criteria: String,
    pub links: Vec<SnapshotReference>,
    pub body: String,
}

/// A persistent implementation plan encoded in a snapshot record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanRecord {
    pub id: PlanId,
    pub spec_id: SpecId,
    pub title: String,
    pub status: ContainerStatus,
    pub links: Vec<SnapshotReference>,
    pub body: String,
}

/// An optional ordered phase encoded in a snapshot record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhaseRecord {
    pub id: PhaseId,
    pub plan_id: PlanId,
    pub plan_key: Option<String>,
    pub title: String,
    pub status: ContainerStatus,
    pub position: i64,
    pub links: Vec<SnapshotReference>,
    pub body: String,
}

/// A flexible planning task encoded in a snapshot record.
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
    pub links: Vec<SnapshotReference>,
    pub body: String,
}

/// A Markdown note encoded in a snapshot record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoteRecord {
    pub id: NoteId,
    pub title: String,
    pub links: Vec<SnapshotReference>,
    pub body: String,
}

/// A typed record in the versioned workspace projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SnapshotRecord {
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

impl SnapshotRecord {
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

/// Parse the canonical manifest from a snapshot manifest file.
pub fn decode_manifest(input: &str) -> Result<SnapshotManifest> {
    SnapshotManifest::parse(input)
}

/// Encode a validated manifest in canonical TOML form.
pub fn encode_manifest(manifest: &SnapshotManifest) -> Result<String> {
    manifest.render()
}

/// Parse a record, using its path to select and validate its typed kind.
pub fn decode_record(path: &Path, input: &str) -> Result<SnapshotRecord> {
    let (kind, filename_id) = parse_record_path(path)?;
    let (front_matter, body) = split_record_document(input)?;
    let record = match kind {
        RecordKind::Capture => {
            SnapshotRecord::Capture(CaptureRecord::from_wire(parse_front_matter(&front_matter)?, body)?)
        }
        RecordKind::Release => {
            SnapshotRecord::Release(ReleaseRecord::from_wire(parse_front_matter(&front_matter)?, body)?)
        }
        RecordKind::Spec => SnapshotRecord::Spec(SpecRecord::from_wire(parse_front_matter(&front_matter)?, body)?),
        RecordKind::Plan => SnapshotRecord::Plan(PlanRecord::from_wire(parse_front_matter(&front_matter)?, body)?),
        RecordKind::Phase => SnapshotRecord::Phase(PhaseRecord::from_wire(parse_front_matter(&front_matter)?, body)?),
        RecordKind::Task => SnapshotRecord::Task(TaskRecord::from_wire(parse_front_matter(&front_matter)?, body)?),
        RecordKind::Note => SnapshotRecord::Note(NoteRecord::from_wire(parse_front_matter(&front_matter)?, body)?),
    };

    if record.id_string() != filename_id {
        return Err(SnapshotError::FilenameIdMismatch { id: record.id_string(), filename: filename_id });
    }
    Ok(record)
}

/// Encode a record after checking its kind and ID against `path`.
pub fn encode_record(path: &Path, record: &SnapshotRecord) -> Result<String> {
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
    promoted_to: Option<SnapshotReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    links: Vec<SnapshotReference>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct ReleaseFrontMatter {
    id: ReleaseId,
    title: String,
    status: ContainerStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    members: Vec<SnapshotReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    links: Vec<SnapshotReference>,
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
    links: Vec<SnapshotReference>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct PlanFrontMatter {
    id: PlanId,
    spec: SpecId,
    title: String,
    status: ContainerStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    links: Vec<SnapshotReference>,
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
    links: Vec<SnapshotReference>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    links: Vec<SnapshotReference>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct NoteFrontMatter {
    id: NoteId,
    title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    links: Vec<SnapshotReference>,
}

impl CaptureRecord {
    fn from_wire(front_matter: CaptureFrontMatter, body: String) -> Result<Self> {
        validate_title(&front_matter.title)?;
        if front_matter.created_at.trim().is_empty() {
            return Err(SnapshotError::EmptyRecordCreatedAt);
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
            return Err(SnapshotError::EmptyRecordCreatedAt);
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
            links: normalize_references(front_matter.links),
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
            links,
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

fn render_record_document(record: &SnapshotRecord) -> Result<String> {
    let front_matter = match record {
        SnapshotRecord::Capture(record) => render_front_matter(&record.front_matter()?)?,
        SnapshotRecord::Release(record) => render_front_matter(&record.front_matter()?)?,
        SnapshotRecord::Spec(record) => render_front_matter(&record.front_matter()?)?,
        SnapshotRecord::Plan(record) => render_front_matter(&record.front_matter()?)?,
        SnapshotRecord::Phase(record) => render_front_matter(&record.front_matter()?)?,
        SnapshotRecord::Task(record) => render_front_matter(&record.front_matter()?)?,
        SnapshotRecord::Note(record) => render_front_matter(&record.front_matter()?)?,
    };
    let body = match record {
        SnapshotRecord::Capture(record) => normalize_body(&record.body),
        SnapshotRecord::Release(record) => normalize_body(&record.body),
        SnapshotRecord::Spec(record) => normalize_body(&record.body),
        SnapshotRecord::Plan(record) => normalize_body(&record.body),
        SnapshotRecord::Phase(record) => normalize_body(&record.body),
        SnapshotRecord::Task(record) => normalize_body(&record.body),
        SnapshotRecord::Note(record) => normalize_body(&record.body),
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
    toml::from_str(input).map_err(SnapshotError::FrontMatterParse)
}

fn split_record_document(input: &str) -> Result<(String, String)> {
    let normalized = normalize_line_endings(input);
    let mut lines = normalized.split('\n');
    if lines.next() != Some(FRONT_MATTER_DELIMITER) {
        return Err(SnapshotError::InvalidRecordFormat(
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
        SnapshotError::InvalidRecordFormat("front matter has no exact closing `+++` delimiter line".to_owned())
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
        return Err(SnapshotError::InvalidRecordPath { path: path.to_owned() });
    };
    let Some(kind) = RecordKind::from_directory(directory) else {
        return Err(SnapshotError::KindPathMismatch { kind: directory.to_owned(), path: path.to_owned() });
    };
    let Some(filename_id) = filename.strip_suffix(".md") else {
        return Err(SnapshotError::InvalidRecordPath { path: path.to_owned() });
    };
    if filename_id.is_empty() {
        return Err(SnapshotError::InvalidRecordPath { path: path.to_owned() });
    }
    Ok((kind, filename_id.to_owned()))
}

fn validate_record_path(path: &Path, kind: RecordKind, id: &str) -> Result<()> {
    let (path_kind, filename_id) = parse_record_path(path)?;
    if path_kind != kind {
        return Err(SnapshotError::KindPathMismatch { kind: kind.as_str().to_owned(), path: path.to_owned() });
    }
    if filename_id != id {
        return Err(SnapshotError::FilenameIdMismatch { id: id.to_owned(), filename: filename_id });
    }
    Ok(())
}

fn validate_title(title: &str) -> Result<()> {
    if title.trim().is_empty() { Err(SnapshotError::EmptyRecordTitle) } else { Ok(()) }
}

fn validate_position(position: i64) -> Result<()> {
    if position < 0 { Err(SnapshotError::InvalidRecordPosition { position }) } else { Ok(()) }
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

fn normalize_references(mut references: Vec<SnapshotReference>) -> Vec<SnapshotReference> {
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

    #[test]
    fn manifest_is_version_two_and_canonical() {
        let manifest = SnapshotManifest::parse("format-version = 2").expect("manifest");
        assert_eq!(manifest.render().expect("render"), "format-version = 2\n");
        assert!(matches!(
            SnapshotManifest::parse("format-version = 1"),
            Err(SnapshotError::UnsupportedVersion { .. })
        ));
    }

    #[test]
    fn all_new_record_kinds_round_trip() {
        let records = [
            SnapshotRecord::Capture(CaptureRecord {
                id: CaptureId::parse(CAPTURE_ID).unwrap(),
                title: "Capture".to_owned(),
                status: CaptureStatus::Captured,
                created_at: "2025-01-01T00:00:00.000Z".to_owned(),
                promoted_to: None,
                links: vec![],
                body: "Capture **body**".to_owned(),
            }),
            SnapshotRecord::Spec(SpecRecord {
                id: SpecId::parse(SPEC_ID).unwrap(),
                title: "Spec".to_owned(),
                status: ContainerStatus::Open,
                source_capture_id: Some(CaptureId::parse(CAPTURE_ID).unwrap()),
                acceptance_criteria: "- [ ] criterion".to_owned(),
                links: vec![],
                body: "# Spec".to_owned(),
            }),
            SnapshotRecord::Plan(PlanRecord {
                id: PlanId::parse(PLAN_ID).unwrap(),
                spec_id: SpecId::parse(SPEC_ID).unwrap(),
                title: "Plan".to_owned(),
                status: ContainerStatus::Open,
                links: vec![],
                body: "Plan body".to_owned(),
            }),
            SnapshotRecord::Phase(PhaseRecord {
                id: PhaseId::parse(PHASE_ID).unwrap(),
                plan_id: PlanId::parse(PLAN_ID).unwrap(),
                plan_key: Some("build".to_owned()),
                title: "Build".to_owned(),
                status: ContainerStatus::Open,
                position: 0,
                links: vec![],
                body: "Phase body".to_owned(),
            }),
            SnapshotRecord::Task(TaskRecord {
                id: TaskId::parse(TASK_ID).unwrap(),
                spec_id: Some(SpecId::parse(SPEC_ID).unwrap()),
                plan_id: Some(PlanId::parse(PLAN_ID).unwrap()),
                phase_id: Some(PhaseId::parse(PHASE_ID).unwrap()),
                parent_id: None,
                plan_key: Some("build/task".to_owned()),
                title: "Task".to_owned(),
                status: TaskStatus::Pending,
                priority: TaskPriority::High,
                position: 0,
                blocked_by: vec![TaskId::parse(OTHER_TASK_ID).unwrap()],
                handoff: Some("Resume here".to_owned()),
                evidence: Some("Evidence".to_owned()),
                links: vec![SnapshotReference::new("spec", SPEC_ID)],
                body: "Task body".to_owned(),
            }),
            SnapshotRecord::Note(NoteRecord {
                id: NoteId::parse(NOTE_ID).unwrap(),
                title: "Note".to_owned(),
                links: vec![SnapshotReference::new("task", TASK_ID)],
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
        let record = SnapshotRecord::Task(TaskRecord {
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
            links: vec![],
            body: String::new(),
        });
        let rendered = record.render().expect("render");
        assert!(rendered.contains(&format!("blocked-by = [\"{TASK_ID}\", \"{OTHER_TASK_ID}\"]")));
        assert!(!rendered.contains("plan-key"));
        assert!(!rendered.contains("handoff"));
    }
}
