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
status and, for pending tasks, its readiness. Every status the domain has gets
a mark, with an ASCII form for `--ascii`:

| Mark | ASCII | Status |
| ---- | ----- | ------ |
| `✓` | `+` | task, spec, plan, phase or release completed; capture promoted |
| `✗` | `x` | task, spec, plan, phase or release cancelled; capture discarded |
| `◐` | `~` | task in progress; open spec, plan or phase with a started or finished task |
| `‖` | `=` | task parked |
| `●` | `*` | pending task that is ready |
| `○` | `o` | pending task that is blocked; other open containers; captured capture; note |

Notes have no stored status, and `--plain` shows them as `available`. The
current human line starts with the mark. The folded view and the full and
outline styles reuse the same marks.

Scope:

- a status mark per tree node, from status and readiness, in one function
  every human tree style calls, covering every task, container and capture
  status
- the mark at the start of each human `arcl tree` line
- no change to `--plain` columns or `--json`

Acceptance criteria:

- [ ] Each human `arcl tree` line starts with the record's status mark
- [ ] Every task, container and capture status, and notes, map to a mark
      in the table, and each mark has its ASCII form
- [ ] Pending tasks show `●` when ready and `○` when blocked
- [ ] `--plain` and `--json` output are unchanged

Verification:

- unit tests of the mark and its ASCII form for each status of each kind,
  for a note, and for a ready and a blocked pending task
- CLI tests for a completed, an in-progress, a parked, a cancelled, a ready
  and a blocked task
- `arcl tree` on this repository
- `just check`
