+++
id = "arcl-t-01M5NE5P2N1AMT731RJBAZ66YT"
phase = "arcl-ph-01M5NE5P14T2DD4DCF1P6X059E"
title = "T17: Report readiness and unblocking as data"
status = "pending"
priority = "normal"
position = 1
blocked-by = ["arcl-t-01M5NE5P1WQ0MBKBRYTRJJR162"]
+++

Give automations typed readiness reasons and the tasks a completion unblocks.

Blocked by: T16

Acceptance criteria:

- [ ] Add a stable `kind` and record IDs to every readiness reason in JSON,
      alongside the existing human text
- [ ] Return the tasks that became ready when a task completes
- [ ] Version the JSON shapes and document them for the MCP server

Verification:

- `cargo test --workspace --all-features cli`
- Chain completion to newly ready work using JSON output only
