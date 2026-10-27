+++
id = "arcl-t-01M5YRFB6X9AQQ43BBZVYJWWC8"
phase = "arcl-ph-01M5YRFB35TZ51QW4AG1A8MS5B"
title = "Start the next ready task in one step"
status = "completed"
priority = "normal"
position = 1
blocked-by = ["arcl-t-01M5YRFB4XC9E438GSNCV444XZ"]
evidence = "Delivered by #PR: `arcl next --start [--claim [<name>]]` selects and starts the first ready task in one IMMEDIATE write transaction, honours the `next` filters, and prints nothing to start (exit 0, null `data.task` in JSON) when nothing is ready. A CLI test runs two concurrent calls and checks they start different tasks, and that with one ready task the other finds nothing."
+++

Let an agent pick and start the first ready task in one transaction with
`arcl next --start`, so a race between `next` and `task start` cannot hand two
agents the same task. Taken from beads' `ready --claim`.

Scope:

- `arcl next --start [--claim <name>]`, applying the same priority and filters
  as `arcl next`
- a clear result when nothing is ready

Acceptance criteria:

- [x] `arcl next --start` starts and returns the task `arcl next` would show
- [x] With `--claim`, it claims the task as `task start --claim` does
- [x] It prints nothing to start, and exits 0, when no task is ready

Verification:

- CLI tests, including two concurrent calls that start different tasks
- `just check`
