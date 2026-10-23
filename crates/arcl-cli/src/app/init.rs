use super::*;
use super::{
    error::{IResult, InitError},
    support::nearest_project_root,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Initialization {
    pub(super) root: PathBuf,
    pub(super) workspace_enabled: bool,
}

pub(super) fn initialize(start: &Path, workspace: bool) -> IResult<Initialization> {
    let root = initialization_root(start)?;
    let arcl_directory = root.join(ARCL_DIRECTORY);
    fs::create_dir_all(&arcl_directory)
        .map_err(|source| InitError::CreateDirectory { path: arcl_directory.clone(), source })?;
    let config_path = arcl_directory.join(CONFIG_FILE);
    let config = load_or_create_config(&config_path, workspace)?;
    ensure_gitignore(&arcl_directory.join(GITIGNORE_FILE))?;
    let database_path = arcl_directory.join(DATABASE_FILE);
    Database::open(&database_path).map_err(|source| InitError::OpenDatabase { path: database_path, source })?;
    if config.workspace.enabled {
        ensure_workspace_layout(&root, &config)?;
    }
    Ok(Initialization { root, workspace_enabled: config.workspace.enabled })
}

fn initialization_root(start: &Path) -> IResult<PathBuf> {
    if let Some(root) = nearest_project_root(start) {
        return Ok(root);
    }
    match GixVcs::discover(start) {
        Ok(vcs) => Ok(vcs.worktree_root()?.to_owned()),
        Err(VcsError::Discovery { .. }) => Ok(start.to_owned()),
        Err(error) => Err(InitError::Vcs(error)),
    }
}

fn load_or_create_config(path: &Path, workspace: bool) -> IResult<ProjectConfig> {
    match fs::read_to_string(path) {
        Ok(input) => {
            let mut config = ProjectConfig::parse(&input)
                .map_err(|source| InitError::InvalidConfig { path: path.to_owned(), source })?;
            if workspace && !config.workspace.enabled {
                config.workspace.enabled = true;
                let rendered = config
                    .render()
                    .map_err(|source| InitError::RenderConfig { path: path.to_owned(), source })?;
                fs::write(path, rendered).map_err(|source| InitError::WriteConfig { path: path.to_owned(), source })?;
            }
            Ok(config)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let mut config = ProjectConfig::default();
            config.workspace.enabled = workspace;
            let rendered = config
                .render()
                .map_err(|source| InitError::RenderConfig { path: path.to_owned(), source })?;
            create_file(path, rendered.as_bytes())
                .map_err(|source| InitError::WriteConfig { path: path.to_owned(), source })?;
            Ok(config)
        }
        Err(source) => Err(InitError::ReadConfig { path: path.to_owned(), source }),
    }
}

fn ensure_workspace_layout(root: &Path, config: &ProjectConfig) -> IResult<()> {
    let workspace_root = resolve_workspace_root(root, &config.workspace.path)
        .map_err(|source| InitError::InvalidConfig { path: root.join(ARCL_DIRECTORY).join(CONFIG_FILE), source })?;
    fs::create_dir_all(&workspace_root)
        .map_err(|source| InitError::CreateWorkspaceDirectory { path: workspace_root.clone(), source })?;
    for directory in WORKSPACE_DIRECTORIES {
        let path = workspace_root.join(directory);
        fs::create_dir_all(&path).map_err(|source| InitError::CreateWorkspaceDirectory { path, source })?;
    }
    let manifest_path = workspace_root.join("manifest.toml");
    if !manifest_path.is_file() {
        let manifest = encode_manifest(&WorkspaceManifest::default())
            .map_err(|source| InitError::RenderWorkspaceManifest { path: manifest_path.clone(), source })?;
        create_file(&manifest_path, manifest.as_bytes())
            .map_err(|source| InitError::WriteWorkspaceManifest { path: manifest_path, source })?;
    }
    Ok(())
}

fn ensure_gitignore(path: &Path) -> IResult<()> {
    let existing = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let content = required_gitignore();
            create_file(path, content.as_bytes())
                .map_err(|source| InitError::WriteGitignore { path: path.to_owned(), source })?;
            return Ok(());
        }
        Err(source) => return Err(InitError::ReadGitignore { path: path.to_owned(), source }),
    };
    let mut updated = existing.clone();
    for entry in REQUIRED_GITIGNORE_ENTRIES {
        if updated.lines().any(|line| line.trim() == *entry) {
            continue;
        }
        if !updated.is_empty() && !updated.ends_with('\n') {
            updated.push('\n');
        }
        updated.push_str(entry);
        updated.push('\n');
    }
    if updated != existing {
        fs::write(path, updated).map_err(|source| InitError::WriteGitignore { path: path.to_owned(), source })?;
    }
    Ok(())
}

fn required_gitignore() -> String {
    let mut content = REQUIRED_GITIGNORE_ENTRIES.join("\n");
    content.push('\n');
    content
}

fn create_file(path: &Path, content: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(content)?;
    file.flush()
}
