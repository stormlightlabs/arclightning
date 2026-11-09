---
title: Claims and handoffs
description: What a claim protects and where it stops, why a handoff parks the task, and how the next session reads the trail a task leaves.
---

Several people and agents can work one project at once. A claim stops two of
them starting the same task. A handoff passes unfinished work to whoever comes
next. Task notes and evidence record what happened, so the next session reads
the history instead of guessing.

## What a claim is

A claim is the task's assignee, set when someone starts it with `--claim`.
The name comes from the flag, then `ARCL_AGENT`, then the Git user name.

While a task has an assignee, any start that does not claim it for the same
name is refused with exit code `4` and the `conflict` error code. `--force`
skips readiness checks but not a claim, since overriding someone else's work
is a different decision from starting work early.

`arcl next --start --claim` finds the first ready task and starts it in one
write transaction. Two agents that run it at the same moment get different
tasks. The second waits up to 5 seconds for the first, then takes the next
ready task.

## Where a claim stops

A claim lives in one database, which is one checkout's `.arcl/arcl.db`.
Agents in separate Git worktrees each have their own database, so both can
claim the same task. Each sees the other's claim only after the task file
reaches its checkout through Git and a sync.

Give parallel agents in separate worktrees different tasks, or different
phases with `arcl next --start --claim --phase <id>`.

## Why stopping releases the claim

Parking, handing off, completing and cancelling all clear the assignee,
whoever runs them. A claim says someone is working on the task now. Once the
work stops, the claim would only keep others away from work nobody is doing.
This is why `arcl prime` tells an agent to pick up parked work first: any
session may take it.

## Why a handoff parks the task

A handoff stores one resume note, parks the task and releases the claim, all
in one transaction. The task leaves `in_progress`, so nobody mistakes it for
active work, and stays out of `ready` until someone unparks it on purpose.

Each task keeps one handoff, the latest, because the next session needs to
know where the work stands now. Unparking and starting keep it until another
handoff replaces it. The history of what was tried belongs in task notes.

## The trail a task leaves

| Record    | Written                      | Holds                                      |
| --------- | ---------------------------- | ------------------------------------------ |
| Task note | any time, appended           | a decision, observation, blocker or result |
| Handoff   | when work stops partway      | where the work stands and what is next     |
| Evidence  | when the task completes      | what shows the work is done                |
| Capture   | when work turns up elsewhere | the new work, linked to its source task    |

`arcl context <id>` gathers all of it, with the spec and plan above the task
and the evidence of the tasks that blocked it. With the workspace on, the
trail lives in the task file and travels through Git with the code.

arcl stores evidence as written. It does not run commands, open links or
judge whether the evidence proves the work.
