---
title: Readiness and the lifecycle
description: How arcl decides a task is ready, the statuses each kind of record moves through, and how closing a container treats the work below it.
---

arcl stores no "ready" flag. Every time you ask, it works out readiness from
the task's status, its place in the tree, its blockers and its gates. When
the last thing holding a task back clears, such as its last blocker
completing, the task is ready with no command to run.

## When a task is ready

A ready task is pending, has nothing above it that is closed or parked, and
waits on nothing: every blocker is complete and no gate is open. The
[readiness rules](/reference/deps/#readiness-rules) list each condition.

Only leaf tasks are ready. A task with subtasks is a heading for the work
below it, so its subtasks are the ones `arcl ready` lists. `arcl task start`
still accepts a parent, since having children is the one reason it ignores.

A blocker on a parent holds back every task under it. Moving a subtree under
a blocked parent therefore holds all of it back, with no edge added to each
task.

Only a completed blocker releases a task. A cancelled blocker will never
complete, so the task stays blocked until you remove or replace that
dependency. A parked blocker holds the task back too, since the work is
unfinished.

`arcl explain <id>` lists every reason a task is not ready. [Dependencies and readiness rules](/reference/deps/) lists each
reason by name.

<div id="statuses-by-kind"></div>
<div id="move-tasks"></div>

## Statuses

Every status change is an explicit command. A task starts `pending`, moves to
`in_progress` when someone starts it, and ends `completed` or `cancelled`.
[Lifecycle](/reference/manual/#lifecycle) lists the statuses of each kind of
record and the command behind each task transition.

Parked is open work set aside. It stays out of `ready` until someone unparks
it, and a parked task must be unparked before it completes. Starting a task
that is already in progress changes nothing.

## Final statuses

Completing and cancelling are final for every kind of record. No command
reopens a closed task. A task file can be reopened by hand, as [Workspace file
format](/reference/workspace-format/#task-fields) describes.

<div id="close-containers"></div>

## Closing containers

A spec, plan, phase, release or parent task refuses to close while any record
below it is open. This keeps a plan from being marked done while work in it
is unfinished.

`--allow-open-children` closes only the record you name. It never cascades:
the open work below stays open, so nothing is closed without someone looking
at it. That work then stops being ready, since a task under a closed spec,
plan or phase cannot start, and a closed container takes no new tasks.
`arcl explain` gives the reason `closed_container`.

Releases differ. Membership never holds work back, so closing a release
changes no task's readiness.

To close a task and its open subtasks together, `arcl task complete
--with-subtasks` completes them in one write.
