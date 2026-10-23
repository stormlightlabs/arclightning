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
            import_graph(&workspace_root, &worktree_root, &mut database)?;
        }
        WorkspaceCommand::Sync => {
            sync_workspace(&workspace_root, &worktree_root, &mut database)?;
        }
        WorkspaceCommand::Status => {
            let vcs = match GixVcs::discover(&worktree_root) {
                Ok(vcs) => Some(vcs),
                Err(VcsError::Discovery { .. }) => None,
                Err(error) => return Err(error.into()),
            };
            let status = workspace_status(&workspace_root, &database, vcs.as_ref().map(|vcs| vcs as &dyn Vcs))?;
            return Ok(Some(status));
        }
    }
    Ok(None)
}
