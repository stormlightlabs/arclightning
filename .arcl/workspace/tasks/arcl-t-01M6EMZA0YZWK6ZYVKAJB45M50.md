+++
id = "arcl-t-01M6EMZA0YZWK6ZYVKAJB45M50"
phase = "arcl-ph-01M6EMZ9DHDAY6RXY3AKQT38ZK"
title = "Add arcl hub ready and arcl hub status"
status = "pending"
priority = "normal"
position = 4
blocked-by = ["arcl-t-01M6EMZ9QGD7JVQ5JC346KDJA7"]
+++

Terminal views of the hub, for agents and people without a browser.

Scope:

- `arcl hub ready`: ready tasks across projects, grouped by project
- `arcl hub status`: per project, open milestones with progress and counts of
  in-progress, parked and ready tasks
- human, `--plain` and `--json` output like the other commands

Acceptance criteria:

- [ ] `arcl hub ready --json` lists the same tasks as `arcl ready` in each project
- [ ] An unreachable project is reported and the command still succeeds

Verification:

- CLI tests with fixture projects
- `just check`
