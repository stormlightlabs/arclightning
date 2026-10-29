mod connected;
mod migrations;
mod releases;

pub use connected::*;

use std::{collections::HashMap, path::Path, time::Duration};

use rusqlite::{Connection, TransactionBehavior};
use thiserror::Error;

use arcl_core::domain::*;

/// The newest SQLite schema version understood by the application.
pub const CURRENT_VERSION: i32 = 8;

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

pub type Result<T> = std::result::Result<T, StorageError>;

/// One exact file stored as the last successful workspace base.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceBaseFile {
    /// The path relative to the workspace root.
    pub path: String,
    /// The exact bytes observed after the successful export.
    pub content: Vec<u8>,
}

/// Infrastructure failures from the SQLite storage boundary.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("stored project is invalid: {0}")]
    InvalidProject(DomainError),
    #[error("stored capture is invalid: {0}")]
    InvalidCapture(DomainError),
    #[error("stored spec is invalid: {0}")]
    InvalidSpec(DomainError),
    #[error("stored plan is invalid: {0}")]
    InvalidPlan(DomainError),
    #[error("stored phase is invalid: {0}")]
    InvalidPhase(DomainError),
    #[error("stored planning task is invalid: {0}")]
    InvalidPlanningTask(DomainError),
    #[error("stored note is invalid: {0}")]
    InvalidNote(DomainError),
    #[error("stored release membership is invalid: {0}")]
    InvalidMembership(DomainError),
    #[error("stored record link is invalid: {0}")]
    InvalidLink(DomainError),
    #[error("project was not found")]
    ProjectNotFound,
    #[error("capture `{id}` was not found")]
    CaptureNotFound { id: String },
    #[error("spec `{id}` was not found")]
    SpecNotFound { id: String },
    #[error("plan `{id}` was not found")]
    PlanNotFound { id: String },
    #[error("phase `{id}` was not found")]
    PhaseNotFound { id: String },
    #[error("planning task `{id}` was not found")]
    PlanningTaskNotFound { id: String },
    #[error("note `{id}` was not found")]
    NoteNotFound { id: String },
    #[error("release membership `{release_id}` -> `{record_id}` was not found")]
    ReleaseMembershipNotFound { release_id: String, record_id: String },
    #[error("note link `{note}` -> `{record}` was not found")]
    NoteLinkNotFound { note: String, record: String },
    #[error("record link `{source_id}` -> `{target_id}` was not found")]
    RecordLinkNotFound { source_id: String, target_id: String },
    #[error("stored release is invalid: {0}")]
    InvalidRelease(DomainError),
    #[error("stored connected task dependency is invalid: {0}")]
    InvalidPlanningDependency(DomainError),
    #[error("release `{id}` was not found")]
    ReleaseNotFound { id: String },
    #[error("connected dependency from task `{task}` to blocker `{blocker}` was not found")]
    PlanningDependencyNotFound { task: String, blocker: String },
    #[error("capture `{id}` cannot be promoted from status `{status}`")]
    CaptureNotPromotable { id: String, status: String },
    #[error("capture `{id}` has an inconsistent promotion relationship")]
    InconsistentCapturePromotion { id: String },
    #[error("capture `{capture}` is already promoted to {existing}; cannot promote it to {requested}")]
    AmbiguousCapturePromotion {
        capture: String,
        existing: &'static str,
        requested: &'static str,
    },
    #[error("structured plan input is invalid: {0}")]
    InvalidPlanInput(#[from] arcl_core::plan::PlanError),
    #[error("database user_version {found} is newer than this application supports (latest {latest})")]
    NewerDatabase { found: i32, latest: i32 },
    #[error("migration sequence is missing version {expected} before version {found}")]
    MigrationGap { expected: i32, found: i32 },
    /// Another connection committed after this command read the database, so
    /// replacing the graph would discard that write.
    #[error("the database changed after this command read it; run the command again")]
    ChangedSinceRead,
    /// A claim on a task that another assignee holds.
    #[error("task `{id}` is claimed by `{assignee}`")]
    TaskClaimed { id: String, assignee: String },
    /// A claim whose name is empty, longer than 64 characters, or holds a
    /// control or format character. The name comes from the request, not
    /// from stored data.
    #[error(
        "claim name `{}` is invalid; use a non-empty name of at most 64 characters without control or format characters",
        value.escape_debug()
    )]
    InvalidClaimName { value: String },
    /// Task note text that [`validate_task_note_text`] refuses. The text
    /// comes from the request, not from stored data.
    ///
    /// [`validate_task_note_text`]: arcl_core::domain::validate_task_note_text
    #[error(transparent)]
    InvalidTaskNoteText(DomainError),
    /// A blocker list that names one task twice. The list comes from the
    /// request, not from stored data.
    #[error(transparent)]
    RepeatedBlocker(DomainError),
    /// A short ID argument names no record, more than one, or is not a
    /// short ID. No match exits 5; the rest exit 3.
    #[error(transparent)]
    ShortId(#[from] ShortIdError),
    /// A gate with an invalid kind, target or status.
    #[error("gate is invalid: {0}")]
    InvalidGate(DomainError),
    /// No gate has the ID.
    #[error("gate `{id}` was not found")]
    GateNotFound { id: String },
    /// A gate added to a completed or cancelled task.
    #[error("cannot add a gate to task `{id}` while it is `{status}`")]
    GateOnClosedTask { id: String, status: String },
}

