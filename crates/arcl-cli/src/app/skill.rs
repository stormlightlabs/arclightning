//! `arcl skill install`: write the Arc Lightning skill into a project.
//!
//! The skill files are compiled into the binary from `skills/arclightning/`,
//! the one source directory, so an installed copy always matches the
//! repository copy the binary was built from.

use super::support::{nearest_project_root, write_output};
use super::*;

/// `skills/arclightning/SKILL.md`. `arcl prime` prints the loop it marks.
pub(super) const SKILL_MARKDOWN: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../skills/arclightning/SKILL.md"
));

/// The agent skill directories, relative to the project root, that receive
/// the skill.
const SKILL_DIRECTORIES: &[&str] = &[".claude/skills", ".agents/skills"];

/// The skill's directory name under each agent skill directory.
const SKILL_NAME: &str = "arclightning";

/// Every file in `skills/arclightning/`, as a path relative to that directory
/// and its contents. The `embedded_files_match_the_source_directory` test
/// fails when a file is added there but not here.
const SKILL_FILES: &[(&str, &str)] = &[("SKILL.md", SKILL_MARKDOWN)];

/// One skill file to compare and maybe write, after its path passed the
/// checks in [`check_target`].
struct SkillTarget {
    /// The path relative to the project root, as reported.
    relative: String,
    /// The path with every symbolic link in its directories resolved. Reads
    /// and writes go through it, and two targets that reach the same file
    /// through a linked directory share it.
    resolved: PathBuf,
    /// The embedded contents.
    content: &'static str,
}

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
/// copies that are missing or differ unless `check` is set. Every target is
/// checked before any file is written.
fn install_skill(root: &Path, check: bool) -> CResult<SkillReport> {
    let canonical_root =
        fs::canonicalize(root).map_err(|source| CommandError::ReadSkill { path: root.to_owned(), source })?;
    let targets = SKILL_DIRECTORIES
        .iter()
        .flat_map(|directory| {
            SKILL_FILES
                .iter()
                .map(move |(name, content)| (format!("{directory}/{SKILL_NAME}/{name}"), *content))
        })
        .map(|(relative, content)| check_target(root, &canonical_root, relative, content))
        .collect::<CResult<Vec<_>>>()?;

    let mut files = Vec::new();
    let mut writes: Vec<&SkillTarget> = Vec::new();
    for target in &targets {
        // A linked directory can make a later target the file an earlier one
        // writes; once written, it matches.
        if writes.iter().any(|planned| planned.resolved == target.resolved) {
            files.push(SkillFile { path: target.relative.clone(), status: SkillFileStatus::Unchanged });
            continue;
        }
        let existing = match fs::read(&target.resolved) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(source) => return Err(CommandError::ReadSkill { path: target.resolved.clone(), source }),
        };
        let status = match (existing, check) {
            (Some(bytes), _) if bytes == target.content.as_bytes() => SkillFileStatus::Unchanged,
            (None, true) => SkillFileStatus::Missing,
            (Some(_), true) => SkillFileStatus::Changed,
            (None, false) => SkillFileStatus::Created,
            (Some(_), false) => SkillFileStatus::Updated,
        };
        if matches!(status, SkillFileStatus::Created | SkillFileStatus::Updated) {
            writes.push(target);
        }
        files.push(SkillFile { path: target.relative.clone(), status });
    }
    for target in writes {
        write_replacing(&target.resolved, target.content)?;
    }
    let action = if check { SkillAction::Checked } else { SkillAction::Installed };
    Ok(SkillReport { action, root: root.to_string_lossy().into_owned(), files })
}

/// Check one target before anything is written: the file must be a regular
/// file or absent, not a symbolic link, and its directory must resolve to a
/// directory under `canonical_root`.
fn check_target(root: &Path, canonical_root: &Path, relative: String, content: &'static str) -> CResult<SkillTarget> {
    let path = root.join(&relative);
    match fs::symlink_metadata(&path) {
        Ok(meta) if meta.file_type().is_symlink() => return Err(CommandError::SkillSymlink { path }),
        Ok(meta) if !meta.is_file() => return Err(CommandError::SkillNotFile { path }),
        Ok(_) => {}
        // A parent that is not a directory is reported by the walk below.
        Err(error) if absent(&error) => {}
        Err(source) => return Err(CommandError::ReadSkill { path, source }),
    }
    let (Some(parent), Some(file_name)) = (path.parent(), path.file_name()) else {
        return Err(CommandError::SkillNotFile { path });
    };
    // Resolve the nearest directory that exists; the rest is created later.
    let mut existing = parent;
    let mut created = Vec::new();
    let resolved_parent = loop {
        match fs::symlink_metadata(existing) {
            Ok(_) => {
                let canonical = fs::canonicalize(existing)
                    .map_err(|source| CommandError::ReadSkill { path: existing.to_owned(), source })?;
                if !canonical.is_dir() {
                    return Err(CommandError::SkillNotDirectory { path: existing.to_owned() });
                }
                break canonical;
            }
            Err(error) if absent(&error) => {
                let (Some(name), Some(up)) = (existing.file_name(), existing.parent()) else {
                    return Err(CommandError::ReadSkill { path: existing.to_owned(), source: error });
                };
                created.push(name);
                existing = up;
            }
            Err(source) => return Err(CommandError::ReadSkill { path: existing.to_owned(), source }),
        }
    };
    if !resolved_parent.starts_with(canonical_root) {
        return Err(CommandError::SkillOutsideProject { path, root: canonical_root.to_owned() });
    }
    let mut resolved = resolved_parent;
    resolved.extend(created.iter().rev());
    resolved.push(file_name);
    Ok(SkillTarget { relative, resolved, content })
}

/// Write `content` to a temporary file beside `path`, then rename it over
/// `path`, so a reader sees the old file or the new one and never a partial
/// write. The temporary file is created exclusively, so it never follows a
/// link.
fn write_replacing(path: &Path, content: &str) -> CResult<()> {
    let (Some(parent), Some(file_name)) = (path.parent(), path.file_name()) else {
        return Err(CommandError::SkillNotFile { path: path.to_owned() });
    };
    fs::create_dir_all(parent).map_err(|source| CommandError::WriteSkill { path: parent.to_owned(), source })?;
    let temporary = parent.join(format!(
        ".{}.arcl-{}.tmp",
        file_name.to_string_lossy(),
        std::process::id()
    ));
    let written = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .and_then(|mut file| {
            file.write_all(content.as_bytes())?;
            file.sync_all()
        })
        .and_then(|()| fs::rename(&temporary, path));
    if let Err(source) = written {
        let _ = fs::remove_file(&temporary);
        return Err(CommandError::WriteSkill { path: path.to_owned(), source });
    }
    Ok(())
}

/// Whether a metadata lookup failed because the path does not exist, either
/// missing or below a file.
fn absent(error: &io::Error) -> bool {
    matches!(error.kind(), io::ErrorKind::NotFound | io::ErrorKind::NotADirectory)
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
    }
}
