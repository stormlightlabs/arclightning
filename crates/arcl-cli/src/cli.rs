use std::fmt;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// The explicit color policy accepted by the CLI.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

impl fmt::Display for ColorChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Auto => "auto",
            Self::Always => "always",
            Self::Never => "never",
        };
        formatter.write_str(name)
    }
}

/// The side whose version `workspace resolve` keeps.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum KeepSide {
    /// Keep the database's version and write it to the workspace.
    Database,
    /// Keep the workspace file and import it into the database.
    Workspace,
}

/// The layout of human `arcl tree` output.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum TreeStyle {
    /// A box-drawing tree that folds each run of finished phases into one line.
    #[default]
    Folded,
    /// The box-drawing tree with every phase unfolded and short IDs on the right.
    Full,
    /// STATUS, ID and TITLE columns, with titles indented by depth and no box characters.
    Outline,
}

/// Arc Lightning commands.
#[derive(Clone, Debug, Subcommand)]
pub enum Command {
    /// Initialize or rediscover a project.
    Init {
        /// Enable the optional version-controlled workspace path when creating configuration.
        #[arg(long)]
        workspace: bool,
    },
    /// Capture and manage unstructured inbox records.
    Capture {
        #[command(subcommand)]
        command: CaptureCommand,
    },
    /// Manage named releases.
    Release {
        #[command(subcommand)]
        command: ReleaseCommand,
    },
    /// Create and manage owned Markdown specifications.
    Spec {
        #[command(subcommand)]
        command: SpecCommand,
    },
    /// Create and apply persistent implementation plans.
    Plan {
        #[command(subcommand)]
        command: PlanCommand,
    },
    /// Create and execute tasks in the connected planning model.
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
    /// Add and remove task blocking relationships.
    Dependency {
        #[command(subcommand)]
        command: DependencyCommand,
    },
    /// Create and manage Markdown notes.
    Note {
        #[command(subcommand)]
        command: NoteCommand,
    },
    /// Install the Arc Lightning agent skill into the project.
    Skill {
        #[command(subcommand)]
        command: SkillCommand,
    },
    /// Inspect one record selected by its typed ID prefix.
    Show {
        /// The record ID, or a short ID such as `535B` or `t-535B`.
        id: String,
    },
    /// List connected records with optional graph filters.
    List {
        #[command(flatten)]
        filters: ListArgs,
    },
    /// Display the connected planning hierarchy in deterministic order.
    Tree {
        /// Optional record ID or short ID to use as the tree root.
        id: Option<String>,
        #[command(flatten)]
        options: TreeArgs,
    },
    /// Explain every condition affecting a task's readiness.
    Explain {
        /// The task identifier.
        task_id: String,
    },
    /// Return the bounded context packet for a task.
    Context {
        /// The task identifier.
        task_id: String,
    },
    /// Print the agent loop, open work with handoffs, and the first ready
    /// tasks, for a session-start hook.
    Prime,
    /// Validate database and graph invariants.
    Check,
    /// Inspect or synchronize the version-controlled workspace.
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
    },
    /// List actionable leaf tasks in deterministic order.
    Ready {
        #[command(flatten)]
        filters: ReadyArgs,
    },
    /// Show the first actionable leaf task, if one exists.
    Next {
        #[command(flatten)]
        filters: ReadyArgs,
        /// Start the task in the same transaction that selects it.
        #[arg(long)]
        start: bool,
        /// Claim the started task for NAME, as `task start --claim` does.
        /// Without NAME, use `ARCL_AGENT`, then the Git user name.
        #[arg(long, value_name = "NAME", num_args = 0..=1, requires = "start")]
        claim: Option<Option<String>>,
    },
    /// Serve the web app on 127.0.0.1 until interrupted.
    Serve {
        /// Listen on this port instead of an ephemeral one.
        #[arg(long, value_name = "PORT")]
        port: Option<u16>,
    },
    /// Export records for the web app's claude.ai artifact and apply the edits queued on it.
    Web {
        #[command(subcommand)]
        command: WebCommand,
    },
}

