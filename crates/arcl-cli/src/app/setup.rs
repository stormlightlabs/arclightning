//! `arcl setup <agent>`: write the files a coding agent needs to use arcl.
//!
//! Each agent gets the skill from `arcl skill install`, a marked block of
//! arcl lines in its instruction file, and a hook that runs `arcl prime` when
//! a session starts. A missing file is created. An existing file changes only
//! after the person confirms at a terminal or passes `--yes`, and a second
//! run with nothing to change writes nothing.

mod json;

use json::Json;
use serde::Serialize;

use super::skill::{SKILL_FILES, absent, check_target, current_project_root, install_skill, write_replacing};
use super::support::write_output;
use super::*;

/// The command every hook runs. It does nothing when `arcl` is not
/// installed, so a teammate without it gets no hook error.
const HOOK_COMMAND: &str = "if command -v arcl >/dev/null; then arcl prime; fi";

/// What a hook command must contain for `setup` to count it as the arcl hook.
const HOOK_NEEDLE: &str = "arcl prime";

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
const PI_EXTENSION: &str = r#"// Written by `arcl setup pi`. Runs `arcl prime` when a session starts and
// adds its output to the next prompt.
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

export default function arclPrime(pi: ExtensionAPI) {
	let primer: string | undefined;

	pi.on("session_start", async (_event, ctx) => {
		primer = undefined;
		try {
			const result = await pi.exec("arcl", ["prime"], { cwd: ctx.cwd, timeout: 10_000 });
			if (result.code === 0 && result.stdout.trim()) primer = result.stdout.trim();
		} catch {
			// arcl is not installed; start without the primer.
		}
	});

	pi.on("before_agent_start", async () => {
		if (!primer) return;
		const content = primer;
		primer = undefined;
		return { message: { customType: "arcl-prime", content, display: false } };
	});
}
"#;

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
    hook: Hook,
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
                hook: Hook::Json { path: ".claude/settings.json", matcher: None },
                notes: &[],
            },
            SetupAgent::Codex => Self {
                agent: "codex",
                name: "Codex",
                instructions: "AGENTS.md",
                hook: Hook::Json { path: ".codex/hooks.json", matcher: Some("startup|resume") },
                notes: &["Codex runs project hooks only in a trusted project, after you trust the hook in `/hooks`."],
            },
            SetupAgent::Pi => Self {
                agent: "pi",
                name: "Pi",
                instructions: "AGENTS.md",
                hook: Hook::File { path: ".pi/extensions/arcl-prime.ts", content: PI_EXTENSION },
                notes: &["Pi loads `.pi/extensions/` and `.agents/skills/` only after you trust the project."],
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
        Hook::Json { path: relative, matcher } => {
            let path = resolve(root, &canonical_root, relative)?;
            let before = read_optional(&path)?;
            let after = with_hook(before.as_deref(), matcher, &path)?;
            (relative, path, before, after)
        }
        Hook::File { path: relative, content } => {
            let path = resolve(root, &canonical_root, relative)?;
            (relative, path.clone(), read_optional(&path)?, content.to_owned())
        }
    };
    planned.push(PlannedFile::new(relative.to_owned(), Some(path), before, after));
    Ok(planned)
}

