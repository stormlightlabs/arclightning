//! `arcl skill install`: write the Arc Lightning skill into a project.
//!
//! The skill files are compiled into the binary from `skills/arclightning/`,
//! the one source directory, so an installed copy always matches the
//! repository copy the binary was built from.

use super::support::{nearest_project_root, write_output};
use super::*;

/// The agent skill directories, relative to the project root, that receive
/// the skill.
const SKILL_DIRECTORIES: &[&str] = &[".claude/skills", ".agents/skills"];

/// The skill's directory name under each agent skill directory.
const SKILL_NAME: &str = "arclightning";

/// Every file in `skills/arclightning/`, as a path relative to that directory
/// and its contents. The `embedded_files_match_the_source_directory` test
/// fails when a file is added there but not here.
const SKILL_FILES: &[(&str, &str)] = &[(
    "SKILL.md",
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../skills/arclightning/SKILL.md"
    )),
)];

/// Run one `skill` subcommand and render its report. `install --check`
/// returns an error with exit code 4 after the report when any file drifted.
pub(super) fn exec_skill(command: SkillCommand, renderer: &Renderer) -> anyhow::Result<()> {
    match command {
        SkillCommand::Install { check } => {
            let start = std::env::current_dir()
                .map_err(|source| ApplicationError::from_command(CommandError::CurrentDirectory { source }))?;
            let Some(root) = nearest_project_root(&start) else {
                return Err(ApplicationError::from_command(CommandError::NotInitialized { root: start }).into());
            };
            let report = install_skill(&root, check).map_err(ApplicationError::from_command)?;
            let drifted = report
                .files
                .iter()
                .filter(|file| matches!(file.status, SkillFileStatus::Missing | SkillFileStatus::Changed))
                .count();
            write_output(renderer.render_skill(&report)?)?;
            if drifted == 0 {
                Ok(())
            } else {
                Err(ApplicationError::from_command(CommandError::SkillDrift { count: drifted }).into())
            }
        }
    }
}

/// Compare each embedded skill file with its copy under `root`, and write the
/// copies that are missing or differ unless `check` is set.
fn install_skill(root: &Path, check: bool) -> CResult<SkillReport> {
    let mut files = Vec::new();
    for directory in SKILL_DIRECTORIES {
        for (name, content) in SKILL_FILES {
            let relative = format!("{directory}/{SKILL_NAME}/{name}");
            let path = root.join(&relative);
            let existing = match fs::read(&path) {
                Ok(bytes) => Some(bytes),
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(source) => return Err(CommandError::ReadSkill { path, source }),
            };
            let status = match (existing, check) {
                (Some(bytes), _) if bytes == content.as_bytes() => SkillFileStatus::Unchanged,
                (None, true) => SkillFileStatus::Missing,
                (Some(_), true) => SkillFileStatus::Changed,
                (existing, false) => {
                    if let Some(parent) = path.parent() {
                        fs::create_dir_all(parent)
                            .map_err(|source| CommandError::WriteSkill { path: parent.to_owned(), source })?;
                    }
                    fs::write(&path, content)
                        .map_err(|source| CommandError::WriteSkill { path: path.clone(), source })?;
                    if existing.is_some() { SkillFileStatus::Updated } else { SkillFileStatus::Created }
                }
            };
            files.push(SkillFile { path: relative, status });
        }
    }
    let action = if check { "checked" } else { "installed" };
    Ok(SkillReport { action, root: root.to_string_lossy().into_owned(), files })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// List every file under `directory`, as paths relative to `base` with
    /// `/` separators, in sorted order.
    fn source_files(base: &Path, directory: &Path, found: &mut Vec<String>) {
        for entry in fs::read_dir(directory).expect("skill source directory reads") {
            let path = entry.expect("skill source entry reads").path();
            if path.is_dir() {
                source_files(base, &path, found);
            } else {
                let relative = path.strip_prefix(base).expect("entry is under the base");
                let parts = relative.iter().map(|part| part.to_string_lossy()).collect::<Vec<_>>();
                found.push(parts.join("/"));
            }
        }
    }

    #[test]
    fn embedded_files_match_the_source_directory() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../skills")
            .join(SKILL_NAME);
        let mut found = Vec::new();
        source_files(&base, &base, &mut found);
        found.sort();
        let mut embedded = SKILL_FILES
            .iter()
            .map(|(name, _)| (*name).to_owned())
            .collect::<Vec<_>>();
        embedded.sort();
        assert_eq!(found, embedded, "add each file in skills/{SKILL_NAME}/ to SKILL_FILES");
        for (name, content) in SKILL_FILES {
            let source = fs::read_to_string(base.join(name)).expect("skill source file reads");
            assert_eq!(&source, content, "{name} is embedded from its source file");
        }
    }
}
