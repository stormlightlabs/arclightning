+++
id = "arcl-t-01M5YRFBF5V803RBTEDG12QZJP"
phase = "arcl-ph-01M5YRFB35TZ51QW4AG1A8MS5B"
title = "Link captures to the task they came from"
status = "completed"
priority = "normal"
position = 6
blocked-by = ["arcl-t-01M5YRFB4XC9E438GSNCV444XZ"]
evidence = "Merged in #81 (273d44a)."
+++

Record where a capture came from when an agent files it while working on a
task, so the Inbox shows its source. Taken from beads' `discovered-from`
links.

Scope:

- `arcl capture create --from <task-id>`, defaulting to the one in-progress
  task when `ARCL_AGENT` holds exactly one claim
- the link in the capture's `show`, the Inbox and `--json`
- the link kept when the capture is promoted

Acceptance criteria:

- [ ] A capture made with `--from` shows its source task
- [ ] Promotion keeps the link on the new record
- [ ] The link round-trips through the workspace files

Verification:

- CLI tests for create, show and promote
- `just check`
