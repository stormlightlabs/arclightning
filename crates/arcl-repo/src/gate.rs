//! Resolve open gates: date gates against today's UTC date, pull request and
//! run gates through the GitHub CLI, `gh`.
//!
//! A check runs in three steps so that no lock is held while `gh` asks the
//! network: [`read_open_gates`] reads the gates, [`probe_gates`] asks about
//! each one without the database, and [`record_gate_checks`] marks the met
//! ones in one short transaction. [`check_gates`] runs the three in a row.

use std::{
    io::{self, Read},
    process::{Child, Command, ExitStatus, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use arcl_core::domain::{Gate, GateKind, GateStatus, GithubUrl, TaskId, TaskStatus};
use arcl_store::{Database, StorageError};

/// The program a pull request or run gate check runs, found on `PATH`.
pub const GH_PROGRAM: &str = "gh";

/// The detail a skipped check reports when `gh` is not on `PATH`.
pub const GH_MISSING: &str = "gh not found; PR/run gates left open";

/// How long one `gh` call may run before it is killed and its gate reported
/// as [`GateOutcome::Error`].
pub const GH_TIMEOUT: Duration = Duration::from_secs(30);

/// The environment variable that overrides [`GH_TIMEOUT`] for
/// [`gh_timeout`], in whole milliseconds greater than zero. Tests set it to
/// a short value.
pub const GH_TIMEOUT_ENV: &str = "ARCL_GH_TIMEOUT_MS";

/// How often a running `gh` call is polled for its exit.
const GH_POLL: Duration = Duration::from_millis(10);

/// How long `gh`'s output may stay open once `gh` has exited and its
/// process group is killed.
const GH_DRAIN: Duration = Duration::from_secs(1);

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
    /// The check could not run, timed out, or its answer could not be read.
    /// The gate stays open.
    Error,
    /// The gate was not checked: its task is completed or cancelled, or `gh`
    /// is not on `PATH` for a pull request or run gate. The gate stays open.
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
    /// `gh` ran and failed, timed out, or its output could not be read.
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

/// The open gates in scope, each with its task's status, as
/// [`read_open_gates`] read them for [`probe_gates`].
#[derive(Clone, Debug, Default)]
pub struct OpenGates {
    gates: Vec<(Gate, TaskStatus)>,
}

/// What [`probe_gates`] found about each open gate, for
/// [`record_gate_checks`] to mark the met ones.
#[derive(Debug, Default)]
pub struct ProbedGates {
    probes: Vec<(Gate, Probe)>,
    gh_missing: bool,
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
/// whose condition holds as met: [`read_open_gates`], [`probe_gates`] with
/// `gh_timeout`, then [`record_gate_checks`]. A caller that holds a lock
/// around writes runs the three itself, holding it only for the first and
/// the last.
pub fn check_gates(
    database: &mut Database, task: Option<TaskId>, today: &str, gh_timeout: Duration,
) -> Result<GateCheckReport, StorageError> {
    let open = read_open_gates(database, task)?;
    let probed = probe_gates(open, today, gh_timeout);
    Ok(record_gate_checks(database, probed))
}

/// Read the open gates on task `task`, or on every task, with each gate's
/// task status. Reading is the only step of a check that fails as a whole.
pub fn read_open_gates(database: &Database, task: Option<TaskId>) -> Result<OpenGates, StorageError> {
    let mut statuses = std::collections::HashMap::new();
    let mut gates = Vec::new();
    for gate in database.gates(task)? {
        if gate.status != GateStatus::Open {
            continue;
        }
        let status = match statuses.get(&gate.task_id) {
            Some(status) => *status,
            None => {
                let status = database
                    .planning_task(gate.task_id)?
                    .ok_or_else(|| StorageError::PlanningTaskNotFound { id: gate.task_id.to_string() })?
                    .status;
                statuses.insert(gate.task_id, status);
                status
            }
        };
        gates.push((gate, status));
    }
    Ok(OpenGates { gates })
}

/// Ask about each open gate, without the database.
///
/// A gate on a completed or cancelled task is [`GateOutcome::Skipped`]
/// without being asked about, since its task no longer waits on it. A date
/// gate is met once `today`, a `YYYY-MM-DD` date in UTC, reaches its date;
/// it needs no network. A pull request gate runs `gh pr view <url> --json
/// state,mergedAt` and is met once the pull request merged. A run gate runs
/// `gh run view <id> --repo <host/owner/repo> --json status,conclusion` and
/// is met once the run completed with `success`.
///
/// Each `gh` call may run for `gh_timeout`; past it the call is killed and
/// its gate is [`GateOutcome::Error`] with a `timed out after …` detail.
/// One gate's failure never stops the others. When `gh` is not on `PATH`,
/// pull request and run gates are [`GateOutcome::Skipped`] and the report
/// sets `gh_missing`.
pub fn probe_gates(open: OpenGates, today: &str, gh_timeout: Duration) -> ProbedGates {
    let mut probed = ProbedGates::default();
    for (gate, task_status) in open.gates {
        let probe = match (task_status, gate.kind) {
            (TaskStatus::Completed | TaskStatus::Cancelled, _) => {
                Ok(Probe { outcome: GateOutcome::Skipped, detail: format!("task is {}", task_status.as_str()) })
            }
            (_, GateKind::Date) => Ok(date_probe(&gate.target, today)),
            (_, GateKind::Pr | GateKind::Run) if probed.gh_missing => Err(ProbeError::Missing),
            (_, GateKind::Pr) => pull_request_probe(&gate.target, gh_timeout),
            (_, GateKind::Run) => run_probe(&gate.target, gh_timeout),
        };
        let probe = match probe {
            Ok(probe) => probe,
            Err(ProbeError::Missing) => {
                probed.gh_missing = true;
                Probe { outcome: GateOutcome::Skipped, detail: GH_MISSING.to_owned() }
            }
            Err(ProbeError::Failed(detail)) => Probe { outcome: GateOutcome::Error, detail },
        };
        probed.probes.push((gate, probe));
    }
    probed
}

/// Mark the gates [`probe_gates`] found met, in one immediate transaction
/// that reads each gate again first, and report every gate.
///
/// A gate met by another check since the probe is reported met as it is
/// stored. A gate removed since the probe, or a transaction that fails, is
/// reported as [`GateOutcome::Error`] with that reason; nothing here fails
/// the check as a whole.
pub fn record_gate_checks(database: &mut Database, probed: ProbedGates) -> GateCheckReport {
    let met_ids = probed
        .probes
        .iter()
        .filter(|(_, probe)| probe.outcome == GateOutcome::Met)
        .map(|(gate, _)| gate.id)
        .collect::<Vec<_>>();
    let mut marked =
        if met_ids.is_empty() { Ok(Vec::new()) } else { database.meet_gates(&met_ids) }.map(Vec::into_iter);
    let mut report = GateCheckReport { checks: Vec::new(), gh_missing: probed.gh_missing };
    for (gate, probe) in probed.probes {
        let check = if probe.outcome != GateOutcome::Met {
            GateCheck { gate, outcome: probe.outcome, detail: probe.detail }
        } else {
            match marked.as_mut().map(Iterator::next) {
                Ok(Some(Some(met))) => GateCheck { gate: met, outcome: GateOutcome::Met, detail: probe.detail },
                Ok(Some(None) | None) => GateCheck {
                    gate,
                    outcome: GateOutcome::Error,
                    detail: "the gate was removed during the check".to_owned(),
                },
                Err(error) => GateCheck {
                    gate,
                    outcome: GateOutcome::Error,
                    detail: format!("could not mark the gate met: {error}"),
                },
            }
        };
        report.checks.push(check);
    }
    report
}

/// The `gh` timeout: [`GH_TIMEOUT_ENV`] in milliseconds when it is set to a
/// whole number greater than zero, and [`GH_TIMEOUT`] otherwise.
pub fn gh_timeout() -> Duration {
    std::env::var(GH_TIMEOUT_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|millis| *millis > 0)
        .map_or(GH_TIMEOUT, Duration::from_millis)
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

fn pull_request_probe(url: &str, timeout: Duration) -> Result<Probe, ProbeError> {
    let output = run_gh(&["pr", "view", url, "--json", "state,mergedAt"], timeout)?;
    pull_request_outcome(&output).map_err(ProbeError::Failed)
}

fn run_probe(url: &str, timeout: Duration) -> Result<Probe, ProbeError> {
    let run = GithubUrl::run(url).ok_or_else(|| ProbeError::Failed(format!("`{url}` is not a run URL")))?;
    let id = run.number.to_string();
    let repository = run.repository();
    let output = run_gh(
        &["run", "view", &id, "--repo", &repository, "--json", "status,conclusion"],
        timeout,
    )?;
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
/// closed and prompts are off, so `gh` never waits on the terminal. A call
/// still running after `timeout` fails with `timed out after <timeout>`.
///
/// `gh` runs as the leader of its own process group on unix. Once it exits
/// or times out, the whole group is killed, so a `git` or credential helper
/// it started cannot outlive it or hold its output open. The result is
/// judged from `gh`'s own exit status and output.
fn run_gh(args: &[&str], timeout: Duration) -> Result<String, ProbeError> {
    let deadline = Instant::now() + timeout;
    let mut command = Command::new(GH_PROGRAM);
    command
        .args(args)
        .env("GH_PROMPT_DISABLED", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command.spawn().map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => ProbeError::Missing,
        _ => ProbeError::Failed(format!("could not run gh: {error}")),
    })?;
    // Each pipe drains on its own thread, so a full pipe never stalls `gh`.
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let status = wait_until(&mut child, deadline);
    kill_tree(&child);
    if status.is_none() {
        let _ = child.kill();
    }
    // Reaps `gh` after a timeout; a no-op once `try_wait` has seen it exit.
    let _ = child.wait();
    let Some(status) = status else {
        return Err(ProbeError::Failed(format!("timed out after {timeout:?}")));
    };
    let stdout = stdout.recv_timeout(GH_DRAIN);
    let stderr = stderr.recv_timeout(GH_DRAIN).unwrap_or_default();
    if !status.success() {
        let stderr = String::from_utf8_lossy(&stderr);
        let reason = stderr
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("no error output");
        return Err(ProbeError::Failed(format!("gh failed ({status}): {reason}")));
    }
    let stdout = stdout.map_err(|_| ProbeError::Failed("gh output stayed open after gh exited".to_owned()))?;
    String::from_utf8(stdout).map_err(|_| ProbeError::Failed("gh output is not UTF-8".to_owned()))
}

/// Wait for `child` to exit until `deadline`. Returns its exit status, or
/// `None` when the deadline passed first or it could not be waited on. The
/// caller kills a child still running.
fn wait_until(child: &mut Child, deadline: Instant) -> Option<ExitStatus> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(GH_POLL),
            Ok(None) | Err(_) => return None,
        }
    }
}

/// Kill the process group `child` leads, which `run_gh` gives it on unix.
/// This runs `kill` rather than calling `killpg`, which would need `unsafe`.
/// Linux keeps a group's ID from being reused while any member is alive.
#[cfg(unix)]
fn kill_tree(child: &Child) {
    let _ = Command::new("kill")
        // `--` ends `kill`'s options, so it reads `-<pgid>` as a process group.
        .args(["-KILL", "--", &format!("-{}", child.id())])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Kill `child` and the processes it started with `taskkill /T`. Windows
/// finds them by parent ID, so this reaches them only while `child` runs.
#[cfg(windows)]
fn kill_tree(child: &Child) {
    let _ = Command::new("taskkill")
        .args(["/T", "/F", "/PID", &child.id().to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(not(any(unix, windows)))]
fn kill_tree(_child: &Child) {}

/// Read `pipe` to its end on a new thread, and send what it held. A missing
/// pipe sends nothing at once.
fn drain(pipe: Option<impl Read + Send + 'static>) -> Receiver<Vec<u8>> {
    let (sender, receiver) = mpsc::channel();
    match pipe {
        Some(mut pipe) => {
            thread::spawn(move || {
                let mut output = Vec::new();
                let _ = pipe.read_to_end(&mut output);
                let _ = sender.send(output);
            });
        }
        None => {
            let _ = sender.send(Vec::new());
        }
    }
    receiver
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

    /// A new in-memory database with one pending task titled `title`.
    fn database_with_task(title: &str) -> (Database, TaskId) {
        let mut database = Database::open_in_memory().expect("database opens");
        let task = create_task(&mut database, title);
        (database, task)
    }

    fn create_task(database: &mut Database, title: &str) -> TaskId {
        let project_id = database.project().expect("project exists").id;
        database
            .create_planning_task(arcl_store::PlanningTaskCreate {
                project_id,
                spec_id: None,
                plan_id: None,
                phase_id: None,
                parent_id: None,
                title: title.to_owned(),
                body: String::new(),
                priority: arcl_core::domain::TaskPriority::Normal,
                position: 0,
            })
            .expect("task creates")
            .id
    }

    #[test]
    fn a_check_meets_due_date_gates_and_leaves_the_rest() {
        let (mut database, task) = database_with_task("Work");
        let past = database
            .add_gate(task, GateKind::Date, "2026-10-01")
            .expect("gate adds");
        let future = database
            .add_gate(task, GateKind::Date, "2026-12-01")
            .expect("gate adds");

        let report = check_gates(&mut database, Some(task), "2026-10-27", GH_TIMEOUT).expect("check runs");
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

        let again = check_gates(&mut database, None, "2026-10-27", GH_TIMEOUT).expect("check runs");
        assert_eq!(again.checks.len(), 1, "a met gate is not checked again");
    }

    #[test]
    fn gates_on_closed_tasks_are_skipped_without_asking_gh() {
        let (mut database, done) = database_with_task("Done");
        let dropped = create_task(&mut database, "Dropped");
        database
            .transition_planning_task(done, arcl_core::domain::TaskAction::Start, false)
            .expect("task starts");
        let done_gate = database
            .add_gate(done, GateKind::Pr, "https://github.com/o/r/pull/1")
            .expect("gate adds");
        database
            .complete_planning_task(done, arcl_store::OpenSubtasks::Refuse, None)
            .expect("task completes");
        let dropped_gate = database
            .add_gate(dropped, GateKind::Date, "2000-01-01")
            .expect("gate adds");
        database
            .transition_planning_task(dropped, arcl_core::domain::TaskAction::Cancel, false)
            .expect("task cancels");

        let report = check_gates(&mut database, None, "2026-10-27", GH_TIMEOUT).expect("check runs");
        let mut found = report
            .checks
            .iter()
            .map(|check| (check.gate.id, check.outcome, check.detail.as_str(), check.gate.status))
            .collect::<Vec<_>>();
        found.sort_by_key(|(id, ..)| id.to_string());
        let mut expected = vec![
            (
                done_gate.id,
                GateOutcome::Skipped,
                "task is completed",
                GateStatus::Open,
            ),
            (
                dropped_gate.id,
                GateOutcome::Skipped,
                "task is cancelled",
                GateStatus::Open,
            ),
        ];
        expected.sort_by_key(|(id, ..)| id.to_string());
        assert_eq!(found, expected);
        assert!(!report.gh_missing, "a skipped closed task never looks for gh");
    }

    #[test]
    fn recording_reports_a_gate_removed_after_its_probe() {
        let (mut database, task) = database_with_task("Work");
        let kept = database
            .add_gate(task, GateKind::Date, "2000-01-01")
            .expect("gate adds");
        let removed = database
            .add_gate(task, GateKind::Date, "2000-01-02")
            .expect("gate adds");
        let probed = probe_gates(
            read_open_gates(&database, None).expect("gates read"),
            "2026-10-27",
            GH_TIMEOUT,
        );
        database.remove_gate(removed.id).expect("gate removes");

        let report = record_gate_checks(&mut database, probed);
        assert_eq!(report.checks[0].gate.id, kept.id);
        assert_eq!(report.checks[0].outcome, GateOutcome::Met);
        assert_eq!(report.checks[0].gate.status, GateStatus::Met);
        assert_eq!(report.checks[1].gate.id, removed.id);
        assert_eq!(report.checks[1].outcome, GateOutcome::Error);
        assert_eq!(report.checks[1].detail, "the gate was removed during the check");
    }
}
