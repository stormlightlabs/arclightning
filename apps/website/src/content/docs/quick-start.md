---
title: Quick Start
description: Install arcl, initialize a project, and take one capture through a spec, a plan and a completed task.
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

## Build from source

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
cargo install --git https://github.com/stormlightlabs/arclightning --tag v0.3.0 arcl-cli
```

For development, keep the checkout and run the CLI with Cargo instead:

```sh
cargo run -- --help
```

## Initialize a project

From the Git worktree you want to track:

```sh
cd ~/src/my-project
arcl init --workspace
```

`init` creates `.arcl/config.toml`, the database `.arcl/arcl.db`, and a
`.arcl/.gitignore` that keeps the database out of Git. `--workspace` also
turns on the [workspace](/guides/workspace/): one Markdown file per record
under `.arcl/workspace/`, which you commit with the code. Leave it out to
keep records in the local database only. Running `init` again keeps the
existing configuration and records.

## Capture an idea

A capture is an inbox entry for work that is not planned yet:

```sh
arcl capture create "Add keyboard shortcuts" --body "Support the common navigation actions."
```

```text
Created capture `arcl-c-01M6Z52GC3FZ44XVDYGEXW8XVX`: Add keyboard shortcuts
```

`--body-file notes.md` reads the body from a file, and `--body-file -` from
standard input.

## Promote it to a spec

When the goal is clear, promote the capture. The spec takes the capture's
title and body, and you can add acceptance criteria:

```sh
arcl capture promote 8XVX spec --acceptance-criteria "- [ ] j and k move the selection"
```

```text
Promoted capture `arcl-c-01M6Z52GC3FZ44XVDYGEXW8XVX` to spec `arcl-s-01M6Z52GCZF3KWGK2JPHSPJ7TV`
```

`8XVX` is a short ID, the end of the capture's ID. Any command takes one in
place of the full ID.

## Plan the work

A plan says how the spec will be built. Write its phases and tasks in a TOML
file, `plan.toml`:

```toml
format-version = 1

[[phases]]
key = "keys"
title = "Key handling"
position = 0

[[phases.tasks]]
key = "bindings"
title = "Add the key bindings table"
priority = "high"
position = 0

[[phases.tasks]]
key = "selection"
title = "Move the selection with j and k"
position = 1
blocked-by = ["bindings"]
```

Create the plan and apply the file in one step:

```sh
arcl plan create "Keyboard plan" --spec J7TV --input plan.toml
arcl tree
```

```text
Applied plan `arcl-pl-01M6Z52GDTQFRHBSTMT6FK17MR` (1 phase(s), 2 task(s)).
○ Add keyboard shortcuts
└── ○ Keyboard plan
    └── ○ Key handling  0/2
        ├── ● Add the key bindings table
        └── ○ Move the selection with j and k  blocked by KAJE
```

`●` marks a ready task. The [plans guide](/guides/plans/) covers the file
format and how to change a plan later.

## Work a task

`arcl ready` lists the tasks you can start, highest priority first:

```sh
arcl ready
```

```text
arcl-t-01M6Z52GDTA3RG3BF0KDD2KAJE	[high]	pending	Add the key bindings table
```

Start it, claiming it so no one else starts it too, and record a decision as
you go:

```sh
arcl task start KAJE --claim
arcl task note KAJE --kind decision "Keep bindings in one table"
```

Finish with evidence. The output names the tasks the completion made ready:

```sh
arcl task complete KAJE --evidence "Bindings table merged."
```

```text
Completed task `arcl-t-01M6Z52GDTA3RG3BF0KDD2KAJE`: Add the key bindings table
Unblocked task `arcl-t-01M6Z52GDT6HNSB2K69ZD3XG0Y`: Move the selection with j and k
```

To stop partway, `arcl task handoff <id> --note "<where it stands>"` saves a
resume note and parks the task. `arcl context <id>` shows the task with its
spec, plan, blockers, handoff and evidence. The [tasks guide](/guides/tasks/)
covers both.

## Commit the records

With the workspace on, each command has already written its records under
`.arcl/workspace/`. Commit them like any other change:

```sh
git add .arcl
git commit -m "Plan keyboard shortcuts"
```

A teammate who pulls runs `arcl init` once in the clone, and the next `arcl`
command imports the records.

## Choose an output mode

Human output is the default. Scripts and agents choose a stable format:

```sh
arcl --json ready
arcl --plain tree
arcl --quiet capture create "Try a command palette"
```

`--json` prints an envelope with a `format_version` and `data`, `--plain`
prints one unstyled record per line, and `--quiet` prints nothing on success.
Errors still go to standard error. The [automation guide](/guides/automation/) describes the
envelope.

## Next steps

- Set up a coding agent with `arcl setup`, as the [agent
  workflows](/guides/agents/) guide shows.
- Open the [web app](/guides/web-app/) with `arcl serve`.
- Read the [manual](/reference/manual/) for every command.
