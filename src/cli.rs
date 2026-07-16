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

/// Arc Lightning commands.
#[derive(Clone, Debug, Subcommand)]
pub enum Command {
    /// Initialize or rediscover the project in the enclosing Git worktree.
    Init {
        /// Enable the optional version-controlled snapshot path when creating configuration.
        #[arg(long)]
        snapshot: bool,
    },
    /// Capture and manage ideas in the project inbox.
    Idea {
        #[command(subcommand)]
        command: IdeaCommand,
    },
    /// Group epics into releases.
    Release {
        #[command(subcommand)]
        command: ReleaseCommand,
    },
    /// Track one Markdown specification as an epic.
    Epic {
        #[command(subcommand)]
        command: EpicCommand,
    },
    /// Organize an epic into ordered milestones.
    Milestone {
        #[command(subcommand)]
        command: MilestoneCommand,
    },
    /// Track tasks and subtasks inside milestones.
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
}

/// Idea inbox commands.
#[derive(Clone, Debug, Subcommand)]
pub enum IdeaCommand {
    /// Capture a new idea.
    Create {
        /// The idea title.
        title: String,
        #[command(flatten)]
        description: DescriptionArgs,
    },
    /// Update an existing idea.
    Update {
        /// The idea ID to update.
        id: String,
        /// Replace the idea title.
        #[arg(long, value_name = "TITLE")]
        title: Option<String>,
        #[command(flatten)]
        description: DescriptionArgs,
    },
    /// Discard an idea without deleting its history.
    Discard {
        /// The idea ID to discard.
        id: String,
    },
    /// List all ideas in the project inbox.
    List,
}

/// Release container commands.
#[derive(Clone, Debug, Subcommand)]
pub enum ReleaseCommand {
    /// Create an open release.
    Create {
        /// The release title.
        title: String,
        #[command(flatten)]
        description: DescriptionArgs,
    },
    /// Update a release title or Markdown description.
    Update {
        /// The release ID to update.
        id: String,
        /// Replace the release title.
        #[arg(long, value_name = "TITLE")]
        title: Option<String>,
        #[command(flatten)]
        description: DescriptionArgs,
    },
    /// Mark a release completed.
    Complete {
        /// The release ID to complete.
        id: String,
        /// Complete only the release even when descendants remain open.
        #[arg(long)]
        allow_open_children: bool,
    },
    /// Mark a release cancelled.
    Cancel {
        /// The release ID to cancel.
        id: String,
        /// Cancel only the release even when descendants remain open.
        #[arg(long)]
        allow_open_children: bool,
    },
}

/// Spec-backed epic commands.
#[derive(Clone, Debug, Subcommand)]
pub enum EpicCommand {
    /// Create an open epic for an existing Markdown spec.
    Create {
        /// The epic title.
        title: String,
        /// The Markdown spec path, relative to the current directory.
        #[arg(long, value_name = "PATH")]
        spec: PathBuf,
        /// Associate the epic with a release.
        #[arg(long, value_name = "ID")]
        release: Option<String>,
        #[command(flatten)]
        description: DescriptionArgs,
    },
    /// Update an epic without modifying its linked spec.
    Update {
        /// The epic ID to update.
        id: String,
        /// Replace the epic title.
        #[arg(long, value_name = "TITLE")]
        title: Option<String>,
        /// Replace the linked Markdown spec.
        #[arg(long, value_name = "PATH")]
        spec: Option<PathBuf>,
        /// Associate the epic with a release.
        #[arg(long, value_name = "ID", conflicts_with = "no_release")]
        release: Option<String>,
        /// Remove the epic's release association.
        #[arg(long, conflicts_with = "release")]
        no_release: bool,
        #[command(flatten)]
        description: DescriptionArgs,
    },
    /// Mark an epic completed.
    Complete {
        /// The epic ID to complete.
        id: String,
        /// Complete only the epic even when descendants remain open.
        #[arg(long)]
        allow_open_children: bool,
    },
    /// Mark an epic cancelled.
    Cancel {
        /// The epic ID to cancel.
        id: String,
        /// Cancel only the epic even when descendants remain open.
        #[arg(long)]
        allow_open_children: bool,
    },
}

