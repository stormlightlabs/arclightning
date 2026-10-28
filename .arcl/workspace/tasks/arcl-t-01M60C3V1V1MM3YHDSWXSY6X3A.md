+++
id = "arcl-t-01M60C3V1V1MM3YHDSWXSY6X3A"
phase = "arcl-ph-01M5YRFB35TZ51QW4AG1A8MS5B"
title = "Make --blocked-by repeatable and point a fresh worktree at arcl init"
status = "in_progress"
priority = "normal"
position = 5
+++

`arcl task create --blocked-by` takes several values per flag, so a title after it is read as a blocker ID. `arcl dependency add --blocked-by` takes one. Make both take one ID per flag, repeatable, with comma-separated IDs also accepted.

In a fresh worktree, `arcl` commands that need a database say to run `arcl init`. When a workspace exists, the error should also say that the command after `arcl init` imports it.

Acceptance:

- `arcl task create --blocked-by A "Title"` creates "Title" blocked by A.
- `--blocked-by A --blocked-by B` and `--blocked-by A,B` work the same in `task create` and `dependency add`.
- The missing-database error names the workspace, when one exists, and says the command after `arcl init` imports it.
- The manual, the skill and the help text match.
