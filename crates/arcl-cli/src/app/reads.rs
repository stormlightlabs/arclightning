//! The read commands of the CLI. `arcl serve` answers the same reads through
//! GraphQL, in [`super::rest`], with the same envelopes and errors.

use super::query::{connected_tree, list_connected, resolve_ready_filter, show_record};
use super::skill::{absent, write_replacing};
use super::support::*;
use super::*;

/// `arcl show <id>`.
pub(super) fn read_show(id: &str, renderer: &Renderer) -> anyhow::Result<Option<String>> {
    let database = open_database().map_err(ApplicationError::from_command)?;
    Ok(show_record(&database, id, renderer).map_err(ApplicationError::from_command)?)
}

/// `arcl list`.
pub(super) fn read_list(filters: ListArgs, renderer: &Renderer) -> anyhow::Result<Option<String>> {
    let database = open_database().map_err(ApplicationError::from_command)?;
    let mut summaries = list_connected(&database, filters).map_err(ApplicationError::from_command)?;
    let ids = display_ids(&database, renderer).map_err(ApplicationError::from_command)?;
    for summary in &mut summaries {
        summary.id = ids.short(&summary.id).to_owned();
    }
    Ok(renderer.render_connected_summaries(&summaries)?)
}

/// `arcl explain <task-id>`.
pub(super) fn read_explain(task_id: &str, renderer: &Renderer) -> anyhow::Result<Option<String>> {
    let id = parse_task_id(task_id).map_err(ApplicationError::from_command)?;
    let database = open_database().map_err(ApplicationError::from_command)?;
    let view = database
        .planning_task_view(id)
        .map_err(ApplicationError::from_command)?;
    let ids = display_ids(&database, renderer).map_err(ApplicationError::from_command)?;
    Ok(renderer.render_planning_task_view(&view, &ids)?)
}

/// `arcl context <task-id>`, with archived related tasks only when `all` is
/// set.
pub(super) fn read_context(task_id: &str, all: bool, renderer: &Renderer) -> anyhow::Result<Option<String>> {
    let id = parse_task_id(task_id).map_err(ApplicationError::from_command)?;
    let database = open_database().map_err(ApplicationError::from_command)?;
    let mut context = database.planning_context(id).map_err(ApplicationError::from_command)?;
    if !all {
        context.hide_archived();
    }
    Ok(renderer.render_planning_context(&context)?)
}

/// `arcl compact`: archive the tasks completed before `before`, under
/// `phase` when given, and print what was archived. With `dry_run` it prints
/// what would be and writes nothing.
pub(super) fn exec_compact(
    before: String, phase: Option<String>, include_undated: bool, dry_run: bool, renderer: &Renderer,
) -> anyhow::Result<Option<String>> {
    let phase_id = parse_optional_id::<PhaseId>(phase).map_err(ApplicationError::from_command)?;
    let mut database = open_database().map_err(ApplicationError::from_command)?;
    let request = CompactionRequest { before, phase_id, include_undated, dry_run };
    let compaction = database.compact(&request).map_err(ApplicationError::from_command)?;
    let ids = display_ids(&database, renderer).map_err(ApplicationError::from_command)?;
    Ok(renderer.render_compaction(&compaction, &ids)?)
}

/// `arcl ready`.
pub(super) fn read_ready(filters: ReadyArgs, renderer: &Renderer) -> anyhow::Result<Option<String>> {
    let database = open_database().map_err(ApplicationError::from_command)?;
    let filter = resolve_ready_filter(filters).map_err(ApplicationError::from_command)?;
    let tasks = database
        .ready_planning_tasks_filtered(&filter)
        .map_err(ApplicationError::from_command)?;
    Ok(renderer.render_planning_ready_tasks(&tasks)?)
}

/// `arcl tree`, from the record `root` names or of every record, shaped by
/// `options`.
pub(super) fn read_tree(root: Option<&str>, options: TreeArgs, renderer: &Renderer) -> anyhow::Result<Option<String>> {
    let root = root
        .map(|root| resolve_record_id(root, None))
        .transpose()
        .map_err(ApplicationError::from_command)?;
    let database = open_database().map_err(ApplicationError::from_command)?;
    let mut nodes = connected_tree(&database, root.as_deref(), options).map_err(ApplicationError::from_command)?;
    let ids = display_ids(&database, renderer).map_err(ApplicationError::from_command)?;
    shorten_tree(&mut nodes, &ids);
    let view = TreeView {
        rooted: root.is_some(),
        all: options.all,
        ascii: options.ascii,
        depth: options.depth,
        style: options.style,
    };
    Ok(renderer.render_connected_tree(&nodes, view)?)
}

