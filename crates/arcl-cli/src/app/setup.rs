//! `arcl setup [agent]`: write the files a coding agent needs to use arcl.
//!
//! Each agent gets the skill from `arcl skill install` and a marked block of
//! arcl lines in its instruction file. Claude Code, Codex and Pi also get a
//! hook that runs `arcl prime` when a session starts; `other`, the default,
//! gets no hook. A missing file is created. An existing file changes only
//! after the person confirms at a terminal or passes `--yes`, and a second
//! run with nothing to change writes nothing.

mod json;

use json::with_hook;

use super::skill::{SKILL_FILES, absent, check_target, current_project_root, install_skill, write_replacing};
use super::support::write_output;
use super::*;

/// The command every hook runs. It does nothing when `arcl` is not
/// installed, so a teammate without it gets no hook error.
const HOOK_COMMAND: &str = "if command -v arcl >/dev/null; then arcl prime; fi";

/// The line that opens the arcl block in an instruction file.
const BLOCK_BEGIN: &str = "<!-- arcl-setup:begin -->";

/// The line that closes the arcl block in an instruction file.
const BLOCK_END: &str = "<!-- arcl-setup:end -->";

/// The arcl lines between [`BLOCK_BEGIN`] and [`BLOCK_END`].
const BLOCK_BODY: &str = "\
## Arc Lightning

This project tracks its work with Arc Lightning (`arcl`). Use the
`arclightning` skill to find, start, hand off and complete tasks. `arcl prime`
prints the working loop and the open work. A fresh clone or worktree needs
`arcl init` first.
";

/// The Pi extension `arcl setup pi` writes. Pi has no command hooks, so the
/// extension runs `arcl prime` on `session_start` and adds its output to the
/// next prompt. The type import is erased when Pi loads the file, so it needs
/// no package installed.
const PI_EXTENSION: &str = include_str!("setup/arcl-prime.ts");

/// How an agent runs `arcl prime` at session start.
enum Hook {
    /// A `SessionStart` command hook in a JSON settings file, with an
    /// optional matcher.
    Json {
        path: &'static str,
        matcher: Option<&'static str>,
    },
    /// A whole file `setup` owns.
    File { path: &'static str, content: &'static str },
}

/// What `setup` writes for one agent.
struct AgentFiles {
    /// The agent name as the command takes it.
    agent: &'static str,
    /// The agent's display name.
    name: &'static str,
    /// The instruction file, relative to the project root.
    instructions: &'static str,
    /// The start-up hook, if the agent has a way to run one.
    hook: Option<Hook>,
    /// What the agent needs before it reads the files.
    notes: &'static [&'static str],
}

impl AgentFiles {
    /// The files for `agent`, from each agent's documented locations.
    const fn of(agent: SetupAgent) -> Self {
        match agent {
            SetupAgent::Claude => Self {
                agent: "claude",
                name: "Claude Code",
                instructions: "CLAUDE.md",
                hook: Some(Hook::Json { path: ".claude/settings.json", matcher: None }),
                notes: &[],
            },
            SetupAgent::Codex => Self {
                agent: "codex",
                name: "Codex",
                instructions: "AGENTS.md",
                hook: Some(Hook::Json { path: ".codex/hooks.json", matcher: Some("startup|resume") }),
                notes: &["Codex runs project hooks only in a trusted project, after you trust the hook in `/hooks`."],
            },
            SetupAgent::Pi => Self {
                agent: "pi",
                name: "Pi",
                instructions: "AGENTS.md",
                hook: Some(Hook::File { path: ".pi/extensions/arcl-prime.ts", content: PI_EXTENSION }),
                notes: &["Pi loads `.pi/extensions/` and `.agents/skills/` only after you trust the project."],
            },
            SetupAgent::Other => Self {
                agent: "other",
                name: "other agents",
                instructions: "AGENTS.md",
                hook: None,
                notes: &["No start-up hook is written. Have the agent run `arcl prime` when a session starts."],
            },
        }
    }
}

/// One file `setup` compared and may write.
struct PlannedFile {
    /// The path relative to the project root.
    relative: String,
    /// Where to write it; `None` for a skill file, which `skill install`
    /// writes.
    resolved: Option<PathBuf>,
    before: Option<String>,
    after: String,
    status: SetupFileStatus,
}

impl PlannedFile {
    /// Compare `after` with `before` and pick the status.
    fn new(relative: String, resolved: Option<PathBuf>, before: Option<String>, after: String) -> Self {
        let status = match &before {
            None => SetupFileStatus::Created,
            Some(text) if *text == after => SetupFileStatus::Unchanged,
            Some(_) => SetupFileStatus::Updated,
        };
        Self { relative, resolved, before, after, status }
    }

