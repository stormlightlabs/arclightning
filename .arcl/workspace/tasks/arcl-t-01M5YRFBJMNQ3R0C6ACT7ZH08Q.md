+++
id = "arcl-t-01M5YRFBJMNQ3R0C6ACT7ZH08Q"
phase = "arcl-ph-01M5YRFB35TZ51QW4AG1A8MS5B"
title = "Render a roadmap with arcl roadmap"
status = "completed"
priority = "normal"
position = 8
evidence = "Done in this PR at c8b943f: `arcl roadmap` prints specs, plans and phases with task progress, then releases, as Markdown. Golden test on the REST fixture (roadmap.md), a repeat-run byte check, and an `--output` test that the file matches stdout. Gates pass."
+++

Render specs, plans, milestones and releases as one Markdown page, for a
README or a docs site. Taken from beans' `beans roadmap`.

Scope:

- `arcl roadmap` printing Markdown: each spec, its plans and milestones with
  progress, then releases
- `--output <file>`
- leaving out closed captures and notes

Acceptance criteria:

- [x] `arcl roadmap` prints a Markdown page with milestone progress
- [x] The output is stable for the same data
- [x] `--output` writes the file

Verification:

- a snapshot test on a fixture project
- `just check`
