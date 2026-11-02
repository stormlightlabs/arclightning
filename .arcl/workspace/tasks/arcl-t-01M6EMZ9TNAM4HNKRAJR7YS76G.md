+++
id = "arcl-t-01M6EMZ9TNAM4HNKRAJR7YS76G"
phase = "arcl-ph-01M6EMZ9DHDAY6RXY3AKQT38ZK"
title = "Show every project on a hub dashboard"
status = "pending"
priority = "normal"
position = 2
blocked-by = ["arcl-t-01M6EMZ9QGD7JVQ5JC346KDJA7"]
+++

A hub page that shows every project at a glance.

Scope:

- one row per project: open milestones with progress, counts of ready,
  in-progress and parked tasks, and open gates
- built from the web app's shared components and themes, served by
  `arcl hub serve`
- each record links to its own project: the project's `arcl serve` URL when
  one is given, otherwise its workspace path

Acceptance criteria:

- [ ] The page renders three fixture projects from one `/graphql` request
- [ ] Progress matches `arcl tree` for each project
- [ ] A project that failed to open shows an error row

Verification:

- browser tests with fixture projects
- `just check`