impl StorageError {
    /// Return the exit category adapters report for this failure: `3` for an
    /// invalid project or request, `4` for a conflict, `5` for a missing
    /// record, and `1` for any other failure.
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::ProjectNotFound => 3,
            Self::ReleaseNotFound { .. }
            | Self::CaptureNotFound { .. }
            | Self::SpecNotFound { .. }
            | Self::PlanNotFound { .. }
            | Self::PhaseNotFound { .. }
            | Self::PlanningTaskNotFound { .. }
            | Self::NoteNotFound { .. }
            | Self::ReleaseMembershipNotFound { .. }
            | Self::NoteLinkNotFound { .. }
            | Self::RecordLinkNotFound { .. }
            | Self::PlanningDependencyNotFound { .. }
            | Self::GateNotFound { .. } => 5,
            Self::InvalidProject(_)
            | Self::InvalidCapture(_)
            | Self::InvalidSpec(_)
            | Self::InvalidPlan(_)
            | Self::InvalidPhase(_)
            | Self::InvalidPlanningTask(_)
            | Self::InvalidNote(_)
            | Self::InvalidMembership(_)
            | Self::InvalidLink(_)
            | Self::InvalidPlanningDependency(_)
            | Self::InvalidRelease(_)
            | Self::CaptureNotPromotable { .. }
            | Self::AmbiguousCapturePromotion { .. }
            | Self::InconsistentCapturePromotion { .. }
            | Self::InvalidPlanInput(_)
            | Self::InvalidClaimName { .. }
            | Self::InvalidTaskNoteText(_)
            | Self::RepeatedBlocker(_)
            | Self::InvalidGate(_)
            | Self::GateOnClosedTask { .. }
            | Self::NewerDatabase { .. }
            | Self::MigrationGap { .. } => 3,
            Self::ShortId(error) => {
                if error.is_not_found() {
                    5
                } else {
                    3
                }
            }
            Self::ChangedSinceRead | Self::TaskClaimed { .. } => 4,
            Self::Sqlite(_) => 1,
        }
    }
}

/// The database's data version as one connection observed it.
///
/// Read it with [`Database::data_version`] before reading the state a later
/// replacement is built from. The value changes when another connection
/// commits; commits on the same connection leave it unchanged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DataVersion(i64);

