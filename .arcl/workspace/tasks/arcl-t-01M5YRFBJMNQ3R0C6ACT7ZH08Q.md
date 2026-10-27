+++
id = "arcl-t-01M5YRFBJMNQ3R0C6ACT7ZH08Q"
phase = "arcl-ph-01M5YRFB35TZ51QW4AG1A8MS5B"
title = "Render a roadmap with arcl roadmap"
status = "pending"
priority = "normal"
position = 7
+++

Render specs, plans, milestones and releases as one Markdown page, for a
README or a docs site. Taken from beans' `beans roadmap`.

Scope:

- `arcl roadmap` printing Markdown: each spec, its plans and milestones with
  progress, then releases
- `--output <file>`
- leaving out closed captures and notes

Acceptance criteria:

- [ ] `arcl roadmap` prints a Markdown page with milestone progress
- [ ] The output is stable for the same data
- [ ] `--output` writes the file

Verification:

- a snapshot test on a fixture project
- `just check`
