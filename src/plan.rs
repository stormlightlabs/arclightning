use std::collections::HashSet;

use serde::Deserialize;
use thiserror::Error;

use crate::domain::TaskPriority;

pub const PLAN_FORMAT_VERSION: u32 = 1;

type Result<T> = std::result::Result<T, PlanError>;

/// Errors found while parsing or validating a plan document.
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
}

/// A complete, additive plan document before it is applied to storage.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PlanDocument {
    #[serde(rename = "format-version")]
    pub format_version: u32,
    #[serde(default)]
    pub milestones: Vec<PlanMilestone>,
}

impl PlanDocument {
    pub fn validate(&self) -> Result<()> {
        if self.format_version != PLAN_FORMAT_VERSION {
            return Err(PlanError::UnsupportedVersion { found: self.format_version, expected: PLAN_FORMAT_VERSION });
        }

        let mut milestone_keys = HashSet::new();
        for milestone in &self.milestones {
            validate_key_and_title("milestone", &milestone.key, &milestone.title)?;
            if !milestone_keys.insert(&milestone.key) {
                return Err(PlanError::DuplicateKey { kind: "milestone", key: milestone.key.clone() });
            }
            validate_tasks(&milestone.tasks, &milestone.key)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PlanMilestone {
    pub key: String,
    pub title: String,
    pub position: u32,
    #[serde(default)]
    pub tasks: Vec<PlanTask>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
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

/// Parse and validate a plan in one operation.
pub fn parse(input: &str) -> Result<PlanDocument> {
    let document = toml::from_str::<PlanDocument>(input)?;
    document.validate()?;
    Ok(document)
}

fn validate_tasks(tasks: &[PlanTask], milestone_key: &str) -> Result<()> {
    let mut keys = HashSet::new();
    for task in tasks {
        validate_key_and_title("task", &task.key, &task.title)?;
        if !keys.insert(&task.key) {
            return Err(PlanError::DuplicateKey { kind: "task", key: task.key.clone() });
        }
        for dependency in &task.blocked_by {
            if dependency.trim().is_empty() {
                return Err(PlanError::EmptyDependency { task: format!("{milestone_key}/{}", task.key) });
            }
        }
        validate_tasks(&task.subtasks, milestone_key)?;
    }
    Ok(())
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