/// Inbox capture commands.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Subcommand)]
pub enum CaptureCommand {
    /// Capture a new Markdown thought.
    Create {
        /// The capture title.
        title: String,
        #[command(flatten)]
        body: MarkdownArgs,
    },
    /// Show one capture.
    Show {
        /// The capture ID to show.
        id: String,
    },
    /// List captures in creation order.
    List,
    /// Update an existing capture.
    Update {
        /// The capture ID to update.
        id: String,
        /// Replace the capture title.
        #[arg(long, value_name = "TITLE")]
        title: Option<String>,
        #[command(flatten)]
        body: MarkdownArgs,
    },
    /// Discard a capture without deleting its provenance.
    Discard {
        /// The capture ID to discard.
        id: String,
    },
    /// Promote a capture to a spec, task, or note.
    Promote {
        /// The source capture identifier.
        id: String,
        /// Promotion target (`spec`, `task`, or `note`).
        target: Option<String>,
        /// Promotion target supplied as an option.
        #[arg(long, value_name = "KIND", conflicts_with = "target")]
        to: Option<String>,
        /// Override the destination title; otherwise use the capture title.
        #[arg(long, value_name = "TITLE")]
        title: Option<String>,
        #[command(flatten)]
        body: MarkdownArgs,
        /// Markdown acceptance criteria for a promoted specification.
        #[arg(
            long,
            value_name = "MARKDOWN",
            allow_hyphen_values = true,
            conflicts_with = "acceptance_criteria_file"
        )]
        acceptance_criteria: Option<String>,
        /// Read specification acceptance criteria from a file, or `-` for standard input.
        #[arg(long, value_name = "PATH|-", conflicts_with = "acceptance_criteria")]
        acceptance_criteria_file: Option<PathBuf>,
        /// Attach a promoted task to a specification.
        #[arg(long, value_name = "ID")]
        spec: Option<String>,
        /// Attach a promoted task to a plan.
        #[arg(long, value_name = "ID")]
        plan: Option<String>,
        /// Attach a promoted task to a phase.
        #[arg(long, value_name = "ID")]
        phase: Option<String>,
        /// Attach a promoted task below another task.
        #[arg(long, value_name = "ID")]
        parent: Option<String>,
        /// The priority for a promoted task.
        #[arg(long, default_value = "normal", value_name = "PRIORITY")]
        priority: String,
        /// The display position for a promoted task.
        #[arg(long, default_value_t = 0, value_name = "N")]
        position: i64,
    },
}

/// Release commands.
#[derive(Clone, Debug, Subcommand)]
pub enum ReleaseCommand {
    /// Create an open release.
    Create {
        /// The release title.
        title: String,
        #[command(flatten)]
        body: MarkdownArgs,
    },
    /// Show one release.
    Show {
        /// The release ID to show.
        id: String,
    },
    /// List releases.
    List,
    /// Update a release title or Markdown body.
    Update {
        /// The release ID to update.
        id: String,
        /// Replace the release title.
        #[arg(long, value_name = "TITLE")]
        title: Option<String>,
        #[command(flatten)]
        body: MarkdownArgs,
    },
    /// Mark a release completed.
    Complete {
        /// The release ID to complete.
        id: String,
        /// Complete only the release even when descendants are open.
        #[arg(long)]
        allow_open_children: bool,
    },
    /// Mark a release cancelled.
    Cancel {
        /// The release ID to cancel.
        id: String,
        /// Cancel only the release even when descendants are open.
        #[arg(long)]
        allow_open_children: bool,
    },
    /// Manage explicit release membership.
    Member {
        #[command(subcommand)]
        command: MembershipCommand,
    },
}

