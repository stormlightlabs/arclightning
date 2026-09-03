use super::*;
use super::{
    error::{IResult, InitError},
    support::nearest_project_root,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Initialization {
    pub(super) root: PathBuf,
    pub(super) snapshot_enabled: bool,
}

pub(super) fn initialize(start: &Path, snapshot: bool) -> IResult<Initialization> {
    let root = initialization_root(start)?;
    let arcl_directory = root.join(ARCL_DIRECTORY);
    fs::create_dir_all(&arcl_directory)
        .map_err(|source| InitError::CreateDirectory { path: arcl_directory.clone(), source })?;
    let config_path = arcl_directory.join(CONFIG_FILE);
    let config = load_or_create_config(&config_path, snapshot)?;
    ensure_gitignore(&arcl_directory.join(GITIGNORE_FILE))?;
    let database_path = arcl_directory.join(DATABASE_FILE);
    Database::open(&database_path).map_err(|source| InitError::OpenDatabase { path: database_path, source })?;
    if config.snapshot.enabled {
        ensure_snapshot_layout(&root, &config)?;
    }
    Ok(Initialization { root, snapshot_enabled: config.snapshot.enabled })
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

fn load_or_create_config(path: &Path, snapshot: bool) -> IResult<ProjectConfig> {
    match fs::read_to_string(path) {
        Ok(input) => {
            let mut config = ProjectConfig::parse(&input)
                .map_err(|source| InitError::InvalidConfig { path: path.to_owned(), source })?;
            if snapshot && !config.snapshot.enabled {
                config.snapshot.enabled = true;
                let rendered = config
                    .render()
                    .map_err(|source| InitError::RenderConfig { path: path.to_owned(), source })?;
                fs::write(path, rendered).map_err(|source| InitError::WriteConfig { path: path.to_owned(), source })?;
            }
            Ok(config)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let mut config = ProjectConfig::default();
            config.snapshot.enabled = snapshot;
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

fn ensure_snapshot_layout(root: &Path, config: &ProjectConfig) -> IResult<()> {
    let snapshot_root = resolve_snapshot_root(root, &config.snapshot.path)
        .map_err(|source| InitError::InvalidConfig { path: root.join(ARCL_DIRECTORY).join(CONFIG_FILE), source })?;
    fs::create_dir_all(&snapshot_root)
        .map_err(|source| InitError::CreateSnapshotDirectory { path: snapshot_root.clone(), source })?;
    for directory in SNAPSHOT_DIRECTORIES {
        let path = snapshot_root.join(directory);
        fs::create_dir_all(&path).map_err(|source| InitError::CreateSnapshotDirectory { path, source })?;
    }
    let manifest_path = snapshot_root.join("manifest.toml");
    if !manifest_path.is_file() {
        let manifest = encode_manifest(&SnapshotManifest::default())
            .map_err(|source| InitError::RenderSnapshotManifest { path: manifest_path.clone(), source })?;
        create_file(&manifest_path, manifest.as_bytes())
            .map_err(|source| InitError::WriteSnapshotManifest { path: manifest_path, source })?;
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
