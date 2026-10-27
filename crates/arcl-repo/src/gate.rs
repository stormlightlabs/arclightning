//! Resolve open gates: date gates against today's UTC date, pull request and
//! run gates through the GitHub CLI, `gh`.

use std::{
    io,
    process::{Command, Stdio},
};

use serde::{Deserialize, Serialize};

use arcl_core::domain::{Gate, GateKind, GateStatus, GithubUrl, TaskId};
use arcl_store::{Database, StorageError};

/// The program a pull request or run gate check runs, found on `PATH`.
pub const GH_PROGRAM: &str = "gh";

/// The detail a skipped check reports when `gh` is not on `PATH`.
pub const GH_MISSING: &str = "gh not found; PR/run gates left open";

/// What one gate check found.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum GateOutcome {
    /// The condition holds, and the gate is now met.
    Met,
    /// The condition does not hold yet: the date is ahead, the pull request
    /// is open or the run has not finished. The gate stays open.
    Waiting,
    /// The condition cannot hold any more: the pull request closed without
    /// merging or the run finished without succeeding. The gate stays open
    /// until someone removes it.
    Failed,
    /// The check could not run or its answer could not be read. The gate
    /// stays open.
    Error,
    /// `gh` is not on `PATH`, so the pull request or run gate was not
    /// checked. The gate stays open.
    Skipped,
}

impl GateOutcome {
    /// Return the stable value JSON and plain output use.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Met => "met",
            Self::Waiting => "waiting",
            Self::Failed => "failed",
            Self::Error => "error",
            Self::Skipped => "skipped",
        }
    }
}

/// Why `gh` gave no usable answer.
enum ProbeError {
    /// `gh` is not on `PATH`.
    Missing,
    /// `gh` ran and failed, or its output could not be read.
    Failed(String),
}

/// One checked gate: the gate as it is after the check, what the check
/// found, and a short detail such as `merged` or `run concluded failure`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct GateCheck {
    /// The gate after the check; met when `outcome` is `met`.
    pub gate: Gate,
    /// What the check found.
    pub outcome: GateOutcome,
    /// What the check saw, or why it failed.
    pub detail: String,
}

/// The result of checking every open gate in scope.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct GateCheckReport {
    /// One entry per open gate checked, in task ID order, each task's gates
    /// oldest first.
    pub checks: Vec<GateCheck>,
    /// Whether `gh` was missing, so pull request and run gates were skipped.
    pub gh_missing: bool,
}