/// Explicit release membership commands.
#[derive(Clone, Debug, Subcommand)]
pub enum MembershipCommand {
    /// Add one spec, plan, task, or note to a release.
    Add {
        /// The release ID to update.
        release_id: String,
        /// Member kind: `spec`, `plan`, `task`, or `note`.
        #[arg(long, value_name = "KIND")]
        kind: String,
        /// The member's typed ID.
        #[arg(long, value_name = "ID")]
        record_id: String,
    },
    /// Remove one explicit release member.
    Remove {
        /// The release ID to update.
        release_id: String,
        /// Member kind: `spec`, `plan`, `task`, or `note`.
        #[arg(long, value_name = "KIND")]
        kind: String,
        /// The member's typed ID.
        #[arg(long, value_name = "ID")]
        record_id: String,
    },
    /// List explicit members without expanding descendants.
    List {
        /// The release ID to list.
        release_id: String,
    },
}

/// Specification commands.
#[derive(Clone, Debug, Subcommand)]
pub enum SpecCommand {
    /// Create an open specification.
    Create {
        /// The specification title.
        title: String,
        #[command(flatten)]
        body: MarkdownArgs,
        #[command(flatten)]
        acceptance: AcceptanceArgs,
    },
    /// Show one specification.
    Show {
        /// The specification ID to show.
        id: String,
    },
    /// List specifications.
    List,
    /// Update an owned specification.
    Update {
        /// The specification ID to update.
        id: String,
        /// Replace the specification title.
        #[arg(long, value_name = "TITLE")]
        title: Option<String>,
        #[command(flatten)]
        body: MarkdownArgs,
        #[command(flatten)]
        acceptance: AcceptanceArgs,
    },
    /// Mark a specification completed.
    Complete {
        /// The specification ID to complete.
        id: String,
        /// Complete only the specification even when descendants are open.
        #[arg(long)]
        allow_open_children: bool,
    },
    /// Mark a specification cancelled.
    Cancel {
        /// The specification ID to cancel.
        id: String,
        /// Cancel only the specification even when descendants are open.
        #[arg(long)]
        allow_open_children: bool,
    },
}

/// Plan commands.
#[derive(Clone, Debug, Subcommand)]
pub enum PlanCommand {
    /// Create an open plan owned by a specification.
    Create {
        /// The plan title.
        title: String,
        /// The owning specification ID.
        #[arg(long, value_name = "ID")]
        spec: String,
        #[command(flatten)]
        body: MarkdownArgs,
        /// Apply structured phases and tasks from a TOML file after creating the plan.
        #[arg(long, value_name = "PATH|-", conflicts_with = "no_input")]
        input: Option<PathBuf>,
        /// Create only the plan, without applying structured input.
        #[arg(long, conflicts_with = "input")]
        no_input: bool,
    },
    /// Show one plan and its explicit phases, tasks, and dependencies.
    Show {
        /// The plan ID to show.
        id: String,
    },
    /// List plans.
    List,
    /// Update a persistent plan.
    Update {
        /// The plan ID to update.
        id: String,
        /// Replace the plan title.
        #[arg(long, value_name = "TITLE")]
        title: Option<String>,
        #[command(flatten)]
        body: MarkdownArgs,
    },
    /// Check structured input without writing.
    Check {
        /// The plan ID to check.
        id: String,
        /// The structured plan file, or `-` for standard input.
        #[arg(long, value_name = "PATH|-")]
        file: PathBuf,
    },
    /// Show the changes structured input would make.
    Diff {
        /// The plan ID to compare.
        id: String,
        /// The structured plan file, or `-` for standard input.
        #[arg(long, value_name = "PATH|-")]
        file: PathBuf,
    },
    /// Apply structured input transactionally.
    Apply {
        /// The plan ID to update.
        id: String,
        /// The structured plan file, or `-` for standard input.
        #[arg(long, value_name = "PATH|-")]
        file: PathBuf,
    },
    /// Mark a plan completed.
    Complete {
        /// The plan ID to complete.
        id: String,
        /// Complete only the plan even when descendants are open.
        #[arg(long)]
        allow_open_children: bool,
    },
    /// Mark a plan cancelled.
    Cancel {
        /// The plan ID to cancel.
        id: String,
        /// Cancel only the plan even when descendants are open.
        #[arg(long)]
        allow_open_children: bool,
    },
    /// Manage optional ordered phases.
    Phase {
        #[command(subcommand)]
        command: PhaseCommand,
    },
}

