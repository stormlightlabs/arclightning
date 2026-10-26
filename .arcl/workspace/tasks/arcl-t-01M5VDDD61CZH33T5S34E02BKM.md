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

Compute a status mark for each tree node in one place, from the record's
status and, for pending tasks, its readiness: `✓` done, `◐` in progress,
`●` ready, `○` pending or blocked. The current human line starts with it.
The folded view and the full and outline styles reuse the same marks.

Scope:

- a status mark per tree node, from status and readiness, in one function
  every human tree style calls
- the mark at the start of each human `arcl tree` line
- no change to `--plain` columns or `--json`

Acceptance criteria:

- [ ] Each human `arcl tree` line starts with the record's status mark
- [ ] Pending tasks show `●` when ready and `○` when blocked
- [ ] `--plain` and `--json` output are unchanged

Verification:

- unit tests of the mark for each status, and for a ready and a blocked
  pending task
- CLI tests for a completed, an in-progress, a ready and a blocked task
- `arcl tree` on this repository
- `just check`