/// The answer `gh` gave about a pull request or run.
#[derive(Debug)]
struct Probe {
    outcome: GateOutcome,
    detail: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PullRequestView {
    state: String,
    #[serde(default)]
    merged_at: Option<String>,
}

#[derive(Deserialize)]
struct RunView {
    status: String,
    #[serde(default)]
    conclusion: Option<String>,
}

/// Check every open gate on task `task`, or on every task, and mark the ones
/// whose condition holds as met.
///
/// A date gate is met once `today`, a `YYYY-MM-DD` date in UTC, reaches its
/// date; it needs no network. A pull request gate runs `gh pr view <url>
/// --json state,mergedAt` and is met once the pull request merged. A run
/// gate runs `gh run view <id> --repo <host/owner/repo> --json
/// status,conclusion` and is met once the run completed with `success`.
///
/// One gate's failure never stops the others: it is reported as that gate's
/// [`GateOutcome::Error`]. When `gh` is not on `PATH`, pull request and run
/// gates are [`GateOutcome::Skipped`] and the report sets `gh_missing`.
/// Only reading the gates can fail the whole check.
pub fn check_gates(
    database: &mut Database, task: Option<TaskId>, today: &str,
) -> Result<GateCheckReport, StorageError> {
    let open = database
        .gates(task)?
        .into_iter()
        .filter(|gate| gate.status == GateStatus::Open)
        .collect::<Vec<_>>();
    let mut report = GateCheckReport::default();
    for gate in open {
        let probe = match gate.kind {
            GateKind::Date => Ok(date_probe(&gate.target, today)),
            GateKind::Pr | GateKind::Run if report.gh_missing => Err(ProbeError::Missing),
            GateKind::Pr => pull_request_probe(&gate.target),
            GateKind::Run => run_probe(&gate.target),
        };
        let probe = match probe {
            Ok(probe) => probe,
            Err(ProbeError::Missing) => {
                report.gh_missing = true;
                Probe { outcome: GateOutcome::Skipped, detail: GH_MISSING.to_owned() }
            }
            Err(ProbeError::Failed(detail)) => Probe { outcome: GateOutcome::Error, detail },
        };
        let check = if probe.outcome == GateOutcome::Met {
            match database.meet_gate(gate.id) {
                Ok(met) => GateCheck { gate: met, outcome: GateOutcome::Met, detail: probe.detail },
                Err(error) => GateCheck {
                    gate,
                    outcome: GateOutcome::Error,
                    detail: format!("could not mark the gate met: {error}"),
                },
            }
        } else {
            GateCheck { gate, outcome: probe.outcome, detail: probe.detail }
        };
        report.checks.push(check);
    }
    Ok(report)
}

/// Compare a date gate's `target` with `today`; both are `YYYY-MM-DD`, so
/// they order as strings.
fn date_probe(target: &str, today: &str) -> Probe {
    if today >= target {
        Probe { outcome: GateOutcome::Met, detail: format!("{target} has arrived") }
    } else {
        Probe { outcome: GateOutcome::Waiting, detail: format!("waits until {target}; today is {today} UTC") }
    }
}

fn pull_request_probe(url: &str) -> Result<Probe, ProbeError> {
    let output = run_gh(&["pr", "view", url, "--json", "state,mergedAt"])?;
    pull_request_outcome(&output).map_err(ProbeError::Failed)
}

fn run_probe(url: &str) -> Result<Probe, ProbeError> {
    let run = GithubUrl::run(url).ok_or_else(|| ProbeError::Failed(format!("`{url}` is not a run URL")))?;
    let id = run.number.to_string();
    let repository = run.repository();
    let output = run_gh(&["run", "view", &id, "--repo", &repository, "--json", "status,conclusion"])?;
    run_outcome(&output).map_err(ProbeError::Failed)
}

/// Read `gh pr view --json state,mergedAt` output.
fn pull_request_outcome(output: &str) -> Result<Probe, String> {
    let view: PullRequestView =
        serde_json::from_str(output).map_err(|error| format!("could not read gh output: {error}"))?;
    let merged = view.state.eq_ignore_ascii_case("merged") || view.merged_at.is_some_and(|at| !at.is_empty());
    Ok(if merged {
        Probe { outcome: GateOutcome::Met, detail: "merged".to_owned() }
    } else if view.state.eq_ignore_ascii_case("closed") {
        Probe { outcome: GateOutcome::Failed, detail: "closed without merging".to_owned() }
    } else {
        Probe { outcome: GateOutcome::Waiting, detail: format!("pull request is {}", view.state.to_lowercase()) }
    })
}

/// Read `gh run view --json status,conclusion` output.
fn run_outcome(output: &str) -> Result<Probe, String> {
    let view: RunView = serde_json::from_str(output).map_err(|error| format!("could not read gh output: {error}"))?;
    if !view.status.eq_ignore_ascii_case("completed") {
        return Ok(Probe { outcome: GateOutcome::Waiting, detail: format!("run is {}", view.status.to_lowercase()) });
    }
    let conclusion = view.conclusion.unwrap_or_default().to_lowercase();
    Ok(if conclusion == "success" {
        Probe { outcome: GateOutcome::Met, detail: "run succeeded".to_owned() }
    } else {
        Probe { outcome: GateOutcome::Failed, detail: format!("run concluded {conclusion}") }
    })
}

/// Run `gh` with `args` and return its standard output. Standard input is
/// closed and prompts are off, so `gh` never waits on the terminal.
fn run_gh(args: &[&str]) -> Result<String, ProbeError> {
    let output = Command::new(GH_PROGRAM)
        .args(args)
        .env("GH_PROMPT_DISABLED", "1")
        .stdin(Stdio::null())
        .output()
        .map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => ProbeError::Missing,
            _ => ProbeError::Failed(format!("could not run gh: {error}")),
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let reason = stderr
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("no error output");
        return Err(ProbeError::Failed(format!("gh failed ({}): {reason}", output.status)));
    }
    String::from_utf8(output.stdout).map_err(|_| ProbeError::Failed("gh output is not UTF-8".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_gates_are_met_on_and_after_their_date() {
        assert_eq!(date_probe("2026-10-27", "2026-10-27").outcome, GateOutcome::Met);
        assert_eq!(date_probe("2026-10-26", "2026-10-27").outcome, GateOutcome::Met);
        assert_eq!(date_probe("2026-10-28", "2026-10-27").outcome, GateOutcome::Waiting);
    }

    #[test]
    fn pull_request_states_map_to_outcomes() {
        let outcome = |json: &str| pull_request_outcome(json).map(|probe| probe.outcome);
        assert_eq!(
            outcome(r#"{"state":"MERGED","mergedAt":"2026-10-27T10:00:00Z"}"#),
            Ok(GateOutcome::Met)
        );
        assert_eq!(outcome(r#"{"state":"OPEN","mergedAt":null}"#), Ok(GateOutcome::Waiting));
        assert_eq!(
            outcome(r#"{"state":"CLOSED","mergedAt":null}"#),
            Ok(GateOutcome::Failed)
        );
        assert!(outcome("not json").is_err());
    }

    #[test]
    fn run_states_map_to_outcomes() {
        let outcome = |json: &str| run_outcome(json).map(|probe| probe.outcome);
        assert_eq!(
            outcome(r#"{"status":"completed","conclusion":"success"}"#),
            Ok(GateOutcome::Met)
        );
        assert_eq!(
            outcome(r#"{"status":"completed","conclusion":"failure"}"#),
            Ok(GateOutcome::Failed)
        );
        assert_eq!(
            outcome(r#"{"status":"in_progress","conclusion":""}"#),
            Ok(GateOutcome::Waiting)
        );
        assert!(outcome(r#"{"conclusion":"success"}"#).is_err());
    }

    #[test]
    fn a_check_meets_due_date_gates_and_leaves_the_rest() {
        let mut database = Database::open_in_memory().expect("database opens");
        let project_id = database.project().expect("project exists").id;
        let task = database
            .create_planning_task(arcl_store::PlanningTaskCreate {
                project_id,
                spec_id: None,
                plan_id: None,
                phase_id: None,
                parent_id: None,
                title: "Work".to_owned(),
                body: String::new(),
                priority: arcl_core::domain::TaskPriority::Normal,
                position: 0,
            })
            .expect("task creates");
        let past = database
            .add_gate(task.id, GateKind::Date, "2026-10-01")
            .expect("gate adds");
        let future = database
            .add_gate(task.id, GateKind::Date, "2026-12-01")
            .expect("gate adds");

        let report = check_gates(&mut database, Some(task.id), "2026-10-27").expect("check runs");
        assert!(!report.gh_missing);
        let outcomes = report
            .checks
            .iter()
            .map(|check| (check.gate.id, check.outcome))
            .collect::<Vec<_>>();
        assert_eq!(
            outcomes,
            [(past.id, GateOutcome::Met), (future.id, GateOutcome::Waiting)]
        );
        assert_eq!(report.checks[0].gate.status, GateStatus::Met);

        let again = check_gates(&mut database, None, "2026-10-27").expect("check runs");
        assert_eq!(again.checks.len(), 1, "a met gate is not checked again");
    }
}
