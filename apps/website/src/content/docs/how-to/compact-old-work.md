---
title: Compact old work
description: Archive tasks completed before a date with arcl compact, keeping their evidence in one note per phase.
---

[Compaction](/explanation/compaction/) explains what archiving changes and
why. Run it on one branch: two clones that compact the same tasks set
different `archived-at` times, and Git reports a conflict when they merge.

<div id="preview-then-compact"></div>

## Preview the run

Pick a date. Tasks completed before the start of that day, in UTC, are
candidates:

```sh
arcl compact --before 2026-11-10 --dry-run
```

```text
Would archive 2 tasks in 1 phase. Nothing was written.
Phase `GRYN` Key handling: would create a note Compacted work: Key handling
  `GV2D` Parent
  `81TZ` Child
```

`--phase <ID>` limits the run to one phase.

A completed task stays when it blocks open work, sits outside every phase, or
has an open or cancelled subtask. [The manual](/reference/manual/#compaction)
lists every condition.

<div id="what-gets-archived"></div>
<div id="the-phase-note"></div>

## Archive the tasks

Run the same command without `--dry-run`:

```sh
arcl compact --before 2026-11-10
```

```text
Archived 2 tasks in 1 phase.
Phase `GRYN` Key handling: created note `F71A` Compacted work: Key handling
  `GV2D` Parent
  `81TZ` Child
```

Running it again changes nothing:

```text
No completed tasks to archive.
```

With the workspace on, commit the changed task files and the new note.

## Include tasks with no completion time

Tasks completed before arcl recorded completion times have none, and so does
a task whose file was set to `completed` by hand. `compact` skips them unless
you pass `--include-undated`, which treats them as completed before any date:

```sh
arcl compact --before 2026-11-10 --include-undated
```

<div id="after-compaction"></div>

## Read archived work

The phase's note holds a section per archived task, with its title, ID,
completion time and evidence:

```sh
arcl note show F71A
```

`arcl show <id>` always shows an archived task. `list`, `task list`, `tree`
and `context` show archived tasks with `--all`.
