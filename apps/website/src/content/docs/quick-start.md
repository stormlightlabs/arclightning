---
title: Quick Start
description: Install Arc Lightning and create your first tracked work.
---

## Install

Install the `arcl` binary on macOS or Linux:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/stormlightlabs/arclightning/releases/latest/download/arcl-cli-installer.sh | sh
```

On Windows, in PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/stormlightlabs/arclightning/releases/latest/download/arcl-cli-installer.ps1 | iex"
```

With Homebrew:

```sh
brew install stormlightlabs/tap/arcl
```

With [cargo-binstall](https://github.com/cargo-bins/cargo-binstall), which
downloads the release binary instead of compiling it:

```sh
cargo binstall --git https://github.com/stormlightlabs/arclightning arcl-cli
```

`--git` takes the version from `main`. When `main` carries a version that is
not released yet, cargo-binstall finds no archive and compiles from source,
without the web app. To pin a release, and fail instead of compiling, install
from a clone of its tag:

```sh
git clone --depth 1 --branch v<version> https://github.com/stormlightlabs/arclightning
cargo binstall --manifest-path arclightning --disable-strategies compile arcl-cli
```

Release binaries cover Linux (x86_64 and arm64, static), macOS (arm64 and
x86_64) and Windows (x86_64), and include the web app that `arcl serve` opens.
The `latest` links skip pre-releases. To pin a version, as a cloud session or
CI job would, replace `latest/download` with `download/<tag>`, such as
`download/v0.3.0`.

Releases before 1.0 can change JSON shapes, and each envelope's
`format_version` names the shape it carries.

Arc Lightning uses the enclosing Git worktree root as its boundary. You can run
it from the root or from a nested directory.

## Build from Source

Building needs a Rust toolchain, plus Node and pnpm for the web app. Clone the
repository and install the `arcl` binary into Cargo's local bin directory:

```sh
git clone https://github.com/stormlightlabs/arclightning.git
cd arclightning
pnpm install
pnpm --filter @arclightning/web build
cargo install --path crates/arcl-cli
```

The `pnpm` steps build the web app that `arcl serve` embeds. Without them,
`arcl` still installs, and `arcl serve` serves a page that says the web app was
not built.

`cargo install --git` compiles a tag or branch without a checkout, and leaves
out the web app for the same reason:

```sh
cargo install --git https://github.com/stormlightlabs/arclightning --tag v0.2.0 arcl-cli
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

Use the optional workspace flag when initializing a project that wants the
workspace setting enabled:

```sh
arcl init --workspace
```

See [Share the project workspace](/guides/workspace/) for the export,
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

`--json` emits an envelope with a `format_version`, `--plain` emits
stable unstyled records, and `--quiet` prints only the affected ID for mutations.