/// Write the planned changes: the skill through `skill install`, then each
/// other created or updated file.
fn apply(root: &Path, planned: Vec<PlannedFile>) -> CResult<Vec<SetupFile>> {
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
/// after a blank line.
fn with_block(text: &str, path: &Path) -> CResult<String> {
    let block = format!("{BLOCK_BEGIN}\n{BLOCK_BODY}{BLOCK_END}\n");
    let Some(start) = text.find(BLOCK_BEGIN) else {
        let separator = match text {
            "" => "",
            _ if text.ends_with("\n\n") => "",
            _ if text.ends_with('\n') => "\n",
            _ => "\n\n",
        };
        return Ok(format!("{text}{separator}{block}"));
    };
    let Some(offset) = text[start..].find(BLOCK_END) else {
        return Err(CommandError::UnclosedSetupBlock { path: path.to_owned(), marker: BLOCK_BEGIN, end: BLOCK_END });
    };
    let mut end = start + offset + BLOCK_END.len();
    if text[end..].starts_with('\n') {
        end += 1;
    }
    Ok(format!("{}{block}{}", &text[..start], &text[end..]))
}

/// The settings file with a `SessionStart` hook that runs [`HOOK_COMMAND`],
/// unless a `SessionStart` hook already runs `arcl prime`. Other keys keep
/// their order, and a rewritten file keeps the existing indentation.
fn with_hook(before: Option<&str>, matcher: Option<&str>, path: &Path) -> CResult<String> {
    let invalid = |message: String| CommandError::InvalidSetupJson { path: path.to_owned(), message };
    let text = before.unwrap_or("");
    let mut settings = if text.trim().is_empty() {
        Json::Object(Vec::new())
    } else {
        serde_json::from_str::<Json>(text).map_err(|error| invalid(error.to_string()))?
    };
    if has_hook(&settings) {
        return Ok(text.to_owned());
    }
    let mut entry = Vec::new();
    if let Some(matcher) = matcher {
        entry.push(("matcher".to_owned(), Json::String(matcher.to_owned())));
    }
    let command = Json::Object(vec![
        ("type".to_owned(), Json::String("command".to_owned())),
        ("command".to_owned(), Json::String(HOOK_COMMAND.to_owned())),
    ]);
    entry.push(("hooks".to_owned(), Json::Array(vec![command])));

    let kind = settings.kind();
    let Json::Object(top) = &mut settings else {
        return Err(invalid(format!("the file holds {kind}, not an object")));
    };
    if !top.iter().any(|(key, _)| key == "hooks") {
        top.push(("hooks".to_owned(), Json::Object(Vec::new())));
    }
    let hooks = settings.get_mut("hooks").expect("`hooks` was just added");
    let kind = hooks.kind();
    let Json::Object(events) = &mut *hooks else {
        return Err(invalid(format!("`hooks` is {kind}, not an object")));
    };
    if !events.iter().any(|(key, _)| key == "SessionStart") {
        events.push(("SessionStart".to_owned(), Json::Array(Vec::new())));
    }
    let session_start = hooks.get_mut("SessionStart").expect("`SessionStart` was just added");
    let kind = session_start.kind();
    let Json::Array(entries) = session_start else {
        return Err(invalid(format!("`hooks.SessionStart` is {kind}, not an array")));
    };
    entries.push(Json::Object(entry));

    let indent = indentation(text);
    let mut out = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(indent.as_bytes());
    let mut serializer = serde_json::Serializer::with_formatter(&mut out, formatter);
    settings
        .serialize(&mut serializer)
        .map_err(|error| invalid(error.to_string()))?;
    let mut rendered = String::from_utf8(out).map_err(|error| invalid(error.to_string()))?;
    rendered.push('\n');
    Ok(rendered)
}

/// Whether a `SessionStart` hook in `settings` already runs `arcl prime`.
fn has_hook(settings: &Json) -> bool {
    let Some(entries) = settings
        .get("hooks")
        .and_then(|hooks| hooks.get("SessionStart"))
        .and_then(Json::as_array)
    else {
        return false;
    };
    entries
        .iter()
        .filter_map(|entry| entry.get("hooks").and_then(Json::as_array))
        .flatten()
        .filter_map(|hook| hook.get("command").and_then(Json::as_str))
        .any(|command| command.contains(HOOK_NEEDLE))
}

/// The indentation of the first indented line of `text`, or a tab.
fn indentation(text: &str) -> String {
    text.lines()
        .map(|line| &line[..line.len() - line.trim_start().len()])
        .find(|indent| !indent.is_empty())
        .unwrap_or("\t")
        .to_owned()
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
    fn the_hook_merges_into_existing_settings_in_key_order() {
        let before = "{\n  \"permissions\": {\"allow\": [\"Bash(ls)\"]},\n  \"hooks\": {\"Stop\": []}\n}\n";
        let after = with_hook(Some(before), None, &path()).expect("merges");
        let keys = [
            "\"permissions\"",
            "\"allow\"",
            "\"hooks\"",
            "\"Stop\"",
            "\"SessionStart\"",
        ]
        .map(|key| after.find(key).expect("key kept"));
        assert!(keys.windows(2).all(|pair| pair[0] < pair[1]), "{after}");
        assert!(after.contains("\n  \"hooks\""), "kept two-space indentation: {after}");
        assert_eq!(with_hook(Some(&after), None, &path()).expect("second run"), after);
    }

    #[test]
    fn a_settings_file_that_is_not_an_object_is_an_error() {
        for text in ["[]", "{\"hooks\": []}", "{\"hooks\": {\"SessionStart\": {}}}", "{"] {
            let error = with_hook(Some(text), None, &path()).expect_err(text);
            assert!(matches!(error, CommandError::InvalidSetupJson { .. }), "{text}");
        }
    }

    #[test]
    fn only_yes_accepts() {
        assert!(accepts("y\n") && accepts(" YES \n"));
        assert!(!accepts("\n") && !accepts("n\n") && !accepts("yep\n"));
    }
}
