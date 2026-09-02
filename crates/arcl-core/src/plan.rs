use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::TaskPriority;

/// The version of the structured plan input accepted by Arc Lightning.
pub const PLAN_FORMAT_VERSION: u32 = 1;

/// Errors found while parsing or validating a structured plan document.
#[derive(Debug, Error)]
pub enum PlanError {
    #[error("plan TOML is invalid: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("unsupported plan format version {found}; expected {expected}")]
    UnsupportedVersion { found: u32, expected: u32 },
    #[error("{kind} `{key}` has an empty {field}")]
    EmptyField {
        kind: &'static str,
        key: String,
        field: &'static str,
    },
    #[error("duplicate {kind} key `{key}`")]
    DuplicateKey { kind: &'static str, key: String },
    #[error("task `{task}` has an empty dependency reference")]
    EmptyDependency { task: String },
    #[error("task `{task}` references unknown dependency `{dependency}`")]
    UnknownDependency { task: String, dependency: String },
    #[error("task `{task}` references ambiguous dependency `{dependency}`")]
    AmbiguousDependency { task: String, dependency: String },
}

/// A complete plan document before it is applied to storage.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlanDocument {
    #[serde(rename = "format-version")]
    pub format_version: u32,
    /// Phases in the input.
    #[serde(default)]
    pub phases: Vec<PlanPhase>,
}

impl PlanDocument {
    /// Validate the document's version, keys, titles, and task structure.
    pub fn validate(&self) -> Result<()> {
        if self.format_version != PLAN_FORMAT_VERSION {
            return Err(PlanError::UnsupportedVersion { found: self.format_version, expected: PLAN_FORMAT_VERSION });
        }

        let mut phase_keys = HashSet::new();
        let mut task_paths = HashSet::new();
        for phase in &self.phases {
            validate_key_and_title("phase", &phase.key, &phase.title)?;
            if !phase_keys.insert(&phase.key) {
                return Err(PlanError::DuplicateKey { kind: "phase", key: phase.key.clone() });
            }
            validate_tasks(&phase.tasks, &phase.key, None, &mut task_paths)?;
        }
        Ok(())
    }

    /// Return the stable paths used to match phases and tasks during apply.
    pub fn entries(&self) -> Result<PlanEntries> {
        self.validate()?;
        let mut phases = Vec::with_capacity(self.phases.len());
        let mut tasks = Vec::new();
        for phase in &self.phases {
            phases.push(PlanPhaseEntry {
                key: phase.key.clone(),
                title: phase.title.clone(),
                position: phase.position,
            });
            collect_tasks(&phase.tasks, &phase.key, None, &mut tasks);
        }
        Ok(PlanEntries { phases, tasks })
    }
}

/// One ordered phase supplied by structured plan input.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlanPhase {
    pub key: String,
    pub title: String,
    pub position: u32,
    #[serde(default)]
    pub tasks: Vec<PlanTask>,
}

/// One task supplied by structured plan input.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlanTask {
    pub key: String,
    pub title: String,
    #[serde(default)]
    pub priority: Option<TaskPriority>,
    pub position: u32,
    #[serde(default, rename = "blocked-by")]
    pub blocked_by: Vec<String>,
    #[serde(default)]
    pub subtasks: Vec<PlanTask>,
}

impl PlanTask {
    fn task_for(&self) -> &[Self] {
        &self.subtasks
    }
}

/// The normalized, validated contents of a structured plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanEntries {
    /// Phases in the input order.
    pub phases: Vec<PlanPhaseEntry>,
    /// Tasks in preorder, with parents before children.
    pub tasks: Vec<PlanTaskEntry>,
}

/// A phase key and its values from a plan document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanPhaseEntry {
    pub key: String,
    pub title: String,
    pub position: u32,
}

