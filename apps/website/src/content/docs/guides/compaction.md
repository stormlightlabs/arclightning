---
title: Compaction
description: Archive old completed tasks with arcl compact, keeping their evidence in one note per phase, so reads stay small in long-lived projects.
---

A project that runs for months collects hundreds of completed tasks. They
slow down `arcl tree`, `arcl list` and the web app's Work list, and fill an
agent's context with finished work. `arcl compact` archives completed tasks
older than a date and copies their evidence into one note per phase.

## Preview, then compact

```sh
arcl compact --before 2026-11-10 --dry-run
```

```text
Would archive 1 task in 1 phase. Nothing was written.
Phase `KSX8` Key handling: would create a note Compacted work: Key handling
  `8GQ6` Add focus rings
```

Run it again without `--dry-run` to archive:

```sh
arcl compact --before 2026-11-10
```

```text
Archived 1 task in 1 phase.
Phase `KSX8` Key handling: created note `C62Q` Compacted work: Key handling
  `8GQ6` Add focus rings
```

`--before` takes a UTC date, `YYYY-MM-DD`, and selects tasks completed before
the start of that day. `--phase <id>` limits the run to one phase.

## What gets archived

A task is archived when all of these hold:

- It is completed, before the `--before` date, and not archived yet.
- It sits under a phase, directly or through its parent.
- It does not block a pending, in-progress or parked task.
- Each of its subtasks is archived in the same run or already.

Open and cancelled tasks are never archived, and neither is a task outside
every phase, since its evidence would have no phase note. A task completed
before arcl recorded completion times has none, and stays unless you pass
`--include-undated`.

## The phase note

Each phase gets one note, titled `Compacted work: <phase title>` and linked
to the phase. Each archived task has a section with its title, ID,
completion time and evidence:

```text
+++
id = "arcl-n-01M6Z4WFW21XB90H7HTHYHC62Q"
title = "Compacted work: Key handling"

[[links]]
kind = "phase"
id = "arcl-ph-01M6Z4WFW2Y2MPZERE5E1EKSX8"
+++

Evidence from the completed tasks `arcl compact` archived in this phase.

## Add focus rings

`arcl-t-01M6Z543M9MK02CDC2NKYD8GQ6` · completed 2026-11-09T03:24:24.233Z

Focus rings on every control.
```

The note's ID comes from the phase's ID, so every clone writes the same file.
A later run appends sections to it. Running the same command twice changes
nothing the second time.

## After compaction

An archived task keeps status `completed`, gains an `archived-at` time, and
stays in the workspace files. `arcl show` always shows it. `list`,
`task list`, `tree` and `context` leave it out unless you pass `--all`, and so
do the web app, GraphQL and the MCP tools. Phase progress in `tree`,
`roadmap` and the web app still counts it as done, so totals do not change.

An archived task takes no new work: no task can be created or moved under
it, and no dependency can be added to or from it.

Compact on one branch. Two clones that compact the same tasks write different
`archived-at` times, and Git reports a conflict when they merge. The
[manual](/reference/manual/#compaction) has the output formats and edge
cases.
