use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::{DomainError, GateId, TaskId};

/// The form a pull request gate's target takes.
const PR_SHAPE: &str = "https://github.com/OWNER/REPO/pull/NUMBER";

/// The form a run gate's target takes.
const RUN_SHAPE: &str = "https://github.com/OWNER/REPO/actions/runs/RUN_ID";

/// What a gate waits for.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum GateKind {
    /// A GitHub pull request to merge.
    Pr,
    /// A GitHub Actions run to finish successfully.
    Run,
    /// A date to arrive, in UTC.
    Date,
}

impl GateKind {
    /// Return the stable value stored in SQLite, workspace files and JSON.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pr => "pr",
            Self::Run => "run",
            Self::Date => "date",
        }
    }

    /// Parse a persisted gate kind.
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "pr" => Ok(Self::Pr),
            "run" => Ok(Self::Run),
            "date" => Ok(Self::Date),
            _ => Err(DomainError::InvalidGateKind { value: value.to_owned() }),
        }
    }
}

/// Whether a gate still holds its task back.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum GateStatus {
    /// Not met yet: the task is not ready.
    Open,
    /// Met: `arcl gate check` found the condition true. A met gate stays met.
    Met,
}

impl GateStatus {
    /// Return the stable value stored in SQLite, workspace files and JSON.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Met => "met",
        }
    }

    /// Parse a persisted gate status.
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "open" => Ok(Self::Open),
            "met" => Ok(Self::Met),
            _ => Err(DomainError::InvalidStatus { entity: "gate", value: value.to_owned() }),
        }
    }
}

/// A condition outside arcl that a task waits on: a pull request merging, a
/// CI run passing or a date arriving.
///
/// An open gate keeps its task from being ready. Only `arcl gate check`
/// marks a gate met, so readiness never depends on the clock or the network.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Gate {
    /// The gate's ID, `arcl-g-…`.
    pub id: GateId,
    /// The task the gate holds back.
    pub task_id: TaskId,
    /// What the gate waits for.
    pub kind: GateKind,
    /// The pull request or run URL, or the date as `YYYY-MM-DD`.
    pub target: String,
    /// Whether the gate is still open.
    pub status: GateStatus,
    /// When the gate was added, as an RFC 3339 UTC timestamp.
    pub created_at: String,
    /// When `arcl gate check` found the gate met, as an RFC 3339 UTC
    /// timestamp. Absent while the gate is open.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub met_at: Option<String>,
}

impl Gate {
    /// What the gate waits for, in words that follow "waits":
    /// ``for pull request `URL` to merge``, ``for run `URL` to succeed`` or
    /// ``until `YYYY-MM-DD` ``.
    pub fn condition(&self) -> String {
        match self.kind {
            GateKind::Pr => format!("for pull request `{}` to merge", self.target),
            GateKind::Run => format!("for run `{}` to succeed", self.target),
            GateKind::Date => format!("until `{}`", self.target),
        }
    }
}

/// A GitHub pull request or Actions run URL, split into its parts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GithubUrl {
    /// The host, lowercased, such as `github.com`.
    pub host: String,
    /// The repository owner.
    pub owner: String,
    /// The repository name.
    pub repo: String,
    /// The pull request number or run ID.
    pub number: u64,
}

impl GithubUrl {
    /// Parse a pull request URL, `https://HOST/OWNER/REPO/pull/NUMBER`.
    pub fn pull_request(url: &str) -> Option<Self> {
        Self::parse(url, &["pull"])
    }

    /// Parse an Actions run URL, `https://HOST/OWNER/REPO/actions/runs/ID`.
    pub fn run(url: &str) -> Option<Self> {
        Self::parse(url, &["actions", "runs"])
    }

    /// The repository as `gh --repo` takes it, `HOST/OWNER/REPO`.
    pub fn repository(&self) -> String {
        format!("{}/{}/{}", self.host, self.owner, self.repo)
    }

    /// Parse `https://HOST/OWNER/REPO/<middle>/NUMBER`, with at most one
    /// trailing slash and no query or fragment.
    fn parse(url: &str, middle: &[&str]) -> Option<Self> {
        let rest = url.strip_prefix("https://")?;
        let rest = rest.strip_suffix('/').unwrap_or(rest);
        let mut parts = rest.split('/');
        let host = parts.next()?;
        let owner = parts.next()?;
        let repo = parts.next()?;
        for expected in middle {
            if parts.next()? != *expected {
                return None;
            }
        }
        let number = parts.next()?;
        if parts.next().is_some() {
            return None;
        }
        let host_ok = !host.is_empty()
            && !host.starts_with(['.', '-'])
            && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
        let owner_ok =
            !owner.is_empty() && owner.len() <= 39 && owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
        let repo_ok = !repo.is_empty()
            && repo.len() <= 100
            && repo != "."
            && repo != ".."
            && repo
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
        let number_ok = !number.is_empty() && !number.starts_with('0') && number.chars().all(|c| c.is_ascii_digit());
        if !(host_ok && owner_ok && repo_ok && number_ok) {
            return None;
        }
        Some(Self {
            host: host.to_ascii_lowercase(),
            owner: owner.to_owned(),
            repo: repo.to_owned(),
            number: number.parse().ok()?,
        })
    }
}

