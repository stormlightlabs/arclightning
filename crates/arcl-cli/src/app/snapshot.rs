use super::support::*;
use super::*;

pub(super) fn exec_snapshot(command: SnapshotCommand) -> CResult<()> {
    let (worktree_root, snapshot_root) = snapshot_paths()?;
    let mut database = open_database()?;
    match command {
        SnapshotCommand::Export => {
            let base = database.snapshot_base()?;
            let files = export_graph_with_base(&snapshot_root, &database.connected_graph()?, &base)?;
            let base = files
                .iter()
                .map(|file| arcl_store::SnapshotBaseFile {
                    path: file.path.to_string_lossy().into_owned(),
                    content: file.content.clone(),
                })
                .collect::<Vec<_>>();
            database.replace_snapshot_base(&base)?;
        }
        SnapshotCommand::Import => {
            import_graph(&snapshot_root, &worktree_root, &mut database)?;
        }
    }
    Ok(())
}
