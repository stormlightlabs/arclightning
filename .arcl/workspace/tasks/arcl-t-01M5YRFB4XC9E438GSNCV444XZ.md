+++
id = "arcl-t-01M5YRFB4XC9E438GSNCV444XZ"
phase = "arcl-ph-01M5YRFB35TZ51QW4AG1A8MS5B"
title = "Claim a task when starting it"
status = "completed"
priority = "normal"
position = 0
evidence = "Delivered by #74: `arcl task start --claim [<name>]` sets the assignee and status in one write, a claim held by another assignee fails with `conflict` (exit 4), and the assignee round-trips through workspace files, `list --assignee`, `show` and the web Task view."
+++

Record who started a task, so two agents working the same repository cannot
take the same one. Taken from beads' `--claim`.

Add an `assignee` field to tasks. `arcl task start <id> --claim <name>` sets
the assignee and the status in one transaction, and refuses when another
assignee already holds the task. `--claim` without a name takes it from
`ARCL_AGENT`, then the Git user name.

Scope:

- the `assignee` field in the store, the workspace files and `--json`
- `task start --claim`, and refusing a claim held by someone else
- `arcl list --assignee` and the assignee in `show` and the web app
- releasing the claim when the task is parked, completed or cancelled

Acceptance criteria:

- [x] `task start --claim` sets the assignee and status atomically
- [x] A second claim by a different assignee is refused with a typed error
- [x] The assignee round-trips through the workspace files

Verification:

- CLI tests for claiming, a refused claim and release
- `just check`