/// The single SQLite connection owner for one application command.
#[derive(Debug)]
pub struct Database {
    connection: Connection,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let mut connection = Connection::open(path)?;
        configure(&mut connection)?;
        migrations::apply(&mut connection)?;
        Ok(Self { connection })
    }

    pub fn open_in_memory() -> Result<Self> {
        let mut connection = Connection::open_in_memory()?;
        configure(&mut connection)?;
        migrations::apply(&mut connection)?;
        Ok(Self { connection })
    }

    pub fn schema_version(&self) -> Result<i32> {
        Ok(self
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    /// Read the data version that guards a later graph replacement.
    pub fn data_version(&self) -> Result<DataVersion> {
        Ok(DataVersion(self.connection.pragma_query_value(
            None,
            "data_version",
            |row| row.get(0),
        )?))
    }

    /// Read the exact files recorded by the last successful workspace export.
    pub fn workspace_base(&self) -> Result<Vec<WorkspaceBaseFile>> {
        let mut statement = self
            .connection
            .prepare("SELECT path, content FROM workspace_files ORDER BY path")?;
        let rows = statement.query_map([], |row| {
            Ok(WorkspaceBaseFile { path: row.get(0)?, content: row.get(1)? })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Replace the connected graph and its workspace base in one transaction.
    ///
    /// The repository adapter validates the candidate graph before calling this
    /// method. Records are inserted in dependency order, and task parents are
    /// attached only after every task exists so file order cannot affect import.
    ///
    /// `read` is the [`DataVersion`] taken before the caller read the state the
    /// graph was built from. The transaction takes the write lock first and
    /// returns [`StorageError::ChangedSinceRead`] without writing if another
    /// connection committed since then.
    pub fn replace_graph_and_workspace_base(
        &mut self, graph: &ConnectedGraph, files: &[WorkspaceBaseFile], read: DataVersion,
    ) -> Result<()> {
        let current_project = self.project()?;
        if current_project.id != graph.project.id {
            return Err(StorageError::InvalidProject(DomainError::DifferentProject {
                entity: "workspace".to_owned(),
                id: graph.project.id.to_string(),
                related: current_project.id.to_string(),
            }));
        }

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current: i64 = transaction.pragma_query_value(None, "data_version", |row| row.get(0))?;
        if DataVersion(current) != read {
            return Err(StorageError::ChangedSinceRead);
        }
        transaction.execute_batch(
            "DELETE FROM record_links;
             DELETE FROM release_memberships;
             DELETE FROM capture_promotions;
             DELETE FROM planning_task_dependencies;
             DELETE FROM task_notes;
             DELETE FROM task_gates;
             UPDATE captures SET source_task_id = NULL;
             UPDATE planning_tasks SET parent_id = NULL;
             DELETE FROM planning_tasks;
             DELETE FROM phases;
             DELETE FROM plans;
             DELETE FROM specs;
             DELETE FROM notes;
             DELETE FROM captures;
             DELETE FROM releases;",
        )?;

        for release in &graph.releases {
            transaction.execute(
                "INSERT INTO releases (id, project_id, title, description, status)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    release.id.to_string(),
                    graph.project.id.to_string(),
                    release.title,
                    release.description,
                    release.status.as_str()
                ],
            )?;
        }
        for capture in &graph.captures {
            transaction.execute(
                "INSERT INTO captures (id, project_id, title, body, status, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    capture.id.to_string(),
                    capture.project_id.to_string(),
                    capture.title,
                    capture.body,
                    capture.status.as_str(),
                    capture.created_at
                ],
            )?;
        }
        for spec in &graph.specs {
            transaction.execute(
                "INSERT INTO specs (id, project_id, title, body, acceptance_criteria, status, source_capture_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    spec.id.to_string(),
                    spec.project_id.to_string(),
                    spec.title,
                    spec.body,
                    spec.acceptance_criteria,
                    spec.status.as_str(),
                    spec.source_capture_id.map(|id| id.to_string())
                ],
            )?;
        }
        for plan in &graph.plans {
            transaction.execute(
                "INSERT INTO plans (id, project_id, spec_id, title, body, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    plan.id.to_string(),
                    plan.project_id.to_string(),
                    plan.spec_id.to_string(),
                    plan.title,
                    plan.body,
                    plan.status.as_str()
                ],
            )?;
        }
        for phase in &graph.phases {
            transaction.execute(
                "INSERT INTO phases (id, project_id, plan_id, plan_key, title, body, status, position)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    phase.id.to_string(),
                    phase.project_id.to_string(),
                    phase.plan_id.to_string(),
                    phase.plan_key,
                    phase.title,
                    phase.body,
                    phase.status.as_str(),
                    phase.position
                ],
            )?;
        }
        for task in &graph.tasks {
            transaction.execute(
                "INSERT INTO planning_tasks
                 (id, project_id, spec_id, plan_id, phase_id, parent_id, plan_key, title, body,
                  status, priority, position, handoff, evidence, assignee)
                 VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                rusqlite::params![
                    task.id.to_string(),
                    task.project_id.to_string(),
                    task.spec_id.map(|id| id.to_string()),
                    task.plan_id.map(|id| id.to_string()),
                    task.phase_id.map(|id| id.to_string()),
                    task.plan_key,
                    task.title,
                    task.body,
                    task.status.as_str(),
                    task.priority.as_str(),
                    task.position,
                    task.handoff,
                    task.evidence,
                    task.assignee
                ],
            )?;
        }
        for task in &graph.tasks {
            if let Some(parent_id) = task.parent_id {
                transaction.execute(
                    "UPDATE planning_tasks SET parent_id = ?1 WHERE id = ?2",
                    rusqlite::params![parent_id.to_string(), task.id.to_string()],
                )?;
            }
        }
        let mut positions = HashMap::<TaskId, i64>::new();
        for capture in &graph.captures {
            if let Some(task_id) = capture.source_task_id {
                transaction.execute(
                    "UPDATE captures SET source_task_id = ?1 WHERE id = ?2",
                    rusqlite::params![task_id.to_string(), capture.id.to_string()],
                )?;
            }
        }
        for note in &graph.task_notes {
            let position = positions.entry(note.task_id).or_default();
            transaction.execute(
                "INSERT INTO task_notes (project_id, task_id, position, created_at, kind, text)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    graph.project.id.to_string(),
                    note.task_id.to_string(),
                    *position,
                    note.created_at,
                    note.kind.map(TaskNoteKind::as_str),
                    note.text
                ],
            )?;
            *position += 1;
        }
        for gate in &graph.gates {
            transaction.execute(
                "INSERT INTO task_gates (id, project_id, task_id, kind, target, status, created_at, met_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    gate.id.to_string(),
                    graph.project.id.to_string(),
                    gate.task_id.to_string(),
                    gate.kind.as_str(),
                    gate.target,
                    gate.status.as_str(),
                    gate.created_at,
                    gate.met_at
                ],
            )?;
        }
        for note in &graph.notes {
            transaction.execute(
                "INSERT INTO notes (id, project_id, title, body) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![note.id.to_string(), note.project_id.to_string(), note.title, note.body],
            )?;
        }
        for promotion in &graph.capture_promotions {
            transaction.execute(
                "INSERT INTO capture_promotions (capture_id, project_id, target_kind, target_id)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![
                    promotion.capture_id.to_string(),
                    promotion.project_id.to_string(),
                    promotion.target.promotion_kind(),
                    promotion.target.promotion_target_id()
                ],
            )?;
        }
        for dependency in &graph.dependencies {
            transaction.execute(
                "INSERT INTO planning_task_dependencies (project_id, task_id, blocker_id)
                 VALUES (?1, ?2, ?3)",
                rusqlite::params![
                    graph.project.id.to_string(),
                    dependency.task_id.to_string(),
                    dependency.blocker_id.to_string()
                ],
            )?;
        }
        for membership in &graph.release_memberships {
            transaction.execute(
                "INSERT INTO release_memberships (project_id, release_id, record_kind, record_id)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![
                    membership.project_id.to_string(),
                    membership.release_id.to_string(),
                    membership.record_kind.as_str(),
                    membership.record_id
                ],
            )?;
        }
        for link in &graph.links {
            transaction.execute(
                "INSERT INTO record_links
                 (project_id, source_kind, source_id, target_kind, target_id)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    link.project_id.to_string(),
                    link.source_kind.as_str(),
                    link.source_id,
                    link.target_kind.as_str(),
                    link.target_id
                ],
            )?;
        }

        transaction.execute("DELETE FROM workspace_files", [])?;
        for file in files {
            transaction.execute(
                "INSERT INTO workspace_files (path, content) VALUES (?1, ?2)",
                rusqlite::params![&file.path, &file.content],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Replace the stored workspace base after filesystem export has completed.
    pub fn replace_workspace_base(&mut self, files: &[WorkspaceBaseFile]) -> Result<()> {
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM workspace_files", [])?;
        for file in files {
            transaction.execute(
                "INSERT INTO workspace_files (path, content) VALUES (?1, ?2)",
                rusqlite::params![&file.path, &file.content],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Read the project represented by this operational database.
    pub fn project(&self) -> Result<Project> {
        connected::project(&self.connection)
    }

    /// Read the full ID and title of every capture, release, spec, plan,
    /// phase, task, note and gate. A gate's title is its kind and target.
    pub fn record_refs(&self) -> Result<Vec<RecordRef>> {
        let mut statement = self.connection.prepare(
            "SELECT id, title FROM captures UNION ALL SELECT id, title FROM releases \
             UNION ALL SELECT id, title FROM specs UNION ALL SELECT id, title FROM plans \
             UNION ALL SELECT id, title FROM phases UNION ALL SELECT id, title FROM planning_tasks \
             UNION ALL SELECT id, title FROM notes \
             UNION ALL SELECT id, kind || ' gate ' || target FROM task_gates ORDER BY id",
        )?;
        let rows = statement.query_map([], |row| Ok(RecordRef { id: row.get(0)?, title: row.get(1)? }))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// The short ID of every record, for human output.
    pub fn short_ids(&self) -> Result<ShortIds> {
        Ok(ShortIds::new(self.record_refs()?.into_iter().map(|record| record.id)))
    }

    /// The full record ID `input` names. A full ID comes back unchanged,
    /// whether or not the record exists; a short ID, such as `535B` or
    /// `t-535B`, must end exactly one record's ID. `expected`, a full-ID
    /// prefix such as `arcl-t-`, narrows a short ID that names no kind.
    pub fn resolve_record_id(&self, input: &str, expected: Option<&str>) -> Result<String> {
        if is_full_id(input) {
            return Ok(input.to_owned());
        }
        let records = self.record_refs()?;
        Ok(resolve_short_id(input, &records, expected)?.id.clone())
    }

    /// Load every connected planning record and explicit relationship.
    pub fn connected_graph(&self) -> Result<ConnectedGraph> {
        connected::graph(&self.connection)
    }

    /// Read all inbox captures in creation order.
    pub fn captures(&self) -> Result<Vec<Capture>> {
        let project = self.project()?;
        connected::captures(&self.connection, project.id)
    }

    /// Read one capture by its typed ID.
    pub fn capture(&self, id: CaptureId) -> Result<Option<Capture>> {
        connected::capture(&self.connection, id)
    }

    /// Store a Markdown capture in the project inbox.
    pub fn create_capture(&mut self, title: String, body: String) -> Result<Capture> {
        self.create_capture_from(title, body, None)
    }

    /// Store a Markdown capture filed from `source_task_id`, when given. An
    /// unknown task fails with [`StorageError::PlanningTaskNotFound`].
    pub fn create_capture_from(
        &mut self, title: String, body: String, source_task_id: Option<TaskId>,
    ) -> Result<Capture> {
        let project = self.project()?;
        connected::create_capture(&mut self.connection, project.id, title, body, source_task_id)
    }

    /// Read the task a promoted record came from: the source task of the
    /// capture promoted to `record_id`, a spec, task or note ID.
    pub fn promoted_source_task(&self, record_id: &str) -> Result<Option<TaskId>> {
        connected::promoted_source_task(&self.connection, record_id)
    }

    /// Read the provenance edges created by capture promotion.
    pub fn capture_promotions(&self) -> Result<Vec<CapturePromotion>> {
        let project = self.project()?;
        connected::capture_promotions(&self.connection, project.id)
    }

    /// Update a capture's title or Markdown body.
    pub fn update_capture(&mut self, id: CaptureId, update: CaptureUpdate) -> Result<Capture> {
        connected::update_capture(&mut self.connection, id, update)
    }

    /// Discard a capture without deleting its provenance.
    pub fn discard_capture(&mut self, id: CaptureId) -> Result<Capture> {
        connected::discard_capture(&mut self.connection, id)
    }

    /// Apply an inbox lifecycle action.
    pub fn transition_capture(&mut self, id: CaptureId, action: CaptureAction) -> Result<Capture> {
        match action {
            CaptureAction::Discard => self.discard_capture(id),
        }
    }

    /// Promote a capture to one owned record and preserve its provenance.
    pub fn promote_capture(&mut self, id: CaptureId, input: CapturePromotionInput) -> Result<CapturePromotionResult> {
        connected::promote_capture(&mut self.connection, id, input)
    }

    /// Promote a capture to an owned specification.
    pub fn promote_capture_to_spec(
        &mut self, id: CaptureId, title: String, body: String, acceptance_criteria: String,
    ) -> Result<CapturePromotionResult> {
        self.promote_capture(id, CapturePromotionInput::Spec { title, body, acceptance_criteria })
    }

    /// Promote a capture directly to a project task.
    pub fn promote_capture_to_task(
        &mut self, id: CaptureId, input: CaptureTaskPromotion,
    ) -> Result<CapturePromotionResult> {
        self.promote_capture(id, CapturePromotionInput::Task(input))
    }

    /// Promote a capture to an owned note.
    pub fn promote_capture_to_note(
        &mut self, id: CaptureId, title: String, body: String,
    ) -> Result<CapturePromotionResult> {
        self.promote_capture(id, CapturePromotionInput::Note { title, body })
    }

    /// Read all owned specifications.
    pub fn specs(&self) -> Result<Vec<Spec>> {
        let project = self.project()?;
        connected::specs(&self.connection, project.id)
    }

    /// Read one owned specification.
    pub fn spec(&self, id: SpecId) -> Result<Option<Spec>> {
        connected::spec(&self.connection, id)
    }

    /// Create a specification with an owned Markdown body.
    pub fn create_spec(&mut self, title: String, body: String, acceptance_criteria: String) -> Result<Spec> {
        let project = self.project()?;
        connected::create_spec(&mut self.connection, project.id, title, body, acceptance_criteria)
    }

    /// Update owned specification content and acceptance criteria.
    pub fn update_spec(&mut self, id: SpecId, update: SpecUpdate) -> Result<Spec> {
        connected::update_spec(&mut self.connection, id, update)
    }

    /// Complete or cancel a specification while guarding its open descendants.
    pub fn transition_spec(&mut self, id: SpecId, action: ContainerAction, allow_open_children: bool) -> Result<Spec> {
        connected::transition_spec(&mut self.connection, id, action, allow_open_children)
    }

    /// Read all persistent plans.
    pub fn plans(&self) -> Result<Vec<Plan>> {
        let project = self.project()?;
        connected::plans(&self.connection, project.id)
    }

    /// Read one persistent plan.
    pub fn plan(&self, id: PlanId) -> Result<Option<Plan>> {
        connected::plan(&self.connection, id)
    }

    /// Create a plan owned by a specification.
    pub fn create_plan(&mut self, spec_id: SpecId, title: String, body: String) -> Result<Plan> {
        let project = self.project()?;
        connected::create_plan(&mut self.connection, project.id, spec_id, title, body)
    }

    /// Update persistent plan content.
    pub fn update_plan(&mut self, id: PlanId, update: PlanUpdate) -> Result<Plan> {
        connected::update_plan(&mut self.connection, id, update)
    }

    /// Complete or cancel a plan while guarding its open descendants.
    pub fn transition_plan(&mut self, id: PlanId, action: ContainerAction, allow_open_children: bool) -> Result<Plan> {
        connected::transition_plan(&mut self.connection, id, action, allow_open_children)
    }

    /// Check structured plan input without changing the database.
    pub fn check_plan(&self, id: PlanId, document: &arcl_core::plan::PlanDocument) -> Result<PlanDiff> {
        connected::check_plan(&self.connection, id, document)
    }

    /// Return the changes structured plan input would make.
    pub fn diff_plan(&self, id: PlanId, document: &arcl_core::plan::PlanDocument) -> Result<PlanDiff> {
        connected::diff_plan(&self.connection, id, document)
    }

    /// Apply structured plan input transactionally and avoid duplicate keyed records.
    pub fn apply_plan(&mut self, id: PlanId, document: &arcl_core::plan::PlanDocument) -> Result<PlanApplyResult> {
        connected::apply_plan(&mut self.connection, id, document)
    }

    /// Create a plan and apply structured phases and tasks in one transaction.
    pub fn create_and_apply_plan(
        &mut self, spec_id: SpecId, title: String, body: String, document: &arcl_core::plan::PlanDocument,
    ) -> Result<PlanApplyResult> {
        let project = self.project()?;
        connected::create_and_apply_plan(&mut self.connection, project.id, spec_id, title, body, document)
    }

    /// Read all optional plan phases.
    pub fn phases(&self) -> Result<Vec<Phase>> {
        let project = self.project()?;
        connected::phases(&self.connection, project.id)
    }

    /// Read one optional plan phase.
    pub fn phase(&self, id: PhaseId) -> Result<Option<Phase>> {
        connected::phase(&self.connection, id)
    }

    /// Create an ordered phase inside a plan.
    pub fn create_phase(&mut self, plan_id: PlanId, title: String, body: String, position: i64) -> Result<Phase> {
        let project = self.project()?;
        connected::create_phase(&mut self.connection, project.id, plan_id, title, body, position)
    }

    /// Update phase content and ordering.
    pub fn update_phase(&mut self, id: PhaseId, update: PhaseUpdate) -> Result<Phase> {
        connected::update_phase(&mut self.connection, id, update)
    }

    /// Complete or cancel a phase while guarding its open tasks.
    pub fn transition_phase(
        &mut self, id: PhaseId, action: ContainerAction, allow_open_children: bool,
    ) -> Result<Phase> {
        connected::transition_phase(&mut self.connection, id, action, allow_open_children)
    }

    /// Read all flexible connected-model tasks.
    pub fn planning_tasks(&self) -> Result<Vec<PlanningTask>> {
        let project = self.project()?;
        connected::planning_tasks(&self.connection, project.id)
    }

    /// Read one flexible connected-model task.
    pub fn planning_task(&self, id: TaskId) -> Result<Option<PlanningTask>> {
        connected::planning_task(&self.connection, id)
    }

    /// Create a task at any supported planning level.
    pub fn create_planning_task(&mut self, create: PlanningTaskCreate) -> Result<PlanningTask> {
        connected::create_planning_task(&mut self.connection, create)
    }

    /// Create a task and its blocking relationships in one transaction.
    pub fn create_planning_task_with_dependencies(
        &mut self, create: PlanningTaskCreate, blockers: &[TaskId],
    ) -> Result<PlanningTask> {
        connected::create_planning_task_with_dependencies(&mut self.connection, create, blockers)
    }

    /// Update a task's body, metadata, or explicit ancestry atomically.
    pub fn update_planning_task(&mut self, id: TaskId, update: PlanningTaskUpdate) -> Result<PlanningTask> {
        connected::update_planning_task(&mut self.connection, id, update)
    }

    /// Read all connected-model dependency edges.
    pub fn planning_dependencies(&self) -> Result<Vec<TaskDependency>> {
        let project = self.project()?;
        connected::planning_dependencies(&self.connection, project.id)
    }

    /// Add a dependency between connected-model tasks.
    pub fn add_planning_dependency(&mut self, task_id: TaskId, blocker_id: TaskId) -> Result<TaskDependency> {
        let mut added = self.add_planning_dependencies(task_id, &[blocker_id])?;
        Ok(added.remove(0))
    }

    /// Add the dependencies of `task_id` on each of `blocker_ids` in one
    /// transaction, so a refused edge leaves none of them added.
    pub fn add_planning_dependencies(
        &mut self, task_id: TaskId, blocker_ids: &[TaskId],
    ) -> Result<Vec<TaskDependency>> {
        let project = self.project()?;
        connected::add_planning_dependencies(&mut self.connection, project.id, task_id, blocker_ids)
    }

    /// Remove a connected-model dependency.
    pub fn remove_planning_dependency(&mut self, task_id: TaskId, blocker_id: TaskId) -> Result<TaskDependency> {
        let project = self.project()?;
        connected::remove_planning_dependency(&mut self.connection, project.id, task_id, blocker_id)
    }

    /// Read all actionable leaf tasks across the flexible connected hierarchy.
    pub fn ready_planning_tasks(&self) -> Result<Vec<PlanningTask>> {
        self.ready_planning_tasks_filtered(&PlanningReadyFilter::default())
    }

    /// Read actionable connected-model leaf tasks matching the supplied filters.
    pub fn ready_planning_tasks_filtered(&self, filter: &PlanningReadyFilter) -> Result<Vec<PlanningTask>> {
        let project = self.project()?;
        connected::planning_ready_tasks(&self.connection, project.id, filter)
    }

    /// Build a connected-model task inspection view.
    pub fn planning_task_view(&self, id: TaskId) -> Result<PlanningTaskView> {
        connected::planning_task_view(&self.connection, id)
    }

    /// Read every connected-model task with its readiness and effective
    /// phase, and every phase, in one pass.
    pub fn planning_work(&self) -> Result<PlanningWork> {
        let project = self.project()?;
        connected::planning_work(&self.connection, project.id)
    }

    /// Build the focused context packet for a connected-model task.
    pub fn planning_context(&self, id: TaskId) -> Result<PlanningContext> {
        connected::context(&self.connection, id)
    }

    /// Apply one connected-model task lifecycle action atomically.
    pub fn transition_planning_task(
        &mut self, id: TaskId, action: TaskAction, allow_open_children: bool,
    ) -> Result<PlanningTask> {
        let project = self.project()?;
        connected::transition_planning_task(&mut self.connection, project.id, id, action, allow_open_children)
    }

    /// Start a connected-model task atomically. A pending task must be ready,
    /// apart from having child tasks, unless `force` is set.
    pub fn start_planning_task(&mut self, id: TaskId, force: bool) -> Result<PlanningTask> {
        let project = self.project()?;
        connected::start_planning_task(&mut self.connection, project.id, id, force)
    }

    /// Start a connected-model task and set its assignee atomically. A task
    /// another assignee holds is refused with [`StorageError::TaskClaimed`];
    /// see [`connected::claim_planning_task`].
    pub fn claim_planning_task(&mut self, id: TaskId, force: bool, assignee: &str) -> Result<PlanningTask> {
        let project = self.project()?;
        connected::claim_planning_task(&mut self.connection, project.id, id, force, assignee)
    }

    /// Start the first ready task matching `filter` in one write transaction,
    /// claiming it for `claim` when given. Returns `None` when no task is
    /// ready; see [`connected::start_next_planning_task`].
    pub fn start_next_planning_task(
        &mut self, filter: &PlanningReadyFilter, claim: Option<&str>,
    ) -> Result<Option<PlanningTask>> {
        let project = self.project()?;
        connected::start_next_planning_task(&mut self.connection, project.id, filter, claim)
    }

    /// Complete a connected-model task and optionally store Markdown evidence
    /// atomically. `open_subtasks` chooses what happens to open descendants;
    /// see [`connected::complete_planning_task`].
    pub fn complete_planning_task(
        &mut self, id: TaskId, open_subtasks: OpenSubtasks, evidence: Option<String>,
    ) -> Result<PlanningCompletion> {
        let project = self.project()?;
        connected::complete_planning_task(&mut self.connection, project.id, id, open_subtasks, evidence)
    }

    /// Store a handoff note and park an in-progress connected-model task atomically.
    pub fn handoff_planning_task(&mut self, id: TaskId, note: String) -> Result<PlanningTask> {
        let project = self.project()?;
        connected::handoff_planning_task(&mut self.connection, project.id, id, note)
    }

    /// Append a timestamped note to a task, whatever its status; see
    /// [`connected::add_task_note`].
    pub fn add_task_note(&mut self, id: TaskId, kind: Option<TaskNoteKind>, text: &str) -> Result<TaskNote> {
        let project = self.project()?;
        connected::add_task_note(&mut self.connection, project.id, id, kind, text)
    }

    /// Read the notes on a task, oldest first.
    pub fn task_notes(&self, id: TaskId) -> Result<Vec<TaskNote>> {
        connected::notes_for_task(&self.connection, id)
    }

    /// Add an open gate to a task; see [`connected::add_gate`].
    pub fn add_gate(&mut self, task_id: TaskId, kind: GateKind, target: &str) -> Result<Gate> {
        let project = self.project()?;
        connected::add_gate(&mut self.connection, project.id, task_id, kind, target)
    }

    /// Remove a gate, open or met, and return it as it was.
    pub fn remove_gate(&mut self, id: GateId) -> Result<Gate> {
        let project = self.project()?;
        connected::remove_gate(&mut self.connection, project.id, id)
    }

    /// Mark a gate met; see [`connected::meet_gate`].
    pub fn meet_gate(&mut self, id: GateId) -> Result<Gate> {
        let project = self.project()?;
        connected::meet_gate(&mut self.connection, project.id, id)
    }

    /// Mark gates met in one transaction, re-reading each first; see
    /// [`connected::meet_gates`].
    pub fn meet_gates(&mut self, ids: &[GateId]) -> Result<Vec<Option<Gate>>> {
        let project = self.project()?;
        connected::meet_gates(&mut self.connection, project.id, ids)
    }

    /// Read gate `id`, or fail with [`StorageError::GateNotFound`] when it is
    /// missing or in another project.
    pub fn gate(&self, id: GateId) -> Result<Gate> {
        let project = self.project()?;
        connected::project_gate(&self.connection, project.id, id)
    }

    /// Read the gates on one task, oldest first, or on every task, grouped
    /// by task in task ID order. A task that does not exist is refused.
    pub fn gates(&self, task_id: Option<TaskId>) -> Result<Vec<Gate>> {
        match task_id {
            Some(id) => {
                connected::planning_task(&self.connection, id)?
                    .ok_or_else(|| StorageError::PlanningTaskNotFound { id: id.to_string() })?;
                connected::gates_for_task(&self.connection, id)
            }
            None => connected::gates(&self.connection, self.project()?.id),
        }
    }

    /// Read all Markdown notes.
    pub fn notes(&self) -> Result<Vec<Note>> {
        let project = self.project()?;
        connected::notes(&self.connection, project.id)
    }

    /// Read one Markdown note.
    pub fn note(&self, id: NoteId) -> Result<Option<Note>> {
        connected::note(&self.connection, id)
    }

    /// Create a Markdown note in the project.
    pub fn create_note(&mut self, title: String, body: String) -> Result<Note> {
        let project = self.project()?;
        connected::create_note(&mut self.connection, project.id, title, body)
    }

    /// Update note content.
    pub fn update_note(&mut self, id: NoteId, update: NoteUpdate) -> Result<Note> {
        connected::update_note(&mut self.connection, id, update)
    }

    /// Read one release by its validated identifier.
    pub fn release(&self, id: ReleaseId) -> Result<Option<Release>> {
        releases::find(&self.connection, &id)
    }

    /// Read all releases in deterministic identifier order.
    pub fn releases(&self) -> Result<Vec<Release>> {
        releases::list(&self.connection)
    }

    /// Create an open release.
    pub fn create_release(&mut self, title: String, description: String) -> Result<Release> {
        releases::create(&mut self.connection, title, description)
    }

    /// Update a release's title or Markdown body.
    pub fn update_release(
        &mut self, id: ReleaseId, title: Option<String>, description: Option<String>,
    ) -> Result<Release> {
        releases::update(&mut self.connection, id, title, description)
    }

    /// Complete or cancel a release, guarding non-terminal explicit members.
    pub fn transition_release(
        &mut self, id: ReleaseId, action: ContainerAction, allow_open_children: bool,
    ) -> Result<Release> {
        releases::transition(&mut self.connection, id, action, allow_open_children)
    }

    /// Add an explicit release member. Descendants are never added implicitly.
    pub fn add_release_membership(
        &mut self, release_id: ReleaseId, record_kind: ReleaseMemberKind, record_id: String,
    ) -> Result<ReleaseMembership> {
        let project = self.project()?;
        connected::add_release_membership(&mut self.connection, project.id, release_id, record_kind, record_id)
    }

    /// Remove an explicit release member.
    pub fn remove_release_membership(
        &mut self, release_id: ReleaseId, record_kind: ReleaseMemberKind, record_id: &str,
    ) -> Result<ReleaseMembership> {
        let project = self.project()?;
        connected::remove_release_membership(&mut self.connection, project.id, release_id, record_kind, record_id)
    }

    /// Read release memberships without expanding descendants.
    pub fn release_memberships(&self) -> Result<Vec<ReleaseMembership>> {
        let project = self.project()?;
        connected::release_memberships(&self.connection, project.id)
    }

    /// Link a note to one related record in this project.
    pub fn add_note_link(
        &mut self, note_id: NoteId, record_kind: LinkedRecordKind, record_id: String,
    ) -> Result<NoteLink> {
        let project = self.project()?;
        connected::add_note_link(&mut self.connection, project.id, note_id, record_kind, record_id)
    }

    /// Remove a note relationship.
    pub fn remove_note_link(
        &mut self, note_id: NoteId, record_kind: LinkedRecordKind, record_id: &str,
    ) -> Result<NoteLink> {
        let project = self.project()?;
        connected::remove_note_link(&mut self.connection, project.id, note_id, record_kind, record_id)
    }

    /// Read note relationships as explicit edges.
    pub fn note_links(&self) -> Result<Vec<NoteLink>> {
        let project = self.project()?;
        connected::note_links(&self.connection, project.id)
    }

    /// Add a relationship between any two records in the project.
    pub fn add_record_link(
        &mut self, source_kind: LinkedRecordKind, source_id: String, target_kind: LinkedRecordKind, target_id: String,
    ) -> Result<RecordLink> {
        let project = self.project()?;
        connected::add_record_link(
            &mut self.connection,
            project.id,
            source_kind,
            source_id,
            target_kind,
            target_id,
        )
    }

    /// Remove a relationship between any two records in the project.
    pub fn remove_record_link(
        &mut self, source_kind: LinkedRecordKind, source_id: &str, target_kind: LinkedRecordKind, target_id: &str,
    ) -> Result<RecordLink> {
        let project = self.project()?;
        connected::remove_record_link(
            &mut self.connection,
            project.id,
            source_kind,
            source_id,
            target_kind,
            target_id,
        )
    }

    /// Read every generic record relationship.
    pub fn record_links(&self) -> Result<Vec<RecordLink>> {
        let project = self.project()?;
        connected::record_links(&self.connection, project.id)
    }

    /// Validate SQLite constraints and the connected planning graph.
    pub fn check(&self) -> Result<CheckReport> {
        connected::check(&self.connection)
    }

    #[cfg(test)]
    pub fn connection(&self) -> &Connection {
        &self.connection
    }
}

fn configure(connection: &mut Connection) -> Result<()> {
    connection.pragma_update(None, "foreign_keys", true)?;
    connection.busy_timeout(BUSY_TIMEOUT)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{CURRENT_VERSION, Database, StorageError};
    use arcl_core::domain::ShortIdError;

    #[test]
    fn opening_a_database_applies_embedded_migrations() {
        let database = Database::open_in_memory().expect("in-memory SQLite opens");
        assert_eq!(database.schema_version().expect("version is readable"), CURRENT_VERSION);

        let foreign_keys: i32 = database
            .connection()
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .expect("foreign keys pragma is readable");
        assert_eq!(foreign_keys, 1);

        let format_version: String = database
            .connection()
            .query_row(
                "SELECT value FROM meta WHERE key = 'database-format-version'",
                [],
                |row| row.get(0),
            )
            .expect("format marker exists");
        assert_eq!(format_version, CURRENT_VERSION.to_string());
    }

    #[test]
    fn a_graph_replacement_refuses_a_commit_made_after_the_read() {
        let directory = tempfile::tempdir().expect("directory creates");
        let path = directory.path().join("arcl.db");
        let mut reader = Database::open(&path).expect("reader opens");
        let mut writer = Database::open(&path).expect("writer opens");
        reader
            .create_capture("Before".to_owned(), "Read by the replacement".to_owned())
            .expect("capture creates");

        let read = reader.data_version().expect("version reads");
        let graph = reader.connected_graph().expect("graph loads");
        let written = writer
            .create_capture("After".to_owned(), "Committed by another connection".to_owned())
            .expect("capture creates");

        let error = reader
            .replace_graph_and_workspace_base(&graph, &[], read)
            .expect_err("stale replacement refuses");
        assert!(matches!(error, StorageError::ChangedSinceRead), "{error:?}");
        let captures = reader.captures().expect("captures load");
        assert_eq!(captures.len(), 2);
        assert!(captures.iter().any(|capture| capture.id == written.id));

        let read = reader.data_version().expect("version reads");
        let graph = reader.connected_graph().expect("graph loads");
        reader
            .replace_graph_and_workspace_base(&graph, &[], read)
            .expect("current replacement succeeds");
        assert_eq!(reader.captures().expect("captures load").len(), 2);
    }

    #[test]
    fn short_ids_resolve_across_every_record_kind() {
        let mut database = Database::open_in_memory().expect("database opens");
        let capture = database
            .create_capture("Capture".to_owned(), String::new())
            .expect("capture creates");
        let spec = database
            .create_spec("Spec".to_owned(), String::new(), String::new())
            .expect("spec creates");
        let note = database
            .create_note("Note".to_owned(), String::new())
            .expect("note creates");
        let ids = database.short_ids().expect("short IDs compute");

        for id in [capture.id.to_string(), spec.id.to_string(), note.id.to_string()] {
            let short = ids.short(&id);
            assert!(short.len() >= 4 && id.ends_with(short), "{short} is a suffix of {id}");
            assert_eq!(database.resolve_record_id(short, None).expect("short ID resolves"), id);
        }
        let full = spec.id.to_string();
        assert_eq!(database.resolve_record_id(&full, None).expect("full ID passes"), full);
        let suffix = &full[full.len() - 6..];
        assert_eq!(
            database
                .resolve_record_id(&format!("s-{suffix}"), None)
                .expect("kind narrows"),
            full
        );
        let error = database
            .resolve_record_id(&format!("n-{suffix}"), None)
            .expect_err("no note ends in the spec's suffix");
        assert!(
            matches!(error, StorageError::ShortId(ShortIdError::NotFound { .. })),
            "{error:?}"
        );
        assert_eq!(error.exit_code(), 5);
        let error = database.resolve_record_id("AB", None).expect_err("too short");
        assert_eq!(error.exit_code(), 3);
    }
}
