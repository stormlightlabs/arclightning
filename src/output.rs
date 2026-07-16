use std::path::Path;

use owo_colors::{OwoColorize, Stream};
use serde::Serialize;
use thiserror::Error;

use crate::cli::ColorChoice;
use crate::domain::{Epic, Idea, Milestone, Release, Task, TaskDependency};

/// The output format for a command result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputMode {
    Human,
    Plain,
    Json,
    Quiet,
}

/// The mutation represented by an idea command's output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdeaMutation {
    Created,
    Updated,
    Discarded,
}

/// The mutation represented by a release command's output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseMutation {
    Created,
    Updated,
    Completed,
    Cancelled,
}

impl ReleaseMutation {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }

    const fn human_verb(self) -> &'static str {
        match self {
            Self::Created => "Created",
            Self::Updated => "Updated",
            Self::Completed => "Completed",
            Self::Cancelled => "Cancelled",
        }
    }
}

/// The mutation represented by an epic command's output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EpicMutation {
    Created,
    Updated,
    Completed,
    Cancelled,
}

/// The mutation represented by a milestone command's output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MilestoneMutation {
    Created,
    Updated,
    Completed,
    Cancelled,
}

/// The mutation represented by a task command's output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskMutation {
    Created,
    Updated,
    Started,
    Parked,
    Unparked,
    Completed,
    Cancelled,
}

/// The relationship change represented by a dependency command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DependencyMutation {
    Added,
    Removed,
}

impl DependencyMutation {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Removed => "removed",
        }
    }

    const fn verb(self) -> &'static str {
        match self {
            Self::Added => "Added",
            Self::Removed => "Removed",
        }
    }
}

impl EpicMutation {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }

    const fn verb(self) -> &'static str {
        match self {
            Self::Created => "Created",
            Self::Updated => "Updated",
            Self::Completed => "Completed",
            Self::Cancelled => "Cancelled",
        }
    }
}

impl MilestoneMutation {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }

    const fn verb(self) -> &'static str {
        match self {
            Self::Created => "Created",
            Self::Updated => "Updated",
            Self::Completed => "Completed",
            Self::Cancelled => "Cancelled",
        }
    }
}

impl TaskMutation {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Started => "started",
            Self::Parked => "parked",
            Self::Unparked => "unparked",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }

    const fn verb(self) -> &'static str {
        match self {
            Self::Created => "Created",
            Self::Updated => "Updated",
            Self::Started => "Started",
            Self::Parked => "Parked",
            Self::Unparked => "Unparked",
            Self::Completed => "Completed",
            Self::Cancelled => "Cancelled",
        }
    }
}

impl IdeaMutation {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Discarded => "discarded",
        }
    }

    const fn verb(self) -> &'static str {
        match self {
            Self::Created => "Created",
            Self::Updated => "Updated",
            Self::Discarded => "Discarded",
        }
    }
}

/// Output failures that can occur before the application boundary adds context.
#[derive(Debug, Error)]
pub enum OutputError {
    #[error("could not serialize JSON output: {0}")]
    Json(#[from] serde_json::Error),
}

/// The renderer used by the application boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Renderer {
    mode: OutputMode,
    color: ColorChoice,
}

impl Renderer {
    pub const fn new(mode: OutputMode, color: ColorChoice) -> Self {
        Self { mode, color }
    }

