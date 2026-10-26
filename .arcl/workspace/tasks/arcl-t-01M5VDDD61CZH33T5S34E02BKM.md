+++
id = "arcl-t-01M5VDDD61CZH33T5S34E02BKM"
phase = "arcl-ph-01M5VDD6PCB2B64MKW9TX01XXG"
title = "Show status on each arcl tree line"
status = "pending"
priority = "normal"
position = 1
+++

Show each record's status on its `arcl tree` line, and readiness on open
tasks. Every human line is `kind \`id\` [title]` today, so completed and
ready work look the same. `--plain` already carries the status, and `--json`
carries it on each node.

Scope:

- the status after the title on each human line, as `arcl list` shows it
- `ready` or `blocked` after the status on pending tasks
- no change to `--plain` columns or `--json`

Acceptance criteria:

- [ ] Each human `arcl tree` line shows the record's status
- [ ] Pending tasks also show whether they are ready or blocked
- [ ] `--plain` and `--json` output are unchanged

Verification:

- CLI tests for a completed, an in-progress, a ready and a blocked task
- `arcl tree` on this repository
- `just check`
