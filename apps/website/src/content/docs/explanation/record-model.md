---
title: The record model
description: The kinds of record Arc Lightning keeps, what each one is for, and how they connect.
---

An Arc Lightning project holds seven kinds of record. Specs, plans and phases
describe the work. Tasks are the work. Captures, notes and releases sit beside
them.

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

## Captures

A capture is an inbox entry: a title and a Markdown body for work nobody has
planned yet. Filing one takes a second, so a thought does not pull you off the
task at hand. Later, someone sorts the inbox. Each capture is promoted to a
spec, a task or a note, or discarded.

A capture filed while working on a task can name that task as its source. The
record it becomes keeps the source, so the trail runs from the finished work
back to the task that turned it up.

## Specs, plans and phases

A spec states a goal: what should be true when the work is done. It has a
Markdown body and acceptance criteria, stored in the record rather than in a
document elsewhere. Agents read the criteria with each task under the spec.

A plan says how to build one spec. A spec can have several plans, such as a
first attempt and its replacement. A plan holds tasks directly or in phases,
which are ordered stages such as "Storage" before "Web app". Phases are
optional.

Most plans start as a TOML plan file that lists phases, tasks and blockers.
Applying the file again after an edit updates the plan in place. The file
never deletes or cancels work, so removing a line from it leaves the task as
it was.

## Tasks

A task is a unit of work, with a status, a priority, a position among its
siblings, and optional subtasks, blockers and gates. A subtask is an ordinary
task with a parent. A task can sit under a phase, a plan, a spec, another
task, or nothing.

Blockers connect any two tasks in the project, across specs and plans. A gate
holds a task back until something outside arcl happens: a pull request
merges, a CI run passes, or a date arrives.

A task also carries the record of its work. Task notes are short, timestamped
lines of decisions and findings. A handoff is one resume note for unfinished
work. Evidence is the Markdown recorded when the task completes.

## Notes

A note record holds research, decisions or findings that outlive one task. It
has a title, a Markdown body and links to any other records. Notes have no
status and never affect readiness. A task note is different: it belongs to
one task and has no title.

## Releases

A release is a named set of specs, plans, tasks and notes that ship together,
such as `v0.1.0`. Membership is explicit. Adding a spec does not add its
plans or tasks, though a release counts the tasks under its specs and plans
in its progress. A record can belong to several releases. Release membership
never holds work back.

## IDs

Every record has a typed ID such as `arcl-t-01M5VDDD42HZQ1FEN49X80GBA5`. The
letter after `arcl-` names the kind: `c` capture, `s` spec, `pl` plan, `ph`
phase, `t` task, `n` note, `r` release and `g` gate. The rest is a ULID, so
IDs sort by creation time.

Commands also take a short ID, the last four or more characters, such as
`GBA5`. Records made close together share the start of their IDs, so a short
ID is always the end. A short ID that is unique today can match a second
record later, so files, links and scripts use full IDs. The
[manual](/reference/manual/#record-ids) gives the matching rules.

## Records are never deleted

No command deletes a record. A record closes through its status instead:
completed, cancelled or discarded. Closed records stay in the database and the
workspace, so the history of a plan stays readable. Compaction archives old
completed tasks, which hides them from everyday reads without removing them;
[Compaction](/explanation/compaction/) explains why.
