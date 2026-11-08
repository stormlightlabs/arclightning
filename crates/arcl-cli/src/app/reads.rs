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

/// `arcl context <task-id>`.
pub(super) fn read_context(task_id: &str, renderer: &Renderer) -> anyhow::Result<Option<String>> {
    let id = parse_task_id(task_id).map_err(ApplicationError::from_command)?;
    let database = open_database().map_err(ApplicationError::from_command)?;
    let context = database.planning_context(id).map_err(ApplicationError::from_command)?;
    Ok(renderer.render_planning_context(&context)?)
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
/// `output` is a symbolic link, the page goes to the file it points to and
/// the link stays.
pub(super) fn exec_roadmap(output: Option<&Path>, renderer: &Renderer) -> anyhow::Result<()> {
    let database = open_database().map_err(ApplicationError::from_command)?;
    let nodes = connected_tree(&database, None, TreeArgs::default()).map_err(ApplicationError::from_command)?;
    let memberships = database.release_memberships().map_err(ApplicationError::from_command)?;
    let roadmap = Roadmap::new(&nodes, &memberships);
    let Some(path) = output else {
        return write_output(renderer.render_roadmap(&roadmap)?);
    };
    let target = roadmap_target(path).map_err(ApplicationError::from_command)?;
    write_replacing(&target, &format!("{}\n", roadmap.markdown())).map_err(ApplicationError::from_command)?;
    write_output(renderer.render_roadmap_written(path)?)
}

/// The file `arcl roadmap --output <path>` writes: `path`, or the file at the
/// end of the symbolic links starting at `path`. Fails when that is a
/// directory or another non-file, or when the path has no file name.
fn roadmap_target(path: &Path) -> CResult<PathBuf> {
    /// The most symbolic links followed, as Linux allows.
    const MAX_LINKS: usize = 40;
    let not_file = || CommandError::OutputNotFile { path: path.to_owned() };
    let mut target = path.to_owned();
    for _ in 0..MAX_LINKS {
        match fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let link = fs::read_link(&target)
                    .map_err(|source| CommandError::ReadSkill { path: target.clone(), source })?;
                target = target
                    .parent()
                    .map_or_else(|| link.clone(), |parent| parent.join(&link));
            }
            Ok(metadata) if metadata.is_file() => return Ok(target),
            Ok(_) => return Err(not_file()),
            Err(error) if absent(&error) => return target.file_name().map(|_| target.clone()).ok_or_else(not_file),
            Err(source) => return Err(CommandError::WriteSkill { path: target, source }),
        }
    }
    Err(CommandError::WriteSkill {
        path: path.to_owned(),
        source: std::io::Error::other(format!("more than {MAX_LINKS} symbolic links")),
    })
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
