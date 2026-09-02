use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::domain::{
    ContainerStatus, EpicId, IdeaId, IdeaStatus, MilestoneId, ReleaseId, TaskId, TaskPriority, TaskStatus,
};

use super::{Result, SNAPSHOT_FORMAT_VERSION, SnapshotError};

const FRONT_MATTER_DELIMITER: &str = "+++";

/// The stable kind and directory name of a snapshot record.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RecordKind {
    /// An idea in the project inbox.
    Idea,
    /// A release grouping epics.
    Release,
    /// An epic linked to a Markdown specification.
    Epic,
    /// An ordered stage belonging to an epic.
    Milestone,
    /// A task or subtask belonging to a milestone.
    Task,
}

impl RecordKind {
    /// Return the snapshot directory for this record kind.
    pub const fn directory(self) -> &'static str {
        match self {
            Self::Idea => "ideas",
            Self::Release => "releases",
            Self::Epic => "epics",
            Self::Milestone => "milestones",
            Self::Task => "tasks",
        }
    }

    /// Return the human-readable kind name used in validation errors.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Idea => "idea",
            Self::Release => "release",
            Self::Epic => "epic",
            Self::Milestone => "milestone",
            Self::Task => "task",
        }
    }

    fn from_directory(directory: &str) -> Option<Self> {
        match directory {
            "ideas" => Some(Self::Idea),
            "releases" => Some(Self::Release),
            "epics" => Some(Self::Epic),
            "milestones" => Some(Self::Milestone),
            "tasks" => Some(Self::Task),
            _ => None,
        }
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

/// An idea encoded in a snapshot record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdeaRecord {
    pub id: IdeaId,
    pub title: String,
    pub status: IdeaStatus,
    pub promoted_to: Option<EpicId>,
    pub description: String,
}

/// A release encoded in a snapshot record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseRecord {
    pub id: ReleaseId,
    pub title: String,
    pub status: ContainerStatus,
    pub description: String,
}

/// An epic encoded in a snapshot record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EpicRecord {
    pub id: EpicId,
    pub title: String,
    pub status: ContainerStatus,
    pub spec_path: String,
    pub release: Option<ReleaseId>,
    pub source_idea: Option<IdeaId>,
    pub description: String,
}

/// A milestone encoded in a snapshot record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneRecord {
    pub id: MilestoneId,
    pub title: String,
    pub status: ContainerStatus,
    pub epic: EpicId,
    pub position: i64,
    pub plan_key: Option<String>,
    pub description: String,
}

/// A task or subtask encoded in a snapshot record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskRecord {
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
    pub description: String,
}

/// A typed record in the version-controlled snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SnapshotRecord {
    /// An idea record.
    Idea(IdeaRecord),
    /// A release record.
    Release(ReleaseRecord),
    /// An epic record.
    Epic(EpicRecord),
    /// A milestone record.
    Milestone(MilestoneRecord),
    /// A task or subtask record.
    Task(TaskRecord),
}

impl SnapshotRecord {
    /// Return the record's typed kind.
    pub const fn kind(&self) -> RecordKind {
        match self {
            Self::Idea(_) => RecordKind::Idea,
            Self::Release(_) => RecordKind::Release,
            Self::Epic(_) => RecordKind::Epic,
            Self::Milestone(_) => RecordKind::Milestone,
            Self::Task(_) => RecordKind::Task,
        }
    }

    /// Return the canonical ID string used by the record filename.
    pub fn id_string(&self) -> String {
        match self {
            Self::Idea(record) => record.id.to_string(),
            Self::Release(record) => record.id.to_string(),
            Self::Epic(record) => record.id.to_string(),
            Self::Milestone(record) => record.id.to_string(),
            Self::Task(record) => record.id.to_string(),
        }
    }

    /// Return the canonical worktree-relative path for this record.
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
    let (front_matter, description) = split_record_document(input)?;