/// Optional plan phase commands.
#[derive(Clone, Debug, Subcommand)]
pub enum PhaseCommand {
    /// Create an open phase in a plan.
    Create {
        /// The phase title.
        title: String,
        /// The owning plan ID.
        #[arg(long, value_name = "ID")]
        plan: String,
        /// The display position within the plan.
        #[arg(long, default_value_t = 0, value_name = "N")]
        position: i64,
        #[command(flatten)]
        body: MarkdownArgs,
    },
    /// Show one phase.
    Show {
        /// The phase ID to show.
        id: String,
    },
    /// List phases.
    List,
    /// Update a phase.
    Update {
        /// The phase ID to update.
        id: String,
        /// Replace the phase title.
        #[arg(long, value_name = "TITLE")]
        title: Option<String>,
        /// Replace the display position within the plan.
        #[arg(long, value_name = "N")]
        position: Option<i64>,
        #[command(flatten)]
        body: MarkdownArgs,
    },
    /// Mark a phase completed.
    Complete {
        /// The phase ID to complete.
        id: String,
        /// Complete only the phase even when tasks are open.
        #[arg(long)]
        allow_open_children: bool,
    },
    /// Mark a phase cancelled.
    Cancel {
        /// The phase ID to cancel.
        id: String,
        /// Cancel only the phase even when tasks are open.
        #[arg(long)]
        allow_open_children: bool,
    },
}

/// Task and subtask commands.
#[derive(Clone, Debug, Subcommand)]
pub enum TaskCommand {
    /// Create a pending task at any supported planning level.
    Create {
        /// The task title.
        title: String,
        /// Attach the task to a specification.
        #[arg(long, value_name = "ID")]
        spec: Option<String>,
        /// Attach the task to a plan.
        #[arg(long, value_name = "ID")]
        plan: Option<String>,
        /// Attach the task to a phase.
        #[arg(long, value_name = "ID")]
        phase: Option<String>,
        /// Attach the task below another task.
        #[arg(long, value_name = "ID")]
        parent: Option<String>,
        /// The task priority.
        #[arg(long, default_value = "normal", value_name = "PRIORITY")]
        priority: String,
        /// The display position within its container or parent.
        #[arg(long, default_value_t = 0, value_name = "N")]
        position: i64,
        /// Add direct blocker relationships after creating the task.
        #[arg(long = "blocked-by", value_name = "ID", num_args = 1..)]
        blocked_by: Vec<String>,
        #[command(flatten)]
        body: MarkdownArgs,
    },
    /// Show one task with readiness, ancestry, blockers, and dependents.
    Show {
        /// The task ID to show.
        id: String,
    },
    /// List tasks.
    List,
    /// Update task metadata, Markdown, or ancestry.
    Update {
        /// The task ID to update.
        id: String,
        /// Replace the task title.
        #[arg(long, value_name = "TITLE")]
        title: Option<String>,
        #[command(flatten)]
        body: MarkdownArgs,
        /// Replace the task priority.
        #[arg(long, value_name = "PRIORITY")]
        priority: Option<String>,
        /// Replace the display position.
        #[arg(long, value_name = "N")]
        position: Option<i64>,
        /// Attach the task to a specification.
        #[arg(long, value_name = "ID", conflicts_with = "no_spec")]
        spec: Option<String>,
        /// Remove the task's specification association.
        #[arg(long)]
        no_spec: bool,
        /// Attach the task to a plan.
        #[arg(long, value_name = "ID", conflicts_with = "no_plan")]
        plan: Option<String>,
        /// Remove the task's plan association.
        #[arg(long)]
        no_plan: bool,
        /// Attach the task to a phase.
        #[arg(long, value_name = "ID", conflicts_with = "no_phase")]
        phase: Option<String>,
        /// Remove the task's phase association.
        #[arg(long)]
        no_phase: bool,
        /// Reparent the task below another task.
        #[arg(long, value_name = "ID", conflicts_with = "no_parent")]
        parent: Option<String>,
        /// Remove the task's parent.
        #[arg(long)]
        no_parent: bool,
    },
    /// Start pending work.
    Start {
        /// The task ID to start.
        id: String,
        /// Start the task even when it is not ready. A claimed task still
        /// needs `--claim` with its holder's name.
        #[arg(long)]
        force: bool,
        /// Claim the task for NAME, refusing a task someone else holds.
        /// Without NAME, use `ARCL_AGENT`, then the Git user name.
        #[arg(long, value_name = "NAME", num_args = 0..=1)]
        claim: Option<Option<String>>,
    },
    /// Temporarily exclude work from readiness.
    Park {
        /// The task ID to park.
        id: String,
    },
    /// Return parked work to pending.
    Unpark {
        /// The task ID to unpark.
        id: String,
    },
    /// Leave a resume note and park in-progress work atomically.
    Handoff {
        /// The task ID to hand off.
        id: String,
        /// The Markdown resume note.
        #[arg(long, conflicts_with = "note_file", allow_hyphen_values = true)]
        note: Option<String>,
        /// Read the Markdown resume note from a file, or `-` for standard input.
        #[arg(long, value_name = "PATH|-", conflicts_with = "note")]
        note_file: Option<PathBuf>,
    },
    /// Append a timestamped note to a task, whatever its status.
    Note {
        /// The task ID to note.
        id: String,
        /// What the note records: decision, observation, blocker or result.
        /// Without it the note has no kind.
        #[arg(long, value_name = "KIND")]
        kind: Option<String>,
        /// The plain-text note, at most 2000 characters. It may span lines.
        #[arg(allow_hyphen_values = true)]
        text: String,
    },
    /// Mark work completed.
    Complete {
        /// The task ID to complete.
        id: String,
        /// Complete only this task even when descendants are open.
        #[arg(long)]
        allow_open_children: bool,
        /// Also complete every pending and in-progress descendant, in one
        /// write. Refused while a descendant is parked or a blocker outside
        /// the subtree is unfinished.
        #[arg(long, conflicts_with = "allow_open_children")]
        with_subtasks: bool,
        /// Store Markdown completion evidence on this task only.
        #[arg(long, conflicts_with = "evidence_file", allow_hyphen_values = true)]
        evidence: Option<String>,
        /// Read Markdown completion evidence from a file, or `-` for standard input.
        #[arg(long, value_name = "PATH|-", conflicts_with = "evidence")]
        evidence_file: Option<PathBuf>,
    },
    /// Mark work cancelled.
    Cancel {
        /// The task ID to cancel.
        id: String,
        /// Cancel only this task even when descendants are open.
        #[arg(long)]
        allow_open_children: bool,
    },
    /// Explain readiness for one task.
    Explain {
        /// The task ID to explain.
        id: String,
    },
    /// Return the focused context packet for one task.
    Context {
        /// The task ID to contextualize.
        id: String,
    },
}

