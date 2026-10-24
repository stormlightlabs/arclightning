+++
id = "arcl-t-01M5NE5P2N1AMT731RJBAZ66YT"
phase = "arcl-ph-01M5NE5P14T2DD4DCF1P6X059E"
title = "T17: Report readiness and unblocking as data"
status = "completed"
priority = "normal"
position = 1
blocked-by = ["arcl-t-01M5NE5P1WQ0MBKBRYTRJJR162"]
evidence = "Delivered by T17.1–T17.3: #23, #22 and the PR completing T17.3."
+++

Give automations typed readiness reasons and the tasks a completion unblocks.

Blocked by: T16

Delivered by subtasks T17.1–T17.3. The PR for T17.3, the last of them,
completes T17 as well (Decision: Parent completion).

Acceptance criteria:

- [ ] Add a stable `kind` and record IDs to every readiness reason in JSON,
      alongside the existing human text
- [ ] Return the tasks that became ready when a task completes
- [ ] Version the JSON shapes and document them for the MCP server

Verification:

- `cargo test -p arcl-cli --test cli task_dependencies`
- The new test filters each subtask names
- Chain completion to newly ready work using JSON output only
- `just check`