/// Milestone container commands.
#[derive(Clone, Debug, Subcommand)]
pub enum MilestoneCommand {
    /// Create an open milestone owned by an epic.
    Create {
        /// The milestone title.
        title: String,
        /// The owning epic ID.
        #[arg(long, value_name = "ID")]
        epic: String,
        /// The display position within the epic.
        #[arg(long, default_value_t = 0, value_name = "N")]
        position: i64,
        #[command(flatten)]
        description: DescriptionArgs,
    },
    /// Update a milestone's title, Markdown description, or position.
    Update {
        /// The milestone ID to update.
        id: String,
        /// Replace the milestone title.
        #[arg(long, value_name = "TITLE")]
        title: Option<String>,
        /// Replace the display position within the epic.
        #[arg(long, value_name = "N")]
        position: Option<i64>,
        #[command(flatten)]
        description: DescriptionArgs,
    },
    /// Mark a milestone completed.
    Complete {
        /// The milestone ID to complete.
        id: String,
        /// Complete only the milestone even when tasks remain open.
        #[arg(long)]
        allow_open_children: bool,
    },
    /// Mark a milestone cancelled.
    Cancel {
        /// The milestone ID to cancel.
        id: String,
        /// Cancel only the milestone even when tasks remain open.
        #[arg(long)]
        allow_open_children: bool,
    },
}

/// Task and subtask commands.
#[derive(Clone, Debug, Subcommand)]
pub enum TaskCommand {
    /// Create a pending task or subtask.
    Create {
        /// The task title.
        title: String,
        /// The owning milestone ID.
        #[arg(long, value_name = "ID")]
        milestone: String,
        /// Attach the row as a subtask of this task.
        #[arg(long, value_name = "ID")]
        parent: Option<String>,
        /// The task priority.
        #[arg(long, default_value = "normal", value_name = "PRIORITY")]
        priority: String,
        /// The display position within the milestone or parent.
        #[arg(long, default_value_t = 0, value_name = "N")]
        position: i64,
        #[command(flatten)]
        description: DescriptionArgs,
    },
    /// Update task metadata and optionally move a task subtree.
    Update {
        /// The task ID to update.
        id: String,
        /// Replace the task title.
        #[arg(long, value_name = "TITLE")]
        title: Option<String>,
        /// Replace the task priority.
        #[arg(long, value_name = "PRIORITY")]
        priority: Option<String>,
        /// Replace the display position.
        #[arg(long, value_name = "N")]
        position: Option<i64>,
        /// Move the task and descendants to this milestone.
        #[arg(long, value_name = "ID")]
        milestone: Option<String>,
        /// Reparent the task to another task in the same milestone.
        #[arg(long, value_name = "ID", conflicts_with = "no_parent")]
        parent: Option<String>,
        /// Remove the task's parent.
        #[arg(long, conflicts_with = "parent")]
        no_parent: bool,
        #[command(flatten)]
        description: DescriptionArgs,
    },
    /// Start pending work.
    Start {
        /// The task ID to start.
        id: String,
    },
    /// Temporarily exclude pending or in-progress work from readiness.
    Park {
        /// The task ID to park.
        id: String,
    },
    /// Return parked work to pending.
    Unpark {
        /// The task ID to unpark.
        id: String,
    },
    /// Mark work completed.
    Complete {
        /// The task ID to complete.
        id: String,
        /// Complete only this task even when descendants remain open.
        #[arg(long)]
        allow_open_children: bool,
    },
    /// Mark work cancelled.
    Cancel {
        /// The task ID to cancel.
        id: String,
        /// Cancel only this task even when descendants remain open.
        #[arg(long)]
        allow_open_children: bool,
    },
}

/// Mutually exclusive sources for an idea's Markdown description.
#[derive(Clone, Debug, Default, Args)]
pub struct DescriptionArgs {
    /// Use this inline Markdown description.
    #[arg(
        short = 'd',
        long,
        value_name = "MARKDOWN",
        allow_hyphen_values = true,
        conflicts_with = "description_file"
    )]
    pub description: Option<String>,
    /// Read the UTF-8 description from a file, or `-` for standard input.
    #[arg(long, value_name = "PATH|-", conflicts_with = "description")]
    pub description_file: Option<PathBuf>,
}

/// Arc Lightning's command-line options.
#[derive(Debug, Parser)]
#[command(
    name = "arcl",
    version,
    about = "Arc Lightning: a Git-aware task tracker for developers and coding agents",
    after_help = "Examples:

    arcl init
    arcl init --snapshot
    arcl --help
    arcl --version
"
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