/// A task key, ancestry path, and values from a plan document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanTaskEntry {
    /// The phase key containing this task.
    pub phase_key: String,
    /// The task key as written in the document.
    pub key: String,
    /// The path relative to the plan, including the phase key.
    pub path: String,
    /// The stable path of the parent task, if this is a subtask.
    pub parent_path: Option<String>,
    pub title: String,
    pub priority: TaskPriority,
    pub position: u32,
    /// References to other tasks that must complete first.
    pub blocked_by: Vec<String>,
}

type Result<T> = std::result::Result<T, PlanError>;

/// Parse and validate a plan in one operation.
pub fn parse(input: &str) -> Result<PlanDocument> {
    let document = toml::from_str::<PlanDocument>(input)?;
    document.validate()?;
    Ok(document)
}

fn validate_tasks(
    tasks: &[PlanTask], phase_key: &str, parent_path: Option<&str>, paths: &mut HashSet<String>,
) -> Result<()> {
    let mut sibling_keys = HashSet::new();
    for task in tasks {
        validate_key_and_title("task", &task.key, &task.title)?;
        if !sibling_keys.insert(&task.key) {
            return Err(PlanError::DuplicateKey { kind: "task", key: task.key.clone() });
        }
        let path = task_path(phase_key, parent_path, &task.key);
        if !paths.insert(path.clone()) {
            return Err(PlanError::DuplicateKey { kind: "task", key: path });
        }
        for dependency in &task.blocked_by {
            if dependency.trim().is_empty() {
                return Err(PlanError::EmptyDependency { task: path.clone() });
            }
        }
        validate_tasks(task.task_for(), phase_key, Some(&path), paths)?
    }
    Ok(())
}

fn collect_tasks(tasks: &[PlanTask], phase_key: &str, parent_path: Option<&str>, entries: &mut Vec<PlanTaskEntry>) {
    for task in tasks {
        let path = task_path(phase_key, parent_path, &task.key);
        entries.push(PlanTaskEntry {
            phase_key: phase_key.to_owned(),
            key: task.key.clone(),
            path: path.clone(),
            parent_path: parent_path.map(str::to_owned),
            title: task.title.clone(),
            priority: task.priority.unwrap_or_default(),
            position: task.position,
            blocked_by: task.blocked_by.clone(),
        });
        collect_tasks(task.task_for(), phase_key, Some(&path), entries);
    }
}

fn task_path(phase_key: &str, parent_path: Option<&str>, key: &str) -> String {
    match parent_path {
        Some(parent) => format!("{parent}/{key}"),
        None => format!("{phase_key}/{key}"),
    }
}

fn validate_key_and_title(kind: &'static str, key: &str, title: &str) -> Result<()> {
    if key.trim().is_empty() {
        return Err(PlanError::EmptyField { kind, key: key.to_owned(), field: "key" });
    }
    if title.trim().is_empty() {
        return Err(PlanError::EmptyField { kind, key: key.to_owned(), field: "title" });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{PLAN_FORMAT_VERSION, PlanError, parse};

    #[test]
    fn structured_plans_accept_phase_vocabulary_and_build_stable_paths() {
        let document = parse(
            r#"
format-version = 1

[[phases]]
key = "storage"
title = "Storage"
position = 0

[[phases.tasks]]
key = "schema"
title = "Update schema"
position = 0

[[phases.tasks.subtasks]]
key = "tests"
title = "Add tests"
position = 0
blocked-by = ["storage/schema"]
"#,
        )
        .expect("plan parses");
        let entries = document.entries().expect("entries validate");
        assert_eq!(entries.phases[0].key, "storage");
        assert_eq!(entries.tasks[0].path, "storage/schema");
        assert_eq!(entries.tasks[1].parent_path.as_deref(), Some("storage/schema"));
    }

    #[test]
    fn invalid_plan_input_is_rejected_before_entries_are_built() {
        let document = super::PlanDocument { format_version: PLAN_FORMAT_VERSION, phases: Vec::new() };
        assert!(document.entries().is_ok());
        let invalid = parse("format-version = 2").expect_err("version is invalid");
        assert!(matches!(invalid, PlanError::UnsupportedVersion { .. }));
        assert!(parse("format-version = 1\nmilestones = []").is_err());
    }
}
