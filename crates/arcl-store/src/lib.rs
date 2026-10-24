//! SQLite persistence and transactional operations for Arc Lightning.

mod storage;

pub use storage::*;

/// The storage types other crates import: [`Database`], its errors, and the
/// inputs and results of its operations.
///
/// `use arcl_store::prelude::*;` brings them in. The `Result` alias and the
/// free functions on a raw connection stay out, so the glob does not shadow
/// `std::result::Result` or bring in names such as `plan` and `check`. Domain
/// items come from `arcl_core::prelude`.
pub mod prelude {
    pub use crate::{
        CURRENT_VERSION, CapturePromotionInput, CapturePromotionRecord, CapturePromotionResult, CaptureTaskPromotion,
        CaptureUpdate, CheckReport, ConnectedGraph, DEFAULT_PROJECT_ID, DataVersion, Database, NoteUpdate,
        OpenSubtasks, PhaseUpdate, PlanApplyResult, PlanChange, PlanDependencyDiff, PlanDiff, PlanDiffItem, PlanUpdate,
        PlanningBlockerView, PlanningCompletion, PlanningContext, PlanningReadiness, PlanningReadyFilter,
        PlanningTaskCreate, PlanningTaskUpdate, PlanningTaskView, ReadinessReason, ReadinessReasonKind, SpecUpdate,
        StorageError, WorkspaceBaseFile,
    };
}