    /// The change as a unified diff, `--- /dev/null` for a new file.
    fn diff(&self) -> String {
        let before = self.before.as_deref().unwrap_or("");
        let old = if self.before.is_some() { format!("a/{}", self.relative) } else { "/dev/null".to_owned() };
        similar::TextDiff::from_lines(before, self.after.as_str())
            .unified_diff()
            .context_radius(3)
            .header(&old, &format!("b/{}", self.relative))
            .to_string()
    }
}

/// Run `arcl setup <agent>` and render its report.
pub(super) fn exec_setup(agent: SetupAgent, yes: bool, dry_run: bool, renderer: &Renderer) -> anyhow::Result<()> {
    let files = AgentFiles::of(agent);
    let root = current_project_root().map_err(ApplicationError::from_command)?;
    let planned = plan(&root, &files).map_err(ApplicationError::from_command)?;
    let report = |action, files_out| SetupReport {
        action,
        agent: files.agent,
        agent_name: files.name,
        root: root.to_string_lossy().into_owned(),
        files: files_out,
        notes: files.notes.to_vec(),
    };
    if dry_run {
        let entries = planned
            .iter()
            .map(|file| SetupFile {
                path: file.relative.clone(),
                status: file.status,
                diff: (file.status != SetupFileStatus::Unchanged).then(|| file.diff()),
            })
            .collect();
        return write_output(renderer.render_setup(&report(SetupAction::Planned, entries))?);
    }
    let existing = planned
        .iter()
        .filter(|file| file.status == SetupFileStatus::Updated)
        .collect::<Vec<_>>();
    if !yes && !existing.is_empty() {
        confirm(&existing).map_err(ApplicationError::from_command)?;
    }
    let entries = apply(&root, planned).map_err(ApplicationError::from_command)?;
    write_output(renderer.render_setup(&report(SetupAction::Applied, entries))?)
}

/// Compare every file `setup` writes for `files` with what is on disk, and
/// check each path, before anything is written.
fn plan(root: &Path, files: &AgentFiles) -> CResult<Vec<PlannedFile>> {
    let canonical_root =
        fs::canonicalize(root).map_err(|source| CommandError::ReadSkill { path: root.to_owned(), source })?;
    let mut planned = Vec::new();
    for skill in install_skill(root, true)?.files {
        let content = SKILL_FILES
            .iter()
            .find(|(name, _)| skill.path.ends_with(&format!("/{name}")))
            .map_or("", |(_, content)| *content);
        let before = read_optional(&root.join(&skill.path))?;
        planned.push(PlannedFile::new(skill.path, None, before, content.to_owned()));
    }

    let path = resolve(root, &canonical_root, files.instructions)?;
    let before = read_optional(&path)?;
    let after = with_block(before.as_deref().unwrap_or(""), &path)?;
    planned.push(PlannedFile::new(
        files.instructions.to_owned(),
        Some(path),
        before,
        after,
    ));

    let (relative, path, before, after) = match files.hook {
        None => return Ok(planned),
        Some(Hook::Json { path: relative, matcher }) => {
            let path = resolve(root, &canonical_root, relative)?;
            let before = read_optional(&path)?;
            let after = with_hook(before.as_deref(), matcher, &path)?;
            (relative, path, before, after)
        }
        Some(Hook::File { path: relative, content }) => {
            let path = resolve(root, &canonical_root, relative)?;
            (relative, path.clone(), read_optional(&path)?, content.to_owned())
        }
    };
    planned.push(PlannedFile::new(relative.to_owned(), Some(path), before, after));
    Ok(planned)
}

/// Write the planned changes: the skill through `skill install`, then each
/// other created or updated file. Every planned file is read again first, and
/// if any differs from what the diff was made from, such as an edit saved
/// while the prompt waited, nothing is written.
fn apply(root: &Path, planned: Vec<PlannedFile>) -> CResult<Vec<SetupFile>> {
    for file in &planned {
        let path = file.resolved.clone().unwrap_or_else(|| root.join(&file.relative));
        if read_optional(&path)? != file.before {
            return Err(CommandError::SetupFileChanged { path: file.relative.clone() });
        }
    }
    let skill_changes = planned
        .iter()
        .any(|file| file.resolved.is_none() && file.status != SetupFileStatus::Unchanged);
    let mut skill_statuses = if skill_changes { install_skill(root, false)?.files } else { Vec::new() }.into_iter();
    let mut entries = Vec::new();
    for file in planned {
        let status = match &file.resolved {
            // `skill install` reports a file a linked directory already wrote
            // as unchanged.
            None => skill_statuses.next().map_or(file.status, |skill| match skill.status {
                SkillFileStatus::Created => SetupFileStatus::Created,
                SkillFileStatus::Updated => SetupFileStatus::Updated,
                _ => SetupFileStatus::Unchanged,
            }),
            Some(path) => {
                if file.status != SetupFileStatus::Unchanged {
                    write_replacing(path, &file.after)?;
                }
                file.status
            }
        };
        entries.push(SetupFile { path: file.relative, status, diff: None });
    }
    Ok(entries)
}

