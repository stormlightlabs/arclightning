---
title: Dependencies and Ready Work
description: Model task blockers and find deterministic actionable work.
---

Tasks can wait on other tasks without storing a separate ready status. Only a
completed blocker satisfies a dependency. Cancelled and parked blockers keep
dependent work blocked.

```sh
arcl dependency add <task-id> --blocked-by <blocker-id>
arcl dependency remove <task-id> --blocked-by <blocker-id>
arcl ready
arcl next --plain
arcl explain <task-id>
```

`dependency add`, `task create --blocked-by`, and `plan apply` refuse a blocker
edge that closes a cycle or joins a task to its own ancestor or descendant.
`task update --parent` and `plan apply` also refuse a move after which a task
in the moved subtree is blocked by a new ancestor, or blocks one. The error
lists the task IDs along the cycle, or the parent chain from the descendant up
to the ancestor, and nothing is written. A bad edge already stored elsewhere in
the project does not block an unrelated write. `plan check` and `plan diff`
report the same errors as `plan apply`.

`ready` returns actionable leaf tasks in deterministic priority and position
order. `next` returns the first result, or an empty result when no work is ready.
Both commands also accept `--priority`, `--release`, `--epic`, `--milestone`,
and `--parent` filters, and support `--json` for automation.

Use `explain` when a task does not appear in `ready`. It reports every applicable
readiness or exclusion reason rather than stopping at the first one.
