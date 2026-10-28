---
title: Dependencies and Ready Work
description: Model task blockers and find deterministic actionable work.
---

Tasks can wait on other tasks without storing a separate ready status. Only a
completed blocker satisfies a dependency. Cancelled and parked blockers keep
dependent work blocked. A blocker on a parent task also blocks every task under
it.

A cancelled blocker will never complete, so `explain` asks you to remove or
replace that dependency rather than wait on it.

```sh
arcl dependency add <task-id> --blocked-by <blocker-id>
arcl dependency add <task-id> --blocked-by <a>,<b>
arcl task create "<title>" --blocked-by <a> --blocked-by <b>
arcl dependency remove <task-id> --blocked-by <blocker-id>
arcl ready
arcl next --plain
arcl explain <task-id>
```

`--blocked-by` takes one task ID. In `dependency add` and `task create`, repeat
it or separate IDs with commas to add several blockers in one write; if one is
refused, none are added. `dependency remove` takes one.

`dependency add`, `task create --blocked-by`, and `plan apply` refuse a blocker
edge that closes a cycle or joins a task to its own ancestor or descendant.
`task update --parent` and `plan apply` also refuse a move after which a task
in the moved subtree is blocked by a new ancestor, or blocks one. The error
lists the task IDs along the cycle, or the parent chain from the descendant up
to the ancestor, and nothing is written. Plan errors name tasks the plan would
create by their plan path. A bad edge already stored elsewhere in the project
does not block an unrelated write. `check` reports it, and `explain` gives it as
a readiness reason for the task that holds it. `plan check` and `plan diff`
report the same errors as `plan apply`. Workspace sync and import load bad edges
the database already holds, with a warning.

`ready` returns actionable leaf tasks in deterministic priority and position
order. `next` returns the first result, or an empty result when no work is ready.
Both commands also accept `--priority`, `--release`, `--epic`, `--milestone`,
and `--parent` filters, and support `--json` for automation.

Use `explain` when a task does not appear in `ready`. It reports every applicable
readiness or exclusion reason rather than stopping at the first one.
