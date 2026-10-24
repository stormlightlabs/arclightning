use super::auto_sync::lock_project;
use super::support::*;
use super::*;

/// Run one workspace command under the project's sync lock. Only `status`
/// returns a report to render. The warnings are the blocker rule violations
/// an import, sync or resolve loaded because it did not introduce them.
pub(super) fn exec_workspace(command: WorkspaceCommand) -> CResult<(Option<WorkspaceStatus>, Vec<String>)> {
    let (worktree_root, workspace_root) = workspace_paths()?;
    let _lock = lock_project(&worktree_root)?;
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
            let outcome = import_graph(&workspace_root, &worktree_root, &mut database, as_dyn(&vcs))?;
            return Ok((None, outcome.warnings));
        }
        WorkspaceCommand::Sync => {
            let vcs = discover_vcs(&worktree_root)?;
            let outcome = sync_workspace(&workspace_root, &worktree_root, &mut database, as_dyn(&vcs))?;
            return Ok((None, outcome.warnings));
        }
        WorkspaceCommand::Resolve { keep, paths } => {
            let vcs = discover_vcs(&worktree_root)?;
            let keep = match keep {
                KeepSide::Database => Side::Database,
                KeepSide::Workspace => Side::Workspace,
            };
            let current_dir = std::env::current_dir().map_err(|source| CommandError::CurrentDirectory { source })?;
            let outcome = resolve_workspace(
                &workspace_root,
                &worktree_root,
                &current_dir,
                &mut database,
                as_dyn(&vcs),
                &paths,
                keep,
            )?;
            return Ok((None, outcome.warnings));
        }
        WorkspaceCommand::Status => {
            let vcs = discover_vcs(&worktree_root)?;
            let status = workspace_status(&workspace_root, &database, as_dyn(&vcs))?;
            return Ok((Some(status), Vec::new()));
        }
    }
    Ok((None, Vec::new()))
}

/// Open the Git worktree at `worktree_root`, or return `None` when the
/// project is not in one.
pub(super) fn discover_vcs(worktree_root: &Path) -> CResult<Option<GixVcs>> {
    match GixVcs::discover(worktree_root) {
        Ok(vcs) => Ok(Some(vcs)),
        Err(VcsError::Discovery { .. }) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Borrow a discovered worktree as the `Vcs` trait object the workspace
/// functions take.
pub(super) fn as_dyn(vcs: &Option<GixVcs>) -> Option<&dyn Vcs> {
    vcs.as_ref().map(|vcs| vcs as &dyn Vcs)
}
