---
title: Compaction
description: Why arcl archives old completed tasks, what an archived task keeps, and why its evidence moves into one note per phase.
---

A project that runs for months collects hundreds of completed tasks. Each one
is a line in `arcl tree`, a row in `arcl list` and the web app's Work list,
and text in an agent's context. Most of it is finished work nobody reads.
Compaction archives completed tasks older than a date, so everyday reads show
the work that is still moving.

## What an archived task keeps

An archived task keeps its status, `completed`, and gains an `archived-at`
time. It stays in the database and in its workspace file. `arcl show` always
shows it, and `--all` brings it back into `list`, `tree` and `context`.

Everyday reads leave it out: `list`, `task list`, `tree`, `context`, the web
app, GraphQL and the MCP tools. `prime`, `ready` and `next` list only open
tasks, so they never showed it. Phase progress in `tree`, `roadmap` and the
web app still counts archived tasks as done, so a phase keeps its totals.

## Evidence moves to the phase

The evidence on each task is often the only record of how the work was done.
Compaction copies it into one note per phase, titled `Compacted work: <phase
title>` and linked to the phase. Reading one note per phase is shorter than
opening each task.

This is why a task must sit under a phase to be archived. A task outside every
phase has no note for its evidence, so it stays.

The note's ID comes from the phase's ID, so every clone that compacts a phase
writes the same `notes/<id>.md` file. A later run appends sections to it.

## Which tasks stay

Only completed tasks are candidates. A completed task that blocks open work
stays, since the open task still depends on it. A parent stays while any of
its subtasks is open or cancelled.

An archived task also takes no new work. No task can be created or moved
under it, and no dependency can be added to or from it.

## Why one branch

Archiving writes a time into each task file and appends to the phase note.
Two clones that compact the same tasks write different times. Two that
compact different tasks of one phase write different versions of its note.
Either way Git reports a conflict when the branches merge, so compact on one
branch and let the others pull the result.

[Compact old work](/how-to/compact-old-work/) runs it.