    let record = match kind {
        RecordKind::Idea => {
            SnapshotRecord::Idea(IdeaRecord::from_wire(parse_front_matter(&front_matter)?, description)?)
        }
        RecordKind::Release => SnapshotRecord::Release(ReleaseRecord::from_wire(
            parse_front_matter(&front_matter)?,
            description,
        )?),
        RecordKind::Epic => {
            SnapshotRecord::Epic(EpicRecord::from_wire(parse_front_matter(&front_matter)?, description)?)
        }
        RecordKind::Milestone => SnapshotRecord::Milestone(MilestoneRecord::from_wire(
            parse_front_matter(&front_matter)?,
            description,
        )?),
        RecordKind::Task => {
            SnapshotRecord::Task(TaskRecord::from_wire(parse_front_matter(&front_matter)?, description)?)
        }
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
struct IdeaFrontMatter {
    id: IdeaId,
    title: String,
    status: IdeaStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    promoted_to: Option<EpicId>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct ReleaseFrontMatter {
    id: ReleaseId,
    title: String,
    status: ContainerStatus,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct EpicFrontMatter {
    id: EpicId,
    title: String,
    status: ContainerStatus,
    spec_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    release: Option<ReleaseId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_idea: Option<IdeaId>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct MilestoneFrontMatter {
    id: MilestoneId,
    title: String,
    status: ContainerStatus,
    epic: EpicId,
    position: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan_key: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct TaskFrontMatter {
    id: TaskId,
    title: String,
    status: TaskStatus,
    priority: TaskPriority,
    milestone: MilestoneId,
    position: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent: Option<TaskId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan_key: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    blocked_by: Vec<TaskId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    handoff: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    evidence: Option<String>,
}

impl IdeaRecord {
    fn from_wire(front_matter: IdeaFrontMatter, description: String) -> Result<Self> {
        validate_title(&front_matter.title)?;
        Ok(Self {
            id: front_matter.id,
            title: front_matter.title,
            status: front_matter.status,
            promoted_to: front_matter.promoted_to,
            description,
        })
    }

    fn front_matter(&self) -> Result<IdeaFrontMatter> {
        validate_title(&self.title)?;
        Ok(IdeaFrontMatter {
            id: self.id,
            title: self.title.clone(),
            status: self.status,
            promoted_to: self.promoted_to,
        })
    }
}

impl ReleaseRecord {
    fn from_wire(front_matter: ReleaseFrontMatter, description: String) -> Result<Self> {
        validate_title(&front_matter.title)?;
        Ok(Self { id: front_matter.id, title: front_matter.title, status: front_matter.status, description })
    }

    fn front_matter(&self) -> Result<ReleaseFrontMatter> {
        validate_title(&self.title)?;
        Ok(ReleaseFrontMatter { id: self.id, title: self.title.clone(), status: self.status })
    }
}

impl EpicRecord {
    fn from_wire(front_matter: EpicFrontMatter, description: String) -> Result<Self> {
        validate_title(&front_matter.title)?;
        if front_matter.spec_path.is_empty() {
            return Err(SnapshotError::EmptyRecordSpecPath);
        }
        Ok(Self {
            id: front_matter.id,
            title: front_matter.title,
            status: front_matter.status,
            spec_path: front_matter.spec_path,
            release: front_matter.release,
            source_idea: front_matter.source_idea,
            description,
        })
    }

    fn front_matter(&self) -> Result<EpicFrontMatter> {
        validate_title(&self.title)?;
        if self.spec_path.is_empty() {
            return Err(SnapshotError::EmptyRecordSpecPath);
        }
        Ok(EpicFrontMatter {
            id: self.id,
            title: self.title.clone(),
            status: self.status,
            spec_path: self.spec_path.clone(),
            release: self.release,
            source_idea: self.source_idea,
        })
    }
}

impl MilestoneRecord {
    fn from_wire(front_matter: MilestoneFrontMatter, description: String) -> Result<Self> {
        validate_title(&front_matter.title)?;
        validate_position(front_matter.position)?;
        Ok(Self {
            id: front_matter.id,
            title: front_matter.title,
            status: front_matter.status,
            epic: front_matter.epic,
            position: front_matter.position,
            plan_key: normalize_optional(front_matter.plan_key),
            description,
        })
    }

    fn front_matter(&self) -> Result<MilestoneFrontMatter> {
        validate_title(&self.title)?;
        validate_position(self.position)?;
        Ok(MilestoneFrontMatter {
            id: self.id,
            title: self.title.clone(),
            status: self.status,
            epic: self.epic,
            position: self.position,
            plan_key: normalize_optional(self.plan_key.clone()),
        })
    }
}

impl TaskRecord {
    fn from_wire(front_matter: TaskFrontMatter, description: String) -> Result<Self> {
        validate_title(&front_matter.title)?;
        validate_position(front_matter.position)?;
        Ok(Self {
            id: front_matter.id,
            title: front_matter.title,
            status: front_matter.status,
            priority: front_matter.priority,
            milestone: front_matter.milestone,
            position: front_matter.position,
            parent: front_matter.parent,
            plan_key: normalize_optional(front_matter.plan_key),
            blocked_by: front_matter.blocked_by,
            handoff: normalize_optional_markdown(front_matter.handoff),
            evidence: normalize_optional_markdown(front_matter.evidence),
            description,
        })
    }

    fn front_matter(&self) -> Result<TaskFrontMatter> {
        validate_title(&self.title)?;
        validate_position(self.position)?;
        let mut blocked_by = self.blocked_by.clone();
        blocked_by.sort();
        Ok(TaskFrontMatter {
            id: self.id,
            title: self.title.clone(),
            status: self.status,
            priority: self.priority,
            milestone: self.milestone,
            position: self.position,
            parent: self.parent,
            plan_key: normalize_optional(self.plan_key.clone()),
            blocked_by,
            handoff: normalize_optional_markdown(self.handoff.clone()),
            evidence: normalize_optional_markdown(self.evidence.clone()),
        })
    }
}

fn render_record_document(record: &SnapshotRecord) -> Result<String> {
    let front_matter = match record {
        SnapshotRecord::Idea(record) => render_front_matter(&record.front_matter()?)?,
        SnapshotRecord::Release(record) => render_front_matter(&record.front_matter()?)?,
        SnapshotRecord::Epic(record) => render_front_matter(&record.front_matter()?)?,
        SnapshotRecord::Milestone(record) => render_front_matter(&record.front_matter()?)?,
        SnapshotRecord::Task(record) => render_front_matter(&record.front_matter()?)?,
    };
    let description = match record {
        SnapshotRecord::Idea(record) => normalize_body(&record.description),
        SnapshotRecord::Release(record) => normalize_body(&record.description),
        SnapshotRecord::Epic(record) => normalize_body(&record.description),
        SnapshotRecord::Milestone(record) => normalize_body(&record.description),
        SnapshotRecord::Task(record) => normalize_body(&record.description),
    };

    if description.is_empty() {
        Ok(format!(
            "{FRONT_MATTER_DELIMITER}\n{front_matter}{FRONT_MATTER_DELIMITER}\n"
        ))
    } else {
        Ok(format!(
            "{FRONT_MATTER_DELIMITER}\n{front_matter}{FRONT_MATTER_DELIMITER}\n\n{description}\n"
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
    let body = normalize_body(&body_lines.join("\n"));
    Ok((front_matter_lines.join("\n"), body))
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

#[cfg(test)]
mod tests {
    use super::*;

    const IDEA_ID: &str = "arcl-i-01K0B3N4QSC9R7K6W8X2M5YH1Z";
    const RELEASE_ID: &str = "arcl-r-01K0B2ZWTX7JX9PH7W5G1S6A9Q";
    const EPIC_ID: &str = "arcl-e-01K0B2ZWTX7JX9PH7W5G1S6A9Q";
    const TASK_ID: &str = "arcl-t-01K0B31M6VGK4YH8VKT4C0D2DR";
    const OTHER_TASK_ID: &str = "arcl-t-01K0B31M6VGK4YH8VKT4C0D2DS";
    const MILESTONE_ID: &str = "arcl-m-01K0B2ZWTX7JX9PH7W5G1S6A9Q";

    #[test]
    fn manifest_is_exact_and_accepts_missing_final_newline() {
        let manifest = SnapshotManifest::parse("format-version = 1").expect("manifest");
        assert_eq!(manifest.render().expect("render"), "format-version = 1\n");
    }

    #[test]
    fn manifest_rejects_unknown_fields_and_versions() {
        assert!(matches!(
            SnapshotManifest::parse("format-version = 1\nother = true"),
            Err(SnapshotError::ManifestParse(_))
        ));
        assert!(matches!(
            SnapshotManifest::parse("format-version = 2"),
            Err(SnapshotError::UnsupportedVersion { .. })
        ));
    }

    #[test]
    fn release_round_trips_through_canonical_codec() {
        let record = SnapshotRecord::Release(ReleaseRecord {
            id: ReleaseId::parse(RELEASE_ID).expect("release ID"),
            title: "First release".to_owned(),
            status: ContainerStatus::Open,
            description: "Ship the first release.".to_owned(),
        });
        let path = Path::new("releases").join(format!("{RELEASE_ID}.md"));
        let rendered = encode_record(&path, &record).expect("encode release");
        let decoded = decode_record(&path, &rendered).expect("decode release");

        assert_eq!(decoded, record);
        assert_eq!(encode_record(&path, &decoded).expect("re-encode release"), rendered);
    }

    #[test]
    fn epic_round_trip_preserves_optional_relationships() {
        let record = SnapshotRecord::Epic(EpicRecord {
            id: EpicId::parse(EPIC_ID).expect("epic ID"),
            title: "Snapshot codec".to_owned(),
            status: ContainerStatus::Open,
            spec_path: "docs/snapshot.md".to_owned(),
            release: Some(ReleaseId::parse(RELEASE_ID).expect("release ID")),
            source_idea: Some(IdeaId::parse(IDEA_ID).expect("idea ID")),
            description: "Define the snapshot format.".to_owned(),
        });
        let path = Path::new("epics").join(format!("{EPIC_ID}.md"));
        let rendered = encode_record(&path, &record).expect("encode epic");
        let decoded = decode_record(&path, &rendered).expect("decode epic");

        assert!(rendered.contains(&format!("release = \"{RELEASE_ID}\"")));
        assert!(rendered.contains(&format!("source-idea = \"{IDEA_ID}\"")));
        assert_eq!(decoded, record);
        assert_eq!(encode_record(&path, &decoded).expect("re-encode epic"), rendered);
    }

    #[test]
    fn milestone_round_trip_preserves_optional_plan_key() {
        let record = SnapshotRecord::Milestone(MilestoneRecord {
            id: MilestoneId::parse(MILESTONE_ID).expect("milestone ID"),
            title: "Codec coverage".to_owned(),
            status: ContainerStatus::Open,
            epic: EpicId::parse(EPIC_ID).expect("epic ID"),
            position: 10,
            plan_key: Some("snapshot-codec".to_owned()),
            description: "Cover every snapshot record kind.".to_owned(),
        });
        let path = Path::new("milestones").join(format!("{MILESTONE_ID}.md"));
        let rendered = encode_record(&path, &record).expect("encode milestone");
        let decoded = decode_record(&path, &rendered).expect("decode milestone");

        assert!(rendered.contains("plan-key = \"snapshot-codec\""));
        assert_eq!(decoded, record);
        assert_eq!(encode_record(&path, &decoded).expect("re-encode milestone"), rendered);
    }

    #[test]
    fn task_front_matter_is_strict_sorted_and_omits_empty_options() {
        let record = SnapshotRecord::Task(TaskRecord {
            id: TaskId::parse(TASK_ID).expect("task ID"),
            title: "Ready work".to_owned(),
            status: TaskStatus::Pending,
            priority: TaskPriority::High,
            milestone: MilestoneId::parse(MILESTONE_ID).expect("milestone ID"),
            position: 20,
            parent: None,
            plan_key: Some(String::new()),
            blocked_by: vec![
                TaskId::parse(OTHER_TASK_ID).expect("other task ID"),
                TaskId::parse(TASK_ID).expect("task ID"),
            ],
            handoff: Some(String::new()),
            evidence: None,
            description: "Return actionable tasks.\r\n\r\n".to_owned(),
        });
        let rendered =
            encode_record(Path::new("tasks").join(format!("{TASK_ID}.md")).as_path(), &record).expect("record");
        assert_eq!(
            rendered,
            format!(
                "+++\nid = \"{TASK_ID}\"\ntitle = \"Ready work\"\nstatus = \"pending\"\npriority = \"high\"\nmilestone = \"{MILESTONE_ID}\"\nposition = 20\nblocked-by = [\"{TASK_ID}\", \"{OTHER_TASK_ID}\"]\n+++\n\nReturn actionable tasks.\n"
            )
        );
        assert!(!rendered.contains("plan-key"));
        assert!(!rendered.contains("handoff"));
        assert!(!rendered.contains('\r'));
    }

    #[test]
    fn task_round_trip_preserves_handoff_and_evidence() {
        let record = SnapshotRecord::Task(TaskRecord {
            id: TaskId::parse(TASK_ID).expect("task ID"),
            title: "Ready work".to_owned(),
            status: TaskStatus::Pending,
            priority: TaskPriority::High,
            milestone: MilestoneId::parse(MILESTONE_ID).expect("milestone ID"),
            position: 20,
            parent: None,
            plan_key: None,
            blocked_by: Vec::new(),
            handoff: Some("Continue from the validated graph.".to_owned()),
            evidence: Some("The focused snapshot tests pass.".to_owned()),
            description: "Return actionable tasks.".to_owned(),
        });
        let path = Path::new("tasks").join(format!("{TASK_ID}.md"));

        let rendered = encode_record(&path, &record).expect("encode task");
        let decoded = decode_record(&path, &rendered).expect("decode task");

        assert!(rendered.contains("handoff = \"Continue from the validated graph.\""));
        assert!(rendered.contains("evidence = \"The focused snapshot tests pass.\""));
        assert_eq!(decoded, record);
    }

    #[test]
    fn reader_normalizes_body_and_requires_exact_delimiters() {
        let input =
            format!("+++\r\nid = \"{IDEA_ID}\"\r\ntitle = \"Idea\"\r\nstatus = \"captured\"\r\n+++\r\n\r\nbody\r\n");
        let record = decode_record(Path::new("ideas").join(format!("{IDEA_ID}.md")).as_path(), &input).expect("record");
        let SnapshotRecord::Idea(record) = record else { panic!("expected idea") };
        assert_eq!(record.description, "body");

        let malformed = input.replacen("+++\r\n", " +++\r\n", 1);
        assert!(matches!(
            decode_record(Path::new("ideas").join(format!("{IDEA_ID}.md")).as_path(), &malformed),
            Err(SnapshotError::InvalidRecordFormat(_))
        ));
    }

    #[test]
    fn path_and_typed_id_must_agree() {
        let input = format!("+++\nid = \"{IDEA_ID}\"\ntitle = \"Idea\"\nstatus = \"captured\"\n+++\n");
        assert!(matches!(
            decode_record(Path::new("epics").join(format!("{IDEA_ID}.md")).as_path(), &input),
            Err(SnapshotError::FrontMatterParse(_))
        ));

        let mismatched_path = Path::new("ideas").join(format!("{EPIC_ID}.md"));
        assert!(matches!(
            decode_record(&mismatched_path, &input),
            Err(SnapshotError::FilenameIdMismatch { .. })
        ));
    }
}
