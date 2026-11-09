---
title: Overview
description: What Arc Lightning is, where it keeps records, and the interfaces that read and change them.
---

Arc Lightning (`arcl`) is a local-first planning and execution tracker for
developers and coding agents. One project holds captures, specs, plans,
tasks, notes and releases. arcl works out which tasks are ready, and keeps the
handoffs and evidence the next session needs to pick up the work.

## Records live in the repository

`arcl init` creates `.arcl/` at the root of the enclosing Git worktree. Every
command finds that root, so `arcl` works from any directory below it.

```text
.arcl/
├── .gitignore    # keeps the database and temporary files out of Git
├── config.toml   # project settings
├── arcl.db       # the live SQLite database
└── workspace/    # optional: one Markdown file per record, committed with the code
```

The database holds the working state. With the workspace on, every command
syncs the database with `.arcl/workspace/`, so records travel through Git with
the code. Branches, pull requests, merges and review apply to plans and tasks
as they do to source files. [The workspace](/explanation/workspace/) explains
how the two copies stay in step.

## One set of operations, several interfaces

Every interface runs the same Rust operations, with the same validation and
transactions. A task started in the browser is refused for the same reasons
as one started from the CLI.

| Interface          | For                                                       |
| ------------------ | --------------------------------------------------------- |
| `arcl`             | People, and scripts through `--json` envelopes.           |
| `arcl-mcp`         | Coding agents, through typed MCP tools.                   |
| `arcl serve`       | People, in a local web app with Inbox, Work and Planning. |
| `arcl query`       | Scripts that read records with GraphQL, with no server.   |
| `arcl serve --api` | Local tools that call the REST endpoints and `/graphql`.  |
| claude.ai artifact | People who browse and triage from claude.ai.              |

## How work moves

Work starts as a capture: a loose idea filed in the inbox so it does not
interrupt the task at hand. Once its goal is clear, a capture becomes a spec,
which states what should be true when the work is done. A plan says how to
build the spec, in phases and tasks, often written as one TOML file.

From there, `arcl ready` lists the tasks nothing holds back. A person or an
agent starts one, records decisions as notes, and completes it with evidence.
Work that stops partway gets a handoff, a resume note for whoever picks it up
next. [The record model](/explanation/record-model/) describes each kind of
record, and [Readiness and the lifecycle](/explanation/readiness/) describes
how a task becomes ready.

The [quick start](/tutorials/quick-start/) runs this flow end to end.
