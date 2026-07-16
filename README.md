# Arc Lightning (`arcl`)

A task tracking CLI for developers & their agents.

## Usage

Arc Lightning discovers the enclosing non-bare Git worktree, so initialization can
run from the repository root or any directory below it.

```sh
# From a new or existing Git worktree
arcl init

# Initialize with the version-controlled snapshot feature enabled
arcl init --snapshot
```

`init` creates the following structure in your project:

```sh
.arcl
├── .gitignore      # ignores sqlite, temp files & conflict artifacts
├── config.toml
└── arcl.db
```
