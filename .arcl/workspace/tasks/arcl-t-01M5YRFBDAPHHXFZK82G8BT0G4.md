+++
id = "arcl-t-01M5YRFBDAPHHXFZK82G8BT0G4"
phase = "arcl-ph-01M5YRFB35TZ51QW4AG1A8MS5B"
title = "Gate tasks on pull requests, CI runs or dates"
status = "completed"
priority = "normal"
position = 4
evidence = "Delivered by #78: `arcl gate add <task> --pr <url> | --run <url> | --until <date>`, `gate list`, `gate remove` and `gate check` store gates in the `task_gates` table (migration 007) and in task files as a `[[gates]]` array that moves the workspace to format version 4. An open gate is an `open_gate` readiness reason carrying the gate, so `explain`, `ready`, `next` and `next --start` respect it. `gate check` meets date gates against today's UTC date offline and asks `gh` for pull requests and runs, reporting each gate and a missing `gh`. `show`, `context`, `prime` and the web Task view show gates, and the MCP server has `gate_add`, `gate_list` and `gate_check`. CLI tests cover past, today and future dates, a stubbed `gh` and a missing one."
+++

Let a task wait on something outside arcl: a pull request merging, a CI run
passing or a date arriving. Taken from beads' gates.

A gate is a blocker that is not a task. `arcl gate add <task> --pr <url>`,
`--run <url>` or `--until <date>` adds one, and `arcl gate check` resolves the
ones that are met. Readiness reports an open gate as a typed reason.

Scope:

- gate records and their readiness reason
- `gate add`, `gate list`, `gate remove` and `gate check`
- checking pull requests and runs through the GitHub CLI when present, and
  reporting when it is not

Acceptance criteria:

- [x] A task with an open gate is not ready, and `explain` names the gate
- [x] `gate check` closes met gates and leaves the rest
- [x] Date gates resolve without network access

Verification:

- CLI tests with date gates and a stubbed GitHub check
- `just check`
