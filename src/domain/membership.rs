use serde::{Deserialize, Serialize};

use super::{NoteId, PlanId, ProjectId, ReleaseId, SpecId, TaskId};

/// Record kinds that can belong to a release.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseMemberKind {
    Spec,
    Plan,
    Task,
    Note,
}

impl ReleaseMemberKind {
    /// Return the stable value stored in SQLite.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Spec => "spec",
            Self::Plan => "plan",
            Self::Task => "task",
            Self::Note => "note",
        }
    }
}

/// One explicit release membership edge.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReleaseMembership {
    pub project_id: ProjectId,
    pub release_id: ReleaseId,
    pub record_kind: ReleaseMemberKind,
    pub record_id: String,
}

impl ReleaseMembership {
    /// Return the typed ID represented by this membership when its kind agrees.
    pub fn typed_id(&self) -> Option<ReleaseMemberId> {
        match self.record_kind {
            ReleaseMemberKind::Spec => SpecId::parse(&self.record_id).ok().map(ReleaseMemberId::Spec),
            ReleaseMemberKind::Plan => PlanId::parse(&self.record_id).ok().map(ReleaseMemberId::Plan),
            ReleaseMemberKind::Task => TaskId::parse(&self.record_id).ok().map(ReleaseMemberId::Task),
            ReleaseMemberKind::Note => NoteId::parse(&self.record_id).ok().map(ReleaseMemberId::Note),
        }
    }
}

/// A typed target of a release membership edge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseMemberId {
    Spec(SpecId),
    Plan(PlanId),
    Task(TaskId),
    Note(NoteId),
}

/// Record kinds that can be referenced by a note.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkedRecordKind {
    Capture,
    Spec,
    Plan,
    Phase,
    Task,
    Note,
    Release,
}

impl LinkedRecordKind {
    /// Return the stable value stored in SQLite.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Capture => "capture",
            Self::Spec => "spec",
            Self::Plan => "plan",
            Self::Phase => "phase",
            Self::Task => "task",
            Self::Note => "note",
            Self::Release => "release",
        }
    }
}

/// One explicit note-to-record relationship.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NoteLink {
    pub project_id: ProjectId,
    pub note_id: NoteId,
    pub record_kind: LinkedRecordKind,
    pub record_id: String,
}

/// A generic relationship between two records in one project.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RecordLink {
    pub project_id: ProjectId,
    pub source_kind: LinkedRecordKind,
    pub source_id: String,
    pub target_kind: LinkedRecordKind,
    pub target_id: String,
}

/// A valid target for capture promotion.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum CapturePromotionTarget {
    Spec(SpecId),
    Task(TaskId),
    Note(NoteId),
}

impl CapturePromotionTarget {
    /// Return the stable promotion kind for this destination.
    pub(crate) const fn promotion_kind(self) -> &'static str {
        match self {
            Self::Spec(_) => "spec",
            Self::Task(_) => "task",
            Self::Note(_) => "note",
        }
    }

    /// Return the destination identifier in its persisted form.
    pub(crate) fn promotion_target_id(self) -> String {
        match self {
            Self::Spec(id) => id.to_string(),
            Self::Task(id) => id.to_string(),
            Self::Note(id) => id.to_string(),
        }
    }
}

/// Provenance preserved when a capture becomes a planning record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CapturePromotion {
    pub project_id: ProjectId,
    pub capture_id: super::CaptureId,
    pub target: CapturePromotionTarget,
}
