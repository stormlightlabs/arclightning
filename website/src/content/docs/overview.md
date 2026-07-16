---
title: Overview
description: How Arc Lightning fits planning work into a Git worktree.
---

Arc Lightning (`arcl`) is a local-first task tracker for developers and coding
agents. It keeps the live tracker in `.arcl/arcl.db` and discovers the enclosing
non-bare Git worktree, so commands work from the repository root or from any
directory below it.

## The work model

Arc Lightning separates loose ideas from committed execution work:

| Record    | Role                                          | Parent           |
| --------- | --------------------------------------------- | ---------------- |
| Idea      | A captured thought in the project inbox.      | None             |
| Release   | A container for a group of epics.             | None             |
| Epic      | A goal backed by an existing Markdown spec.   | Optional release |
| Milestone | An ordered stage inside an epic.              | Epic             |
| Task      | Work inside a milestone.                      | Milestone        |
| Subtask   | A task with its own lifecycle and identifier. | Task             |

The hierarchy keeps planning metadata close to the repository while leaving
specifications and implementation files in the worktree where normal Git tools
can review them.

## What Arc Lightning stores

Initialization creates `.arcl/` in the worktree root:

```text
.arcl/
├── .gitignore   # local database, temporary files, and conflict artifacts
├── config.toml  # project configuration
└── arcl.db      # live SQLite tracker
```

The database is operational state and is ignored by the scoped `.gitignore`.
Use `arcl init --snapshot` when you want to enable the optional snapshot setting
in `config.toml`.

## When to use it

Use the inbox for ideas that are not ready for a plan. Use a release and epic
when a goal has a spec. Use milestones to give the epic an order, then tasks to
make the next unit of work explicit. The CLI keeps lifecycle changes small and
machine-readable, which makes it useful in shell scripts and agent workflows.

Start with the [Quick Start](/quick-start/) or browse the task-oriented
[guides](/guides/ideas/).
