+++
id = "arcl-t-01M6EMZ9XWMYKAPSXY2NSANKPK"
phase = "arcl-ph-01M6EMZ9DHDAY6RXY3AKQT38ZK"
title = "Show ready and in-flight work across projects"
status = "pending"
priority = "normal"
position = 3
blocked-by = ["arcl-t-01M6EMZ9TNAM4HNKRAJR7YS76G"]
+++

Show the work that can start, and the work in flight, across projects.

Scope:

- a hub view of ready tasks across projects in readiness order within each
  project, filterable by project and priority
- in-progress and parked tasks with assignee and the last handoff
- open gates and tasks completed in the last seven days

Acceptance criteria:

- [ ] Ready work matches `arcl ready` in each project
- [ ] Handoffs and gates match `arcl show` for each task

Verification:

- browser tests with fixture projects
- `just check`
