+++
id = "arcl-t-01M5YRFB6X9AQQ43BBZVYJWWC8"
phase = "arcl-ph-01M5YRFB35TZ51QW4AG1A8MS5B"
title = "Start the next ready task in one step"
status = "pending"
priority = "normal"
position = 1
blocked-by = ["arcl-t-01M5YRFB4XC9E438GSNCV444XZ"]
+++

Let an agent pick and start the first ready task in one transaction with
`arcl next --start`, so a race between `next` and `task start` cannot hand two
agents the same task. Taken from beads' `ready --claim`.

Scope:

- `arcl next --start [--claim <name>]`, applying the same priority and filters
  as `arcl next`
- a clear result when nothing is ready

Acceptance criteria:

- [ ] `arcl next --start` starts and returns the task `arcl next` would show
- [ ] With `--claim`, it claims the task as `task start --claim` does
- [ ] It prints nothing to start, and exits 0, when no task is ready

Verification:

- CLI tests, including two concurrent calls that start different tasks
- `just check`