    /// Render the result of starting the foundation-only application.
    pub fn render_startup(&self) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => self.msg(),
            OutputMode::Plain => "ready".to_owned(),
            OutputMode::Json => serde_json::to_string(&Envelope {
                format_version: 1,
                data: StartupData { status: "ready", message: "Arc Lightning foundation ready" },
            })?,
            OutputMode::Quiet => return Ok(None),
        };

        Ok(Some(message))
    }

    /// Render the result of project initialization.
    pub fn render_init(&self, root: &Path, snapshot_enabled: bool) -> Result<Option<String>, OutputError> {
        let root = root.to_string_lossy();
        let message = match self.mode {
            OutputMode::Human => {
                let snapshot = if snapshot_enabled { " with snapshots enabled" } else { "" };
                format!("Initialized Arc Lightning in `{root}`{snapshot}")
            }
            OutputMode::Plain => format!("initialized\t{root}"),
            OutputMode::Json => serde_json::to_string(&Envelope {
                format_version: 1,
                data: InitData { status: "initialized", root: root.as_ref(), snapshot_enabled },
            })?,
            OutputMode::Quiet => return Ok(None),
        };

        Ok(Some(match (self.mode, self.color) {
            (OutputMode::Human, ColorChoice::Always) => message.bright_blue().to_string(),
            (OutputMode::Human, ColorChoice::Auto) => message
                .if_supports_color(Stream::Stdout, |text| text.bright_blue())
                .to_string(),
            _ => message,
        }))
    }

    /// Render one idea mutation and include the affected record in JSON mode.
    pub fn render_idea(&self, mutation: IdeaMutation, idea: &Idea) -> Result<Option<String>, OutputError> {
        let id = idea.id.to_string();
        let message = match self.mode {
            OutputMode::Human => match mutation {
                IdeaMutation::Discarded => format!("Discarded idea `{id}`"),
                _ => format!("{} idea `{id}`: {}", mutation.verb(), idea.title),
            },
            OutputMode::Plain | OutputMode::Quiet => id,
            OutputMode::Json => serde_json::to_string(&Envelope {
                format_version: 1,
                data: IdeaMutationData { action: mutation.as_str(), idea },
            })?,
        };

        Ok(Some(match (self.mode, self.color) {
            (OutputMode::Human, ColorChoice::Always) => message.bright_blue().to_string(),
            (OutputMode::Human, ColorChoice::Auto) => message
                .if_supports_color(Stream::Stdout, |text| text.bright_blue())
                .to_string(),
            _ => message,
        }))
    }

    /// Render the current idea inbox.
    pub fn render_ideas(&self, ideas: &[Idea]) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => {
                if ideas.is_empty() {
                    "No ideas.".to_owned()
                } else {
                    ideas
                        .iter()
                        .map(|idea| format!("{}\t[{}]\t{}", idea.id, idea.status.as_str(), idea.title))
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            }
            OutputMode::Plain => ideas
                .iter()
                .map(|idea| idea.id.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
            OutputMode::Json => serde_json::to_string(&Envelope { format_version: 1, data: IdeaListData { ideas } })?,
            OutputMode::Quiet => return Ok(None),
        };

        Ok(Some(match (self.mode, self.color) {
            (OutputMode::Human, ColorChoice::Always) => message.bright_blue().to_string(),
            (OutputMode::Human, ColorChoice::Auto) => message
                .if_supports_color(Stream::Stdout, |text| text.bright_blue())
                .to_string(),
            _ => message,
        }))
    }

    /// Render one release mutation and include the affected record in JSON mode.
    pub fn render_release(&self, mutation: ReleaseMutation, release: &Release) -> Result<Option<String>, OutputError> {
        let id = release.id.to_string();
        let message = match self.mode {
            OutputMode::Human => format!("{} release `{id}`: {}", mutation.human_verb(), release.title),
            OutputMode::Plain | OutputMode::Quiet => id,
            OutputMode::Json => serde_json::to_string(&Envelope {
                format_version: 1,
                data: ReleaseMutationData { action: mutation.as_str(), release },
            })?,
        };

        Ok(Some(match (self.mode, self.color) {
            (OutputMode::Human, ColorChoice::Always) => message.bright_blue().to_string(),
            (OutputMode::Human, ColorChoice::Auto) => message
                .if_supports_color(Stream::Stdout, |text| text.bright_blue())
                .to_string(),
            _ => message,
        }))
    }

    /// Render one epic mutation and include the affected record in JSON mode.
    pub fn render_epic(&self, mutation: EpicMutation, epic: &Epic) -> Result<Option<String>, OutputError> {
        let id = epic.id.to_string();
        let message = match self.mode {
            OutputMode::Human => format!("{} epic `{id}`: {}", mutation.verb(), epic.title),
            OutputMode::Plain | OutputMode::Quiet => id,
            OutputMode::Json => serde_json::to_string(&Envelope {
                format_version: 1,
                data: EpicMutationData { action: mutation.as_str(), epic },
            })?,
        };

        Ok(Some(match (self.mode, self.color) {
            (OutputMode::Human, ColorChoice::Always) => message.bright_blue().to_string(),
            (OutputMode::Human, ColorChoice::Auto) => message
                .if_supports_color(Stream::Stdout, |text| text.bright_blue())
                .to_string(),
            _ => message,
        }))
    }

    /// Render one milestone mutation and include the affected record in JSON mode.
    pub fn render_milestone(
        &self, mutation: MilestoneMutation, milestone: &Milestone,
    ) -> Result<Option<String>, OutputError> {
        let id = milestone.id.to_string();
        let message = match self.mode {
            OutputMode::Human => format!("{} milestone `{id}`: {}", mutation.verb(), milestone.title),
            OutputMode::Plain | OutputMode::Quiet => id,
            OutputMode::Json => serde_json::to_string(&Envelope {
                format_version: 1,
                data: MilestoneMutationData { action: mutation.as_str(), milestone },
            })?,
        };

        Ok(Some(match (self.mode, self.color) {
            (OutputMode::Human, ColorChoice::Always) => message.bright_blue().to_string(),
            (OutputMode::Human, ColorChoice::Auto) => message
                .if_supports_color(Stream::Stdout, |text| text.bright_blue())
                .to_string(),
            _ => message,
        }))
    }

    /// Render one task mutation and include the affected record in JSON mode.
    pub fn render_task(&self, mutation: TaskMutation, task: &Task) -> Result<Option<String>, OutputError> {
        let id = task.id.to_string();
        let message = match self.mode {
            OutputMode::Human => format!("{} task `{id}`: {}", mutation.verb(), task.title),
            OutputMode::Plain | OutputMode::Quiet => id,
            OutputMode::Json => serde_json::to_string(&Envelope {
                format_version: 1,
                data: TaskMutationData { action: mutation.as_str(), task },
            })?,
        };

        Ok(Some(match (self.mode, self.color) {
            (OutputMode::Human, ColorChoice::Always) => message.bright_blue().to_string(),
            (OutputMode::Human, ColorChoice::Auto) => message
                .if_supports_color(Stream::Stdout, |text| text.bright_blue())
                .to_string(),
            _ => message,
        }))
    }

    /// Render one dependency relationship change.
    pub fn render_dependency(
        &self, mutation: DependencyMutation, dependency: &TaskDependency,
    ) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => format!(
                "{} dependency `{}` blocked by `{}`",
                mutation.verb(),
                dependency.task_id,
                dependency.blocker_id
            ),
            OutputMode::Plain | OutputMode::Quiet => dependency.task_id.to_string(),
            OutputMode::Json => serde_json::to_string(&Envelope {
                format_version: 1,
                data: DependencyMutationData { action: mutation.as_str(), dependency },
            })?,
        };

        Ok(Some(match (self.mode, self.color) {
            (OutputMode::Human, ColorChoice::Always) => message.bright_blue().to_string(),
            (OutputMode::Human, ColorChoice::Auto) => message
                .if_supports_color(Stream::Stdout, |text| text.bright_blue())
                .to_string(),
            _ => message,
        }))
    }

    /// Render all derived ready-work results.
    pub fn render_ready_tasks(&self, tasks: &[Task]) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => {
                if tasks.is_empty() {
                    "No ready work.".to_owned()
                } else {
                    tasks
                        .iter()
                        .map(|task| {
                            format!(
                                "{}\t[{}]\t{}\t{}",
                                task.id,
                                task.priority.as_str(),
                                task.status.as_str(),
                                task.title
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            }
            OutputMode::Plain => {
                if tasks.is_empty() {
                    return Ok(None);
                }
                tasks
                    .iter()
                    .map(|task| task.id.to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            OutputMode::Json => serde_json::to_string(&Envelope { format_version: 1, data: ReadyData { tasks } })?,
            OutputMode::Quiet => return Ok(None),
        };

        Ok(Some(match (self.mode, self.color) {
            (OutputMode::Human, ColorChoice::Always) => message.bright_blue().to_string(),
            (OutputMode::Human, ColorChoice::Auto) => message
                .if_supports_color(Stream::Stdout, |text| text.bright_blue())
                .to_string(),
            _ => message,
        }))
    }

    /// Render the first ready task or a stable empty result.
    pub fn render_next_task(&self, task: Option<&Task>) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => task.map_or_else(
                || "No ready work.".to_owned(),
                |task| {
                    format!(
                        "{}\t[{}]\t{}\t{}",
                        task.id,
                        task.priority.as_str(),
                        task.status.as_str(),
                        task.title
                    )
                },
            ),
            OutputMode::Plain => task.map(|task| task.id.to_string()).unwrap_or_default(),
            OutputMode::Json => serde_json::to_string(&Envelope { format_version: 1, data: NextData { task } })?,
            OutputMode::Quiet => return Ok(None),
        };

        if self.mode == OutputMode::Plain && task.is_none() {
            return Ok(None);
        }
        Ok(Some(match (self.mode, self.color) {
            (OutputMode::Human, ColorChoice::Always) => message.bright_blue().to_string(),
            (OutputMode::Human, ColorChoice::Auto) => message
                .if_supports_color(Stream::Stdout, |text| text.bright_blue())
                .to_string(),
            _ => message,
        }))
    }

    fn msg(&self) -> String {
        const MESSAGE: &str = "Arc Lightning foundation ready";
        match self.color {
            ColorChoice::Always => MESSAGE.bright_blue().to_string(),
            ColorChoice::Never => MESSAGE.to_owned(),
            ColorChoice::Auto => MESSAGE
                .if_supports_color(Stream::Stdout, |text| text.bright_blue())
                .to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
struct Envelope<T> {
    format_version: u8,
    data: T,
}

#[derive(Debug, Serialize)]
struct StartupData {
    status: &'static str,
    message: &'static str,
}

#[derive(Debug, Serialize)]
struct InitData<'a> {
    status: &'static str,
    root: &'a str,
    snapshot_enabled: bool,
}

#[derive(Debug, Serialize)]
struct IdeaMutationData<'a> {
    action: &'static str,
    idea: &'a Idea,
}

#[derive(Debug, Serialize)]
struct IdeaListData<'a> {
    ideas: &'a [Idea],
}

#[derive(Debug, Serialize)]
struct ReleaseMutationData<'a> {
    action: &'static str,
    release: &'a Release,
}

