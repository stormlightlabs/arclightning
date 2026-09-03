---
title: Quick Start
description: Install Arc Lightning and create your first tracked work.
---

## Requirements

You need:

- A Rust toolchain
- Git

Arc Lightning uses the enclosing worktree root as its boundary.

You can run it from the root or from a nested directory.

## Build from Source

Clone the repository and install the `arcl` binary into Cargo's local bin
directory:

```sh
git clone https://github.com/stormlightlabs/arclightning.git
cd arclightning
cargo install --path .
```

For development, keep the checkout and run the CLI with Cargo instead:

```sh
cargo run -- --help
```

## Initialize a project

From the Git worktree you want to track:

```sh
cd ~/src/my-project
arcl init
```

Initialization creates `.arcl/config.toml`, `.arcl/arcl.db`, and a scoped
`.arcl/.gitignore`. Re-running `arcl init` preserves existing configuration,
database content, and local ignore customizations.

Use the optional snapshot flag when initializing a project that wants the
snapshot setting enabled:

```sh
arcl init --snapshot
```

See [Version-control project snapshots](/guides/snapshots/) for the export,
import, and Git workflow.

## Capture an idea

Ideas are useful while the shape of the work is still changing:

```sh
arcl idea create "Add keyboard shortcuts" \
  --description "Support the common navigation actions."

arcl idea list
```

Descriptions can also come from a Markdown file or standard input:

```sh
arcl idea create "Document recovery" --description-file notes.md

printf '# Recovery\n\nWrite down the restore steps.\n' \
  | arcl idea create "Document recovery" --description-file -
```

## Promote Ideas into Plans

Create a release, promote the captured idea using an existing Markdown spec,
and add an ordered milestone:

```sh
arcl release create "Spring release" \
  --description "Ship the next planning slice."

arcl idea promote arcl-i-01J... \
  --spec specs/keyboard-navigation.md \
  --release arcl-r-01J...

arcl milestone create "Foundation" \
  --epic arcl-e-01J... \
  --position 10
```

Copy the generated release and epic IDs from the command output.

Promotion keeps the source idea linked to the new epic. Repeating the command
returns that same epic instead of creating another one.

Spec paths must resolve to existing regular Markdown files inside the worktree
because Arc Lightning stores the path relative to the worktree root and never
edits the linked spec.

## Add and move work

Create a task, attach a subtask, and move the task through its lifecycle:

```sh
arcl task create "Add schema" \
  --milestone arcl-m-01J... \
  --priority high \
  --position 10 \
  --description "Create the tables needed by the tracker."

arcl task create "Add migration test" \
  --milestone arcl-m-01J... \
  --parent arcl-t-01J...

arcl task start arcl-t-01J...
arcl task complete arcl-t-01J... --evidence "Tests pass."
```

Tasks may be `pending`, `in_progress`, `parked`, `completed`, or `cancelled`.
Use `park` for work that should stay open but temporarily leave the active queue.

`unpark` returns it to `pending`.

Use `arcl task handoff` to store a Markdown resume note and park in-progress
work in one operation. The [Tasks guide](/guides/tasks/) covers handoffs and
completion evidence.

## Inspect Work Graph

```sh
arcl tree
arcl ready
arcl explain arcl-t-01J...
arcl context arcl-t-01J...
arcl check
```

`explain` reports every reason a task is ready or excluded.

`context` collects the task, its ancestors, linked spec, blockers, dependents,
handoff, and blocker evidence in one response.

See [Inspecting Work](/guides/inspect-work/) for list filters and output examples.

## Choosing Output Mode

Human-readable output is the default.

Scripts and agents can select a stable machine-facing format:

```sh
arcl --json idea create "Automate this" --description "Details"
arcl --plain idea list
arcl --quiet task complete arcl-t-01J...
```

`--json` emits a versioned envelope with `format_version: 1`, `--plain` emits
stable unstyled records, and `--quiet` prints only the affected ID for mutations.
