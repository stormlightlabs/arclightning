+++
id = "arcl-t-01M5YRFBDAPHHXFZK82G8BT0G4"
phase = "arcl-ph-01M5YRFB35TZ51QW4AG1A8MS5B"
title = "Gate tasks on pull requests, CI runs or dates"
status = "pending"
priority = "normal"
position = 4
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

- [ ] A task with an open gate is not ready, and `explain` names the gate
- [ ] `gate check` closes met gates and leaves the rest
- [ ] Date gates resolve without network access

Verification:

- CLI tests with date gates and a stubbed GitHub check
- `just check`
