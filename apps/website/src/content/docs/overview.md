---
title: Overview
description: The Arc Lightning project model, where it stores records, and the interfaces that read and change them.
---

Arc Lightning (`arcl`) is a local-first planning and execution tracker for
developers and coding agents. It connects captures, specs, plans, tasks, notes
and releases in one project, works out which tasks are ready, and keeps the
handoffs and evidence that let the next session pick up where the last one
stopped.

## The project model

```text
Project
├── Capture
├── Spec
│   └── Plan
│       ├── Phase (optional)
│       └── Task
├── Task
├── Note
└── Release
```

| Record  | What it holds                                                             |
| ------- | ------------------------------------------------------------------------- |
| Capture | An inbox entry: a title and a Markdown body, promoted later or discarded. |
| Spec    | A goal Arc Lightning owns: a Markdown body and acceptance criteria.       |
| Plan    | How a spec will be built. A spec can have more than one plan.             |
| Phase   | An ordered group of tasks inside a plan. Phases are optional.             |
| Task    | A unit of work, with status, priority, subtasks, blockers and gates.      |
| Note    | Research, decisions or findings, linked to any other record.              |
| Release | A named set of specs, plans, tasks and notes that ship together.          |

A task can sit under a phase, a plan, a spec, another task, or nothing.
Blockers connect any two tasks in the project. A task is ready when it is
pending, every blocker on it and on its ancestors is completed, its spec,
plan and phase are open, and it has no open gate.

Every record has a typed ID such as `arcl-t-01M5VDDD42HZQ1FEN49X80GBA5`. The
prefix names the kind: `c` capture, `s` spec, `pl` plan, `ph` phase, `t`
task, `n` note, `r` release and `g` gate. Commands also take a short ID, the
last four or more characters, such as `GBA5`. The
[manual](/reference/manual/#record-ids) gives the rules.

## Where records live

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
the code: branches, pull requests, merges and review all apply to them. The
[workspace guide](/guides/workspace/) covers sync and conflicts.

## Interfaces

Every interface runs the same Rust operations, with the same validation and
transactions.

- The `arcl` CLI is the primary interface. Commands take `--json` for a
  versioned envelope. The [manual](/reference/manual/) lists every command.
- `arcl serve` opens the [web app](/guides/web-app/) in a browser, with
  Inbox, Work, Planning and Task views.
- `arcl-mcp` is an [MCP server](/guides/mcp/) that gives coding agents typed
  tools for the same operations.
- `arcl query` and `arcl serve --api` answer [GraphQL and REST
  requests](/guides/api/) from scripts and local tools.

## A typical flow

1. File loose ideas as [captures](/guides/captures/).
2. Promote a capture to a [spec](/guides/specs/) once the goal is clear.
3. Write a [plan](/guides/plans/) for the spec, with [phases](/guides/phases/)
   and [tasks](/guides/tasks/).
4. Run `arcl ready` or `arcl next --start` to pick up work, record
   [notes](/guides/notes/) as you go, and complete each task with evidence.
5. Hand off unfinished work so the next person or agent can resume it.

The [Quick Start](/quick-start/) runs this flow end to end.
