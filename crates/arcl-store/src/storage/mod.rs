mod connected;
mod migrations;
mod releases;

pub use connected::*;

use std::{path::Path, time::Duration};

use rusqlite::Connection;
use thiserror::Error;

use arcl_core::domain::*;

/// The newest SQLite schema version understood by the application.
pub const CURRENT_VERSION: i32 = 3;

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

pub type Result<T> = std::result::Result<T, StorageError>;

/// One exact file stored as the last successful snapshot base.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotBaseFile {
    /// The path relative to the snapshot root.
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
}

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

    /// Read the exact files recorded by the last successful snapshot export.
    pub fn snapshot_base(&self) -> Result<Vec<SnapshotBaseFile>> {
        let mut statement = self
            .connection
            .prepare("SELECT path, content FROM snapshot_files ORDER BY path")?;
        let rows = statement.query_map([], |row| {
            Ok(SnapshotBaseFile { path: row.get(0)?, content: row.get(1)? })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Replace the stored snapshot base after filesystem export has completed.
    pub fn replace_snapshot_base(&mut self, files: &[SnapshotBaseFile]) -> Result<()> {
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM snapshot_files", [])?;
        for file in files {
            transaction.execute(
                "INSERT INTO snapshot_files (path, content) VALUES (?1, ?2)",
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
        let project = self.project()?;
        connected::create_capture(&mut self.connection, project.id, title, body)
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
        let project = self.project()?;
        connected::add_planning_dependency(&mut self.connection, project.id, task_id, blocker_id)
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

    /// Complete a connected-model task and optionally store Markdown evidence atomically.
    pub fn complete_planning_task(
        &mut self, id: TaskId, allow_open_children: bool, evidence: Option<String>,
    ) -> Result<PlanningTask> {
        let project = self.project()?;
        connected::complete_planning_task(&mut self.connection, project.id, id, allow_open_children, evidence)
    }

    /// Store a handoff note and park an in-progress connected-model task atomically.
    pub fn handoff_planning_task(&mut self, id: TaskId, note: String) -> Result<PlanningTask> {
        let project = self.project()?;
        connected::handoff_planning_task(&mut self.connection, project.id, id, note)
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
    use super::{CURRENT_VERSION, Database};

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
}