/// Ask on the terminal before changing `existing` files. With no terminal on
/// standard input there is no one to ask, so it refuses.
fn confirm(existing: &[&PlannedFile]) -> CResult<()> {
    let paths = existing.iter().map(|file| file.relative.clone()).collect::<Vec<_>>();
    if !io::stdin().is_terminal() {
        return Err(CommandError::SetupNeedsConfirmation { paths });
    }
    let mut stderr = io::stderr().lock();
    for file in existing {
        let _ = writeln!(stderr, "{}", file.diff());
    }
    let _ = write!(stderr, "Change {}? [y/N] ", paths.join(", "));
    let _ = stderr.flush();
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|source| CommandError::ReadStdin { source })?;
    if accepts(&answer) { Ok(()) } else { Err(CommandError::SetupDeclined) }
}

/// Whether a prompt answer says yes.
fn accepts(answer: &str) -> bool {
    matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

/// Where `relative` is written. A symbolic link, such as `CLAUDE.md` linked
/// to `AGENTS.md`, resolves to the file it names, which must be under the
/// project root; other paths take the skill install checks.
fn resolve(root: &Path, canonical_root: &Path, relative: &str) -> CResult<PathBuf> {
    let path = root.join(relative);
    match fs::symlink_metadata(&path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            let target =
                fs::canonicalize(&path).map_err(|source| CommandError::ReadSkill { path: path.clone(), source })?;
            if !target.starts_with(canonical_root) {
                return Err(CommandError::SkillOutsideProject { path, root: canonical_root.to_owned() });
            }
            if !target.is_file() {
                return Err(CommandError::SkillNotFile { path });
            }
            Ok(target)
        }
        _ => Ok(check_target(root, canonical_root, relative.to_owned(), "")?.resolved),
    }
}

/// The file's text, or `None` when it does not exist.
fn read_optional(path: &Path) -> CResult<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if absent(&error) => Ok(None),
        Err(source) => Err(CommandError::ReadSkill { path: path.to_owned(), source }),
    }
}

/// `text` with the arcl block: replaced where the markers are, or appended
/// after a blank line. A marker counts only on a line of its own, so one
/// named in prose or inline code is left alone. The block takes the line
/// ending of the file's first line. A begin marker with no end after it, or
/// a second begin marker, is an error.
fn with_block(text: &str, path: &Path) -> CResult<String> {
    let eol = line_ending(text);
    let block = format!("{BLOCK_BEGIN}\n{BLOCK_BODY}{BLOCK_END}\n").replace('\n', eol);
    // Each line's byte range, with its line ending, and its trimmed text.
    let mut offset = 0;
    let lines = text
        .split_inclusive('\n')
        .map(|line| {
            let start = offset;
            offset += line.len();
            (start, offset, line.trim())
        })
        .collect::<Vec<_>>();
    let mut begins = lines.iter().filter(|(_, _, line)| *line == BLOCK_BEGIN);
    let Some(&(start, _, _)) = begins.next() else {
        let separator = match text {
            "" => String::new(),
            _ if text.ends_with(&eol.repeat(2)) => String::new(),
            _ if text.ends_with('\n') => eol.to_owned(),
            _ => eol.repeat(2),
        };
        return Ok(format!("{text}{separator}{block}"));
    };
    if begins.next().is_some() {
        return Err(CommandError::DuplicateSetupBlock { path: path.to_owned(), marker: BLOCK_BEGIN });
    }
    let Some(&(_, end, _)) = lines
        .iter()
        .find(|(line_start, _, line)| *line_start > start && *line == BLOCK_END)
    else {
        return Err(CommandError::UnclosedSetupBlock { path: path.to_owned(), marker: BLOCK_BEGIN, end: BLOCK_END });
    };
    Ok(format!("{}{block}{}", &text[..start], &text[end..]))
}

