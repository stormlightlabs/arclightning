# Arc Lightning (`arcl`)

A Git-aware task tracking CLI for developers and their agents. Arc Lightning keeps
the live tracker in `.arcl/arcl.db` and can be used from any directory inside a
non-bare Git worktree.

## Quick start

Arc Lightning discovers the enclosing non-bare Git worktree, so initialization can
run from the repository root or any directory below it.

```sh
# From a new or existing Git worktree
arcl init
```

`init` creates the following structure in your project:

```sh
.arcl
├── .gitignore      # ignores sqlite, temp files & conflict artifacts
├── config.toml
└── arcl.db
```

## Daily Workflow

You can capture an idea, promote it once it has a specification, then create the
first milestone and task:

```sh
arcl idea create "Improve import errors" --description "Make failures easier to fix."
arcl idea promote arcl-i-… --spec specs/import-errors.md
arcl milestone create "Storage" --epic arcl-e-…
arcl task create "Validate records" --milestone arcl-m-… --priority high
arcl ready
```

## Local development

Build and check the CLI from the repository root:

```sh
cargo build
cargo test --workspace --all-features
```

Run the documentation site locally with pnpm:

```sh
pnpm --dir website install
pnpm --dir website dev
```

Documentation content lives in `website/src/content/docs/`. The site build can
be verified with `pnpm --dir website build`.
