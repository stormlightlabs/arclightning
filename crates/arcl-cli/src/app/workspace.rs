use super::support::*;
use super::*;

/// Run one workspace command. Only `status` returns a report to render.
pub(super) fn exec_workspace(command: WorkspaceCommand) -> CResult<Option<WorkspaceStatus>> {
    let (worktree_root, workspace_root) = workspace_paths()?;
    let mut database = open_database()?;
    match command {
        WorkspaceCommand::Export => {
            let base = database.workspace_base()?;
            let files = export_graph_with_base(&workspace_root, &database.connected_graph()?, &base)?;
            let base = files
                .iter()
                .map(|file| arcl_store::WorkspaceBaseFile {
                    path: file.path.to_string_lossy().into_owned(),
                    content: file.content.clone(),
                })
                .collect::<Vec<_>>();
            database.replace_workspace_base(&base)?;
        }
        WorkspaceCommand::Import => {
            let vcs = discover_vcs(&worktree_root)?;
            import_graph(&workspace_root, &worktree_root, &mut database, as_dyn(&vcs))?;
        }
        WorkspaceCommand::Sync => {
            let vcs = discover_vcs(&worktree_root)?;
            sync_workspace(&workspace_root, &worktree_root, &mut database, as_dyn(&vcs))?;
        }
        WorkspaceCommand::Resolve { keep, paths } => {
            let vcs = discover_vcs(&worktree_root)?;
            let keep = match keep {
                KeepSide::Database => Side::Database,
                KeepSide::Workspace => Side::Workspace,
            };
            resolve_workspace(
                &workspace_root,
                &worktree_root,
                &mut database,
                as_dyn(&vcs),
                &paths,
                keep,
            )?;
        }
        WorkspaceCommand::Status => {
            let vcs = discover_vcs(&worktree_root)?;
            let status = workspace_status(&workspace_root, &database, as_dyn(&vcs))?;
            return Ok(Some(status));
        }
    }
    Ok(None)
}

/// Open the Git worktree at `worktree_root`, or return `None` when the
/// project is not in one.
fn discover_vcs(worktree_root: &Path) -> CResult<Option<GixVcs>> {
    match GixVcs::discover(worktree_root) {
        Ok(vcs) => Ok(Some(vcs)),
        Err(VcsError::Discovery { .. }) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn as_dyn(vcs: &Option<GixVcs>) -> Option<&dyn Vcs> {
    vcs.as_ref().map(|vcs| vcs as &dyn Vcs)
}