/// Task dependency commands.
#[derive(Clone, Debug, Subcommand)]
pub enum DependencyCommand {
    /// Make one task wait for another task to complete.
    Add {
        /// The task that is blocked.
        task_id: String,
        /// The task that blocks it.
        #[arg(long = "blocked-by", value_name = "ID")]
        blocker_id: String,
    },
    /// Remove one task blocking relationship.
    Remove {
        /// The task that is blocked.
        task_id: String,
        /// The task that blocks it.
        #[arg(long = "blocked-by", value_name = "ID")]
        blocker_id: String,
    },
    /// List connected task dependencies.
    List,
}

/// Markdown note commands.
#[derive(Clone, Debug, Subcommand)]
pub enum NoteCommand {
    /// Create a Markdown note.
    Create {
        /// The note title.
        title: String,
        #[command(flatten)]
        body: MarkdownArgs,
    },
    /// Show one note.
    Show {
        /// The note ID to show.
        id: String,
    },
    /// List notes.
    List,
    /// Update a note.
    Update {
        /// The note ID to update.
        id: String,
        /// Replace the note title.
        #[arg(long, value_name = "TITLE")]
        title: Option<String>,
        #[command(flatten)]
        body: MarkdownArgs,
    },
    /// Add an explicit note relationship.
    Link {
        #[command(subcommand)]
        command: NoteLinkCommand,
    },
}