/// `"\r\n"` when the first line of `text` ends with it, otherwise `"\n"`.
fn line_ending(text: &str) -> &'static str {
    match text.find('\n') {
        Some(index) if text[..index].ends_with('\r') => "\r\n",
        _ => "\n",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path() -> PathBuf {
        PathBuf::from("settings.json")
    }

    #[test]
    fn the_block_is_appended_once_and_replaced_in_place() {
        let once = with_block("# Project\n", &path()).expect("appends");
        assert!(once.starts_with("# Project\n\n<!-- arcl-setup:begin -->\n"), "{once}");
        assert_eq!(with_block(&once, &path()).expect("replaces"), once);

        let stale = once.replace("A fresh clone", "An old line. A fresh clone") + "tail\n";
        let fixed = with_block(&stale, &path()).expect("replaces");
        assert_eq!(fixed, once.clone() + "tail\n");
    }

    #[test]
    fn an_unclosed_block_is_an_error() {
        let error = with_block("<!-- arcl-setup:begin -->\nhalf\n", &path()).expect_err("refuses");
        assert!(matches!(error, CommandError::UnclosedSetupBlock { .. }));
    }

    #[test]
    fn a_second_begin_marker_is_an_error() {
        let once = with_block("# Project\n", &path()).expect("appends");
        for text in [format!("{once}{once}"), format!("{once}<!-- arcl-setup:begin -->\n")] {
            let error = with_block(&text, &path()).expect_err("refuses");
            assert!(matches!(error, CommandError::DuplicateSetupBlock { .. }), "{text}");
            assert!(error.to_string().contains("settings.json"), "{error}");
        }
    }

    #[test]
    fn the_block_takes_a_crlf_file_line_ending() {
        let once = with_block("# Project\r\n", &path()).expect("appends");
        assert!(
            once.starts_with("# Project\r\n\r\n<!-- arcl-setup:begin -->\r\n"),
            "{once}"
        );
        assert!(once.ends_with("<!-- arcl-setup:end -->\r\n"), "{once}");
        assert_eq!(once.matches('\n').count(), once.matches("\r\n").count(), "{once:?}");
        assert_eq!(with_block(&once, &path()).expect("replaces"), once);

        let stale = once.replace("A fresh clone", "An old line. A fresh clone") + "tail\r\n";
        assert_eq!(
            with_block(&stale, &path()).expect("replaces"),
            once.clone() + "tail\r\n"
        );
    }

    #[test]
    fn markers_named_inside_a_line_are_left_alone() {
        let text = "Wrap it in `<!-- arcl-setup:begin -->` and `<!-- arcl-setup:end -->`.\nKeep this.\n";
        let once = with_block(text, &path()).expect("appends");
        assert!(
            once.starts_with(&format!("{text}\n<!-- arcl-setup:begin -->\n")),
            "{once}"
        );
        assert_eq!(with_block(&once, &path()).expect("replaces"), once);

        let unclosed = format!("{BLOCK_BEGIN}\nsee `{BLOCK_END}`\n");
        let error = with_block(&unclosed, &path()).expect_err("refuses");
        assert!(matches!(error, CommandError::UnclosedSetupBlock { .. }));

        let indented = once.replace(&format!("\n{BLOCK_BEGIN}\n"), &format!("\n  {BLOCK_BEGIN}\t\n"));
        assert_ne!(indented, once);
        assert_eq!(with_block(&indented, &path()).expect("replaces"), once);
    }

    #[test]
    fn a_file_changed_after_the_plan_stops_every_write() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let root = directory.path();
        fs::write(root.join("CLAUDE.md"), "# Rules\n").expect("writes");
        let planned = plan(root, &AgentFiles::of(SetupAgent::Claude)).expect("plans");
        fs::write(root.join("CLAUDE.md"), "# Rules\n\nSaved during the prompt.\n").expect("writes");

        let error = apply(root, planned).expect_err("refuses");
        assert!(
            matches!(&error, CommandError::SetupFileChanged { path } if path == "CLAUDE.md"),
            "{error}"
        );
        assert_eq!(
            fs::read_to_string(root.join("CLAUDE.md")).expect("reads"),
            "# Rules\n\nSaved during the prompt.\n"
        );
        assert!(!root.join(".claude").exists(), "wrote the skill or the hook");
    }

    #[test]
    fn only_yes_accepts() {
        assert!(accepts("y\n") && accepts(" YES \n"));
        assert!(!accepts("\n") && !accepts("n\n") && !accepts("yep\n"));
    }
}