#[derive(Debug, Serialize)]
struct EpicMutationData<'a> {
    action: &'static str,
    epic: &'a Epic,
}

#[derive(Debug, Serialize)]
struct MilestoneMutationData<'a> {
    action: &'static str,
    milestone: &'a Milestone,
}

#[derive(Debug, Serialize)]
struct TaskMutationData<'a> {
    action: &'static str,
    task: &'a Task,
}

#[derive(Debug, Serialize)]
struct DependencyMutationData<'a> {
    action: &'static str,
    dependency: &'a TaskDependency,
}

#[derive(Debug, Serialize)]
struct ReadyData<'a> {
    tasks: &'a [Task],
}

#[derive(Debug, Serialize)]
struct NextData<'a> {
    task: Option<&'a Task>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::ColorChoice;

    #[test]
    fn plain_output_is_stable_and_unstyled() {
        let rendered = Renderer::new(OutputMode::Plain, ColorChoice::Always)
            .render_startup()
            .expect("rendering succeeds")
            .expect("plain output is not quiet");

        assert_eq!(rendered, "ready");
        assert!(!rendered.contains('\u{1b}'));
    }

    #[test]
    fn quiet_output_is_empty() {
        assert_eq!(
            Renderer::new(OutputMode::Quiet, ColorChoice::Always)
                .render_startup()
                .expect("rendering succeeds"),
            None
        );
    }

    #[test]
    fn idea_json_output_is_versioned_and_contains_the_record() {
        let idea = Idea::new("A thought".to_owned(), "Details".to_owned()).expect("idea is valid");
        let rendered = Renderer::new(OutputMode::Json, ColorChoice::Always)
            .render_idea(IdeaMutation::Created, &idea)
            .expect("rendering succeeds")
            .expect("JSON output is present");

        assert!(rendered.contains("\"format_version\":1"));
        assert!(rendered.contains(&idea.id.to_string()));
        assert!(!rendered.contains('\u{1b}'));
    }
}