/// Note relationship commands.
#[derive(Clone, Debug, Subcommand)]
pub enum NoteLinkCommand {
    /// Link a note to another project record.
    Add {
        /// The note ID to update.
        note_id: String,
        /// Target record kind: capture, spec, plan, phase, task, note, or release.
        #[arg(long, value_name = "KIND")]
        kind: String,
        /// The target record's typed ID.
        #[arg(long, value_name = "ID")]
        record_id: String,
    },
    /// Remove a note relationship.
    Remove {
        /// The note ID to update.
        note_id: String,
        /// Target record kind: capture, spec, plan, phase, task, note, or release.
        #[arg(long, value_name = "KIND")]
        kind: String,
        /// The target record's typed ID.
        #[arg(long, value_name = "ID")]
        record_id: String,
    },
    /// List relationships for a note.
    List {
        /// The note ID to list.
        note_id: String,
    },
}

/// Agent skill commands.
#[derive(Clone, Debug, Subcommand)]
pub enum SkillCommand {
    /// Write the skill into `.claude/skills/arclightning/` and
    /// `.agents/skills/arclightning/`, leaving identical files unchanged.
    Install {
        /// Write nothing; report each missing or changed file and exit with
        /// status 4 when any differs.
        #[arg(long)]
        check: bool,
    },
}

/// Commands that keep a claude.ai artifact of the web app in step with the project.
#[derive(Clone, Debug, Subcommand)]
pub enum WebCommand {
    /// Write the artifact database's documents as JSON files under a directory,
    /// with a manifest naming the ones that changed since the last export there.
    Export {
        /// The directory to write; created if missing.
        #[arg(value_name = "DIR")]
        dir: PathBuf,
        /// List every document as changed, for a sync into a new artifact or
        /// after a sync that did not finish.
        #[arg(long)]
        full: bool,
    },
    /// Apply the edits queued on the artifact, read from a JSON file, as `arcl serve` applies a change.
    Apply {
        /// The JSON array of queued edits, or `-` for standard input.
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },
}

/// Explicit workspace commands.
#[derive(Clone, Debug, Subcommand)]
pub enum WorkspaceCommand {
    /// Export the complete database graph into workspace records.
    Export,
    /// Validate and import the complete workspace graph into SQLite.
    Import,
    /// Carry one-sided changes between SQLite and the workspace; stop on divergence.
    Sync,
    /// Report what differs from the last sync, conflicts, and Git state, without writing.
    Status,
    /// Keep one side's version of files both sides changed, then sync the rest.
    Resolve {
        /// The side whose version to keep.
        #[arg(long, value_enum)]
        keep: KeepSide,
        /// Workspace-relative paths, as `arcl workspace status` prints them.
        #[arg(required = true, value_name = "PATH")]
        paths: Vec<PathBuf>,
    },
}

/// Options that choose what `arcl tree` shows.
#[derive(Clone, Copy, Debug, Default, Args)]
pub struct TreeArgs {
    /// Include promoted and discarded captures. Human output also unfolds
    /// finished phases and lists the Inbox, Notes and Releases sections.
    #[arg(long)]
    pub all: bool,
    /// Draw human output with ASCII characters only, status marks included.
    #[arg(long)]
    pub ascii: bool,
    /// Stop human output after this many levels of records.
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u16).range(1..))]
    pub depth: Option<u16>,
    /// Choose the human output layout.
    #[arg(long, value_enum, default_value_t = TreeStyle::Folded)]
    pub style: TreeStyle,
}

