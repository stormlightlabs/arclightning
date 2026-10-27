+++
id = "arcl-t-01M5VDD6X6EP6WH4CMDJSGQ87N"
phase = "arcl-ph-01M5NE5P5RE8Y5Y5Y84T36FMEE"
title = "Leave closed records out of Not ready"
status = "completed"
priority = "normal"
position = 12
evidence = "Delivered by #PR: the web app shows no readiness for a completed or cancelled task, and drops `status` and `closed_container` reasons from it."
+++

Stop showing "Not ready" on closed tasks. A completed task's readiness
callout reads "Not ready · 2 reasons: task status is completed (status);
phase Milestone 1: … is completed (closed_container)". Neither reason tells
the reader anything: the task is done, and so is its milestone.

The fix belongs in the web app's presentation. `arcl explain` and the JSON
readiness answer "can this task be started?", and for a closed task "no,
because it is closed" is correct and is what scripts and agents read. The
web app should instead not ask that question of a closed task.

Scope:

- hide the readiness callout in the Task view when the task is completed
  or cancelled
- for those tasks, leave `status` and `closed_container` reasons out of any
  readiness the web app still shows, such as the Readiness property and
  the Work board grouping
- leave `arcl explain`, `arcl context` and the JSON readiness unchanged

Acceptance criteria:

- [x] A completed or cancelled task's Task view has no readiness callout
- [x] No web view lists a closed ancestor as a reason on a closed task
- [x] Open tasks keep every reason, including `closed_container`
- [x] `arcl explain --json` output for a closed task is unchanged

Verification:

- unit tests in `apps/web/src/lib/task/view.test.ts` for completed,
  cancelled and open tasks under a completed phase
- a DOM test that the callout is absent on a completed task
- `pnpm check`, `pnpm test`, `just check`
