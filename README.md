# Arc Lightning (`arcl`)

A local-first project planning and execution CLI for developers and their agents.
Arc Lightning keeps the live tracker in `.arcl/arcl.db` and can optionally use a
Git worktree for repository-native project files.

## Quick start

Arc Lightning initializes ordinary directories directly and discovers the nearest
project when commands run from any descendant directory. Inside a non-bare Git
worktree, initialization uses the worktree root.

```sh
# From a new or existing project directory
arcl init
```

`init` creates the following structure in your project:

```sh
.arcl
├── .gitignore      # ignores sqlite, temp files & conflict artifacts
├── config.toml
└── arcl.db
```

## Daily workflow

Capture a thought, promote it to an owned specification, create a plan, and add
work to that plan:

```sh
capture=$(arcl --json capture create "Improve import errors" --body "Make failures easier to fix.")
capture_id=$(printf '%s' "$capture" | jq -r '.data.capture.id')
spec=$(arcl --json capture promote "$capture_id" spec --acceptance-criteria "Imports reject invalid records.")
spec_id=$(printf '%s' "$spec" | jq -r '.data.spec.id')
plan=$(arcl --json plan create "Import validation" --spec "$spec_id")
plan_id=$(printf '%s' "$plan" | jq -r '.data.plan.id')
arcl task create "Validate records" --plan "$plan_id" --priority high
arcl ready
```

Small work can move directly from a capture to a task:

```sh
arcl capture promote arcl-c-… task --priority high
```

## Local development

Build and check the CLI from the repository root:

```sh
cargo build
cargo test --workspace --all-features
```

Install the pnpm workspace and run the documentation site:

```sh
pnpm install
pnpm dev:website
```

Documentation content lives in `apps/website/src/content/docs/`. Verify the
site with `pnpm --filter @arclightning/website build`.
