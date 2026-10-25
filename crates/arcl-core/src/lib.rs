//! Domain records and transport-independent application inputs for Arc Lightning.

pub mod domain;
pub mod plan;

/// The items other crates import: every domain record, ID, status and check,
/// and the structured plan types.
///
/// `use arcl_core::prelude::*;` brings them in. The plan parser stays out, so
/// a caller names it as `plan::parse`.
pub mod prelude {
    pub use crate::domain::*;
    pub use crate::plan::{
        PLAN_FORMAT_VERSION, PlanDocument, PlanEntries, PlanError, PlanPhase, PlanPhaseEntry, PlanTask, PlanTaskEntry,
    };
}
