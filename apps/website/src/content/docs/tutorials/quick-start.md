---
title: Quick Start
description: Install arcl, start a project, and take one idea through a spec, a plan and a completed task.
---

In this tutorial you install `arcl`, start a project, and take one idea from a
capture to a completed task. It takes about ten minutes. You need a terminal
and Git.

## Install arcl

On macOS or Linux, run the installer:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/stormlightlabs/arclightning/releases/latest/download/arcl-cli-installer.sh | sh
```

On Windows, run this in PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/stormlightlabs/arclightning/releases/latest/download/arcl-cli-installer.ps1 | iex"
```

Check that it runs:

```sh
arcl --version
```

```text
arcl 0.3.0
```

[Install arcl](/how-to/install/) covers Homebrew, cargo-binstall, pinned
versions and building from source.

## Start a project

Make a Git repository to practise in, and initialize arcl with the workspace
on:

```sh
mkdir keyboard-demo && cd keyboard-demo
git init
arcl init --workspace
```

```text
Initialized Arc Lightning in `/home/you/keyboard-demo` with the workspace enabled
```

arcl made a `.arcl/` directory. It holds the database and a `workspace/`
directory with one Markdown file per record, which you commit with the code.

## Capture an idea

File the idea in the inbox:

```sh
arcl capture create "Add keyboard shortcuts" --body "Support the common navigation actions."
```

```text
Created capture `arcl-c-01M70M85PHVYXC5W3WPQAFQM3S`: Add keyboard shortcuts
```

Your ID differs from the one shown here. Every record gets its own.

## Turn it into a spec

The idea is clear enough to plan, so promote the capture to a spec and give it
an acceptance criterion. Use the last four characters of your capture's ID in
place of `QM3S`:

```sh
arcl capture promote QM3S spec --acceptance-criteria "- [ ] j and k move the selection"
```

```text
Promoted capture `arcl-c-01M70M85PHVYXC5W3WPQAFQM3S` to spec `arcl-s-01M70M8AY4K0WT6RN8C4H4JKHQ`
```

The spec took the capture's title and body. `QM3S` is a short ID, and every
command takes one in place of a full ID.

## Plan the work

Save this plan as `plan.toml`. It has one phase with two tasks, and the second
task waits for the first:

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

Create a plan under the spec from that file, using your spec's short ID, then
draw the tree:

```sh
arcl plan create "Keyboard plan" --spec JKHQ --input plan.toml
arcl tree
```

```text
Applied plan `arcl-pl-01M70M8FPNJTRS3HK1EZ7RDQ8G` (1 phase(s), 2 task(s)).
○ Add keyboard shortcuts
└── ○ Keyboard plan
    └── ○ Key handling  0/2
        ├── ● Add the key bindings table
        └── ○ Move the selection with j and k  blocked by MJS2
```

`●` marks the one task you can start now. The other is blocked by it.

## Work a task

Ask arcl what is ready:

```sh
arcl ready
```

```text
arcl-t-01M70M8FPN8SMXD6TMX2G3MJS2	[high]	pending	Add the key bindings table
```

Start it and claim it, so nobody else starts it too:

```sh
arcl task start MJS2 --claim
```

```text
Started task `arcl-t-01M70M8FPN8SMXD6TMX2G3MJS2`: Add the key bindings table
```

Record a decision as you work:

```sh
arcl task note MJS2 --kind decision "Keep bindings in one table"
```

```text
Added decision note to task `MJS2`
```

Finish the task with evidence of what you did:

```sh
arcl task complete MJS2 --evidence "Bindings table merged."
```

```text
Completed task `arcl-t-01M70M8FPN8SMXD6TMX2G3MJS2`: Add the key bindings table
Unblocked task `arcl-t-01M70M8FPNTS24NBFPXW9F35WW`: Move the selection with j and k
```

The second task is ready now. Run `arcl tree` to see it marked `●`, and the
phase at `1/2`.

## Commit the records

Each command already wrote its records under `.arcl/workspace/`. Commit them
like any other change:

```sh
git add .arcl
git commit -m "Plan keyboard shortcuts"
```

Anyone who clones the repository and runs `arcl init` gets the same spec,
plan and tasks.

## Where to go next

- [Plan and finish a feature with an agent](/tutorials/agent-feature/) runs a
  longer plan with a coding agent, including a handoff between sessions.
- [The record model](/explanation/record-model/) explains captures, specs,
  plans, phases and tasks.
- The [manual](/reference/manual/) lists every command.
