---
title: Dependencies and Ready Work
description: Model task blockers and find deterministic actionable work.
---

Tasks can wait on other tasks without storing a separate ready status. Only a
completed blocker satisfies a dependency; cancelled and parked blockers keep
dependent work blocked.

```sh
arcl dependency add <task-id> --blocked-by <blocker-id>
arcl dependency remove <task-id> --blocked-by <blocker-id>
arcl ready
arcl next --plain
```

`ready` returns actionable leaf tasks in deterministic priority and position
order. `next` returns the first result, or an empty result when no work is ready.
Both commands also accept `--priority`, `--release`, `--epic`, `--milestone`,
and `--parent` filters, and support `--json` for automation.
