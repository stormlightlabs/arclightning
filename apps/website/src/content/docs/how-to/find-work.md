---
title: Find work and explain readiness
description: List and filter records, draw the planning tree, find ready tasks, ask why a task is not ready, gather its context, and check the graph.
---

These commands read records and change nothing.

<div id="show-and-list-records"></div>

## List and show records

```sh
arcl show 7TR6
arcl list --kind task --status pending --priority high
arcl list --spec FVMC
arcl list --phase KSX8 --status pending in_progress
arcl list --assignee codex
```

```text
task	35WW	[pending]	Move the selection with j and k
```

`list` takes `--kind`, `--status`, `--priority`, `--release`, `--spec`,
`--plan`, `--phase`, `--parent`, `--assignee` and `--all`. A spec, plan or
phase filter matches that record and the work below it. Notes have no status,
so `--status` leaves them out. `show` takes an ID of any kind.

<div id="read-the-hierarchy"></div>

## Draw the tree

```sh
arcl tree
arcl tree pl-Z234
arcl tree --style full
arcl tree --style outline | grep in_progress
```

```text
◐ Add keyboard shortcuts                               JKHQ
└── ◐ Keyboard plan                                    DQ8G
    ├── ◐ Key handling  3/4                            GRYN
    │   ├── ✓ Add the key bindings table               MJS2
    │   ├── ✓ Parent                                   GV2D
    │   │   └── ✓ Child                                81TZ
    │   └── ● Move the selection with j and k          35WW
    ├── ○ Documentation  0/1                           FTH4
    │   └── ○ Document the shortcuts  blocked by 35WW  AWEF
    └── ○ Polish                                       KF31
Notes: 2
```

Each line has a status mark and a title. `●` is a task you can start now. A
phase shows its completed and total tasks, and a blocked task names its
blockers. `--style full`, used above, adds each record's short ID on the
right. The default style folds runs of finished phases into one line.

![arcl tree folding finished phases, with a status mark on each line](/images/arcl-tree-status.png)

Use `--all` to unfold every phase and list captures, notes, releases and
archived tasks. Use `--depth <N>` to stop after `N` levels. Use `--ascii` if
your terminal draws the marks double-width, as many CJK locales do. The
[manual](/reference/manual/#inspection) lists every mark and style.

## Find ready work

```sh
arcl ready
arcl ready --phase KSX8
arcl next
arcl next --priority critical high
```

`ready` lists the tasks you can start now, highest priority first, then by
position and ID. `next` prints the first of them. Both take `--priority`,
`--spec`, `--plan`, `--phase` and `--parent`. `next --start` starts the task
it finds.

<div id="explain-readiness"></div>

## Ask why a task is not ready

```sh
arcl explain 35WW
```

```text
Task `35WW` [pending] Move the selection with j and k
Ready: false
Blockers: MJS2
Ancestors:
Spec: JKHQ
Plan: DQ8G
Phase: GRYN
Reason: gate `VS7Q` waits until `2026-11-20`
Gate: `VS7Q` [open] date 2026-11-20
```

`explain` lists every reason. Here the blocker `MJS2` is
complete, and a date gate holds the task. [Dependencies and readiness
rules](/reference/deps/#readiness-reasons) lists each reason and how to clear
it.

<div id="gather-task-context"></div>

## Gather a task's context

Read the context before starting or resuming a task:

```sh
arcl context 35WW
arcl --json context 35WW
```

The context holds the task, its ancestor tasks, its spec, plan and phase,
direct and inherited blockers, dependents, readiness reasons, the handoff, the
task's evidence and the evidence of completed blockers. It lists the 10 most
recent task notes and every gate. `--json` adds the spec's acceptance
criteria. `--all` includes archived tasks among the blockers, dependents and
earlier evidence.

<div id="check-graph-integrity"></div>

## Check the graph

```sh
arcl check
```

```text
Check passed.
```

`check` syncs the workspace, then validates the database and the task graph.
It exits `0` when it finds no errors, and reports duplicate sibling positions
as warnings. Run it in CI to catch broken workspace files and blocker cycles
merged in by hand. The [manual](/reference/manual/#inspection) lists what it
checks and how it fails.