/// `arcl roadmap`: print the roadmap page, or with `output` write it to that
/// file, replacing it whole, and print a confirmation. The file holds the
/// same bytes the command prints without `output` and without `--json`. When
/// `output` goes through a symbolic link, the page goes to the file the link
/// points to and the link stays, as long as that file is under the project
/// root.
pub(super) fn exec_roadmap(output: Option<&Path>, renderer: &Renderer) -> anyhow::Result<()> {
    let database = open_database().map_err(ApplicationError::from_command)?;
    // Archived tasks are done work, so the roadmap counts them, as `arcl
    // tree` does when it leaves them out.
    let options = TreeArgs { all: true, ..TreeArgs::default() };
    let nodes = connected_tree(&database, None, options).map_err(ApplicationError::from_command)?;
    let memberships = database.release_memberships().map_err(ApplicationError::from_command)?;
    let roadmap = Roadmap::new(&nodes, &memberships);
    let Some(path) = output else {
        return write_output(renderer.render_roadmap(&roadmap)?);
    };
    let project = ProjectDir::discover_current().map_err(ApplicationError::from_command)?;
    let target = roadmap_target(path, project.root()).map_err(ApplicationError::from_command)?;
    write_replacing(&target, &format!("{}\n", roadmap.markdown())).map_err(ApplicationError::from_command)?;
    write_output(renderer.render_roadmap_written(path)?)
}

/// The file `arcl roadmap --output <path>` writes, with every symbolic link
/// in `path` resolved. A path with no link in it may name any file. A path
/// that goes through a link must resolve under `root`, even when the file at
/// the end does not exist yet, so a link a cloned repository carries cannot
/// aim the write at a file outside it. Fails when the file is a directory or
/// another non-file, or when the path has no file name.
fn roadmap_target(path: &Path, root: &Path) -> CResult<PathBuf> {
    /// The most symbolic links followed, as Linux allows.
    const MAX_LINKS: usize = 40;
    let not_file = || CommandError::OutputNotFile { path: path.to_owned() };
    // `metadata` follows every link, including kernel links such as
    // `/dev/stdout` that name no path, so a pipe or device is refused here.
    match fs::metadata(path) {
        Ok(metadata) if !metadata.is_file() => return Err(not_file()),
        Ok(_) => {}
        Err(error) if absent(&error) => {}
        Err(source) => return Err(CommandError::WriteSkill { path: path.to_owned(), source }),
    }
    let current = std::env::current_dir().map_err(|source| ProjectError::CurrentDirectory { source })?;
    let mut target = current.join(path);
    let mut followed = 0;
    loop {
        match fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                followed += 1;
                if followed > MAX_LINKS {
                    return Err(CommandError::WriteSkill {
                        path: path.to_owned(),
                        source: std::io::Error::other(format!("more than {MAX_LINKS} symbolic links")),
                    });
                }
                let link = fs::read_link(&target)
                    .map_err(|source| CommandError::ReadSkill { path: target.clone(), source })?;
                target = target
                    .parent()
                    .map_or_else(|| link.clone(), |parent| parent.join(&link));
            }
            Ok(metadata) if metadata.is_file() => break,
            Ok(_) => return Err(not_file()),
            Err(error) if absent(&error) && target.file_name().is_some() => break,
            Err(error) if absent(&error) => return Err(not_file()),
            Err(source) => return Err(CommandError::WriteSkill { path: target, source }),
        }
    }
    let resolved = resolved_location(&target)?;
    if resolved != lexical(&current.join(path)) {
        let canonical_root =
            fs::canonicalize(root).map_err(|source| CommandError::ReadSkill { path: root.to_owned(), source })?;
        if !resolved.starts_with(&canonical_root) {
            return Err(CommandError::OutputLinkOutside {
                path: path.to_owned(),
                target: resolved,
                root: canonical_root,
            });
        }
    }
    Ok(resolved)
}

/// Where the absolute `path` lands with every symbolic link in it resolved:
/// the canonical form of its nearest existing ancestor, followed by the names
/// that do not exist yet. Those names hold no links, so `..` among them
/// drops the name before it.
fn resolved_location(path: &Path) -> CResult<PathBuf> {
    let mut existing = path;
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(existing) {
            Ok(_) => {
                let mut resolved = fs::canonicalize(existing)
                    .map_err(|source| CommandError::ReadSkill { path: existing.to_owned(), source })?;
                resolved.extend(missing.iter().rev());
                return Ok(lexical(&resolved));
            }
            Err(error) if absent(&error) => {
                let (Some(name), Some(up)) = (existing.components().next_back(), existing.parent()) else {
                    return Err(CommandError::ReadSkill { path: existing.to_owned(), source: error });
                };
                missing.push(name.as_os_str());
                existing = up;
            }
            Err(source) => return Err(CommandError::ReadSkill { path: existing.to_owned(), source }),
        }
    }
}

/// `path` with `.` dropped and each `..` removing the name before it,
/// without reading the file system.
fn lexical(path: &Path) -> PathBuf {
    let mut normal = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normal.pop();
            }
            other => normal.push(other),
        }
    }
    normal
}

/// Replace each node's ID, and each blocker ID, with its short ID.
fn shorten_tree(nodes: &mut [ConnectedTreeNode], ids: &ShortIds) {
    for node in nodes {
        node.id = ids.short(&node.id).to_owned();
        for blocker in &mut node.blocked_by {
            *blocker = ids.short(blocker).to_owned();
        }
        shorten_tree(&mut node.children, ids);
    }
}