/// Return `target` in the form a gate of `kind` stores, or refuse it.
///
/// A pull request target must be `https://HOST/OWNER/REPO/pull/NUMBER` and
/// a run target `https://HOST/OWNER/REPO/actions/runs/ID`; either is stored
/// with a lowercase host and no trailing slash. A date target must be a
/// calendar date written `YYYY-MM-DD`. Surrounding whitespace is trimmed.
pub fn validate_gate_target(kind: GateKind, target: &str) -> Result<String, DomainError> {
    let target = target.trim();
    match kind {
        GateKind::Pr => GithubUrl::pull_request(target)
            .map(|url| format!("https://{}/{}/{}/pull/{}", url.host, url.owner, url.repo, url.number))
            .ok_or_else(|| DomainError::InvalidGateUrl {
                kind: "pull request",
                value: target.to_owned(),
                shape: PR_SHAPE,
            }),
        GateKind::Run => GithubUrl::run(target)
            .map(|url| {
                format!(
                    "https://{}/{}/{}/actions/runs/{}",
                    url.host, url.owner, url.repo, url.number
                )
            })
            .ok_or_else(|| DomainError::InvalidGateUrl { kind: "run", value: target.to_owned(), shape: RUN_SHAPE }),
        GateKind::Date => {
            if is_calendar_date(target) {
                Ok(target.to_owned())
            } else {
                Err(DomainError::InvalidGateDate { value: target.to_owned() })
            }
        }
    }
}

/// Whether `value` is a calendar date written `YYYY-MM-DD`, from year 0001.
pub fn is_calendar_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
    {
        return false;
    }
    let number = |range: std::ops::Range<usize>| value[range].parse::<u32>().unwrap_or(0);
    let (year, month, day) = (number(0..4), number(5..7), number(8..10));
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    year >= 1 && (1..=days).contains(&day)
}

/// Today's date in UTC, as `YYYY-MM-DD`. A date gate is met from the start
/// of its date in UTC.
pub fn today_utc() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    civil_date(i64::try_from(seconds / 86_400).unwrap_or(0))
}

/// The proleptic Gregorian date `days` after 1970-01-01, as `YYYY-MM-DD`,
/// by Howard Hinnant's `civil_from_days`.
fn civil_date(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pull_request_and_run_urls_are_normalized_or_refused() {
        assert_eq!(
            validate_gate_target(
                GateKind::Pr,
                " https://GitHub.com/stormlightlabs/arclightning/pull/77/ "
            ),
            Ok("https://github.com/stormlightlabs/arclightning/pull/77".to_owned())
        );
        assert_eq!(
            validate_gate_target(GateKind::Run, "https://github.com/o/r.s_t/actions/runs/123"),
            Ok("https://github.com/o/r.s_t/actions/runs/123".to_owned())
        );
        for (kind, url) in [
            (GateKind::Pr, "http://github.com/o/r/pull/1"),
            (GateKind::Pr, "https://github.com/o/r/pull/0"),
            (GateKind::Pr, "https://github.com/o/r/pull/1?x=1"),
            (GateKind::Pr, "https://github.com/o/r/pull/1/files"),
            (GateKind::Pr, "https://github.com/o/r/issues/1"),
            (GateKind::Pr, "https://github.com/o/../pull/1"),
            (GateKind::Pr, "https://github.com/o/r/actions/runs/1"),
            (GateKind::Run, "https://github.com/o/r/pull/1"),
            (GateKind::Run, "https://github.com/o/r/actions/runs/1/job/2"),
            (GateKind::Run, "https://github.com/o/r/actions/runs/abc"),
        ] {
            assert!(
                matches!(validate_gate_target(kind, url), Err(DomainError::InvalidGateUrl { .. })),
                "{url}"
            );
        }
        let url = GithubUrl::run("https://github.com/o/r/actions/runs/42").expect("run URL parses");
        assert_eq!((url.repository(), url.number), ("github.com/o/r".to_owned(), 42));
    }

    #[test]
    fn dates_must_be_calendar_dates() {
        for date in ["2026-10-27", "2024-02-29", "2000-02-29", "0001-01-01"] {
            assert_eq!(validate_gate_target(GateKind::Date, date), Ok(date.to_owned()));
        }
        for date in [
            "2026-02-29",
            "1900-02-29",
            "2026-13-01",
            "2026-00-10",
            "2026-1-01",
            "26-10-27",
            "2026/10/27",
            "0000-01-01",
            "２026-10-27",
        ] {
            assert!(!is_calendar_date(date), "{date}");
        }
    }

    #[test]
    fn civil_dates_follow_the_calendar() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(11_016), "2000-02-29");
        assert_eq!(civil_date(20_753), "2026-10-27");
        assert!(is_calendar_date(&today_utc()));
    }

    #[test]
    fn kinds_and_statuses_round_trip_through_their_names() {
        for kind in [GateKind::Pr, GateKind::Run, GateKind::Date] {
            assert_eq!(GateKind::parse(kind.as_str()), Ok(kind));
        }
        for status in [GateStatus::Open, GateStatus::Met] {
            assert_eq!(GateStatus::parse(status.as_str()), Ok(status));
        }
        assert!(GateKind::parse("issue").is_err());
        assert!(GateStatus::parse("closed").is_err());
    }
}
