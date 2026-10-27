+++
id = "arcl-t-01M5YRFB8NYG5KXXNBF3TSH9ZQ"
phase = "arcl-ph-01M5YRFB35TZ51QW4AG1A8MS5B"
title = "Print an agent primer with arcl prime"
status = "pending"
priority = "normal"
position = 2
+++

Print the agent loop and the current state of work in one command, for
SessionStart and PreCompact hooks. Taken from beads' `bd prime` and beans'
`beans prime`.

`arcl prime` prints the skill's working loop, then in-progress and parked tasks
with their handoffs, then the first ready tasks. It stays short enough to
inject into an agent's context.

Scope:

- `arcl prime` with human and `--json` output
- a size budget, with a pointer to `arcl context <id>` for detail
- docs showing it in Claude Code and Codex hooks

Acceptance criteria:

- [ ] `arcl prime` prints the loop, open work with handoffs and ready tasks
- [ ] Output stays within a documented size on this repository
- [ ] The guide shows a hook that runs it

Verification:

- a snapshot test of the output on a fixture project
- `just check`