/// Filters accepted by broad connected-record queries.
#[derive(Clone, Debug, Default, Args)]
pub struct ListArgs {
    /// Restrict records to one kind: capture, release, spec, plan, phase, task, or note.
    #[arg(long, value_name = "KIND")]
    pub kind: Option<String>,
    /// Restrict records to one or more statuses.
    #[arg(long, value_name = "STATUS", num_args = 1..)]
    pub status: Vec<String>,
    /// Restrict tasks to one or more priorities.
    #[arg(long, value_name = "PRIORITY", num_args = 1..)]
    pub priority: Vec<String>,
    /// Restrict records to one release.
    #[arg(long, value_name = "ID")]
    pub release: Option<String>,
    /// Restrict records to one specification ancestry.
    #[arg(long, value_name = "ID")]
    pub spec: Option<String>,
    /// Restrict records to one plan ancestry.
    #[arg(long, value_name = "ID")]
    pub plan: Option<String>,
    /// Restrict records to one phase ancestry.
    #[arg(long, value_name = "ID")]
    pub phase: Option<String>,
    /// Restrict tasks to children of one task.
    #[arg(long, value_name = "ID")]
    pub parent: Option<String>,
    /// Restrict tasks to those claimed by one assignee.
    #[arg(long, value_name = "NAME")]
    pub assignee: Option<String>,
}

/// Filters accepted by ready-work and next-work queries.
#[derive(Clone, Debug, Default, Args)]
pub struct ReadyArgs {
    /// Restrict results to one or more priorities.
    #[arg(long, value_name = "PRIORITY", num_args = 1..)]
    pub priority: Vec<String>,
    /// Restrict results to one specification ancestry.
    #[arg(long, value_name = "ID")]
    pub spec: Option<String>,
    /// Restrict results to one plan ancestry.
    #[arg(long, value_name = "ID")]
    pub plan: Option<String>,
    /// Restrict results to one phase ancestry.
    #[arg(long, value_name = "ID")]
    pub phase: Option<String>,
    /// Restrict results to direct children of one task.
    #[arg(long, value_name = "ID")]
    pub parent: Option<String>,
}

/// Mutually exclusive sources for Markdown content.
#[derive(Clone, Debug, Default, Args)]
pub struct MarkdownArgs {
    /// Use this inline Markdown body.
    #[arg(
        short = 'b',
        long,
        value_name = "MARKDOWN",
        allow_hyphen_values = true,
        conflicts_with = "body_file"
    )]
    pub body: Option<String>,
    /// Read UTF-8 Markdown from a file, or `-` for standard input.
    #[arg(long, value_name = "PATH|-", conflicts_with = "body")]
    pub body_file: Option<PathBuf>,
}

/// Mutually exclusive sources for specification acceptance criteria.
#[derive(Clone, Debug, Default, Args)]
pub struct AcceptanceArgs {
    /// Use inline Markdown acceptance criteria.
    #[arg(
        long,
        value_name = "MARKDOWN",
        allow_hyphen_values = true,
        conflicts_with = "acceptance_criteria_file"
    )]
    pub acceptance_criteria: Option<String>,
    /// Read acceptance criteria from a file, or `-` for standard input.
    #[arg(long, value_name = "PATH|-", conflicts_with = "acceptance_criteria")]
    pub acceptance_criteria_file: Option<PathBuf>,
}

/// Arc Lightning's command-line options.
#[derive(Debug, Parser)]
#[command(
    name = "arcl",
    version,
    about = "Arc Lightning: local-first project planning for developers and coding agents",
    after_help = r#"Examples:

    arcl init
    arcl init --workspace
    arcl --help
    arcl --version
    arcl capture create "Improve import errors" --body "Make failures easier to fix."
    arcl capture promote arcl-c-… spec
    arcl capture promote arcl-c-… task --priority high
    arcl plan create "Import validation" --spec arcl-s-…
    arcl task create "Validate records" --plan arcl-pl-…
    arcl ready
    arcl task start arcl-t-… --claim
    arcl next --start --claim
    arcl list --assignee codex
    arcl serve
"#
)]
pub struct Cli {
    /// Choose when human output may use ANSI colors.
    #[arg(long, global = true, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,

    /// Render a versioned JSON envelope.
    #[arg(long, global = true, conflicts_with_all = ["plain", "quiet"])]
    pub json: bool,

    /// Render one stable, unstyled record per line.
    #[arg(long, global = true, conflicts_with_all = ["json", "quiet"])]
    pub plain: bool,

    /// Suppress explanatory output.
    #[arg(long, global = true, conflicts_with_all = ["json", "plain"])]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}
