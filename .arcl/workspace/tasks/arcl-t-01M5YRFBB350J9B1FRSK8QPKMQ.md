+++
id = "arcl-t-01M5YRFBB350J9B1FRSK8QPKMQ"
phase = "arcl-ph-01M5YRFB35TZ51QW4AG1A8MS5B"
title = "Record typed notes on a task"
status = "completed"
priority = "normal"
position = 3
evidence = "Delivered by #77: `arcl task note <id> [--kind decision|observation|blocker|result] <text>` appends a timestamped note, stored in the `task_notes` table (migration 006), in task files as a `[[notes]]` array that moves the workspace to format version 3, and in `--json`. `show` and the web Task view list notes newest first, `context` the 10 most recent with `omitted_notes`, and the MCP server has `task_note`."
+++

Let an agent append short notes to a task while working on it, so what it
tried and decided survives context compaction. Taken from chainlink's
breadcrumbs and typed comments.

`arcl task note <id> [--kind decision|observation|blocker|result] <text>`
appends a timestamped note. Handoff and evidence stay as they are.

Scope:

- notes stored with the task, in the workspace files and `--json`
- `task note`, and notes in `show`, `context` and the Task view
- a limit on how many recent notes `context` includes

Acceptance criteria:

- [x] `task note` appends a timestamped note with an optional kind
- [x] `show`, `context` and the Task view list notes, newest first
- [x] Notes round-trip through the workspace files

Verification:

- CLI tests for each kind and for the `context` limit
- `just check`
