+++
id = "arcl-t-01M5QPZZKW6X9JPSGZ4VWRAMCY"
parent = "arcl-t-01M5NE5P4D72Y56E0Y2JDE535B"
title = "T10.3: Add the MCP server"
status = "completed"
priority = "normal"
position = 3
blocked-by = ["arcl-t-01M5QPZJRBMHJYTP3NVNHQ8T4Y"]
evidence = "Branch `claude/t10-3-mcp-server` adds the `arcl-mcp` crate and binary: 15 rmcp tools over stdio (show, list, ready, explain, context, capture_create, spec_create, plan_create, task_create, task_update, note_create, task_start, task_handoff, task_unpark, task_complete) that call `arcl_store::Database` directly under the CLI's workspace sync and lock. Results are the CLI envelopes with `_meta.arcl/format_version`; input and output JSON Schemas come from schemars derives behind a `schema` feature on arcl-core and arcl-store. `list` and `ready` page with `limit` 1-200 (default 50) and `next_cursor`. `task_start` has no force field and rejects unknown fields. `StorageError::exit_code` moved into arcl-store so both adapters share it. `cargo test -p arcl-mcp` (6 tests, including stdio and workspace sync), cargo fmt --check, clippy -D warnings, cargo test --workspace, pnpm check, pnpm test all pass."
+++

Expose agent operations as typed MCP tools that call the shared application
operations (T06) directly.

Scope:

- an `arcl-mcp` crate in the workspace, served over stdio
- tools for record reads and mutations, ready work, readiness explanation,
  context, handoff, and completion
- input and output schemas versioned like the CLI's JSON envelopes, reusing
  the shapes the automation guide documents (T17.3)
- the tool list in `skills/arclightning/` and the automation guide

Acceptance criteria:

- [x] Add MCP tools for record reads and mutations, ready work, readiness
      explanation, context, handoff, and completion
- [x] Reuse application operations and errors without invoking or scraping the
      CLI
- [x] Publish versioned input and output schemas with bounded list pagination
- [x] The start tool does not expose force (T16)
- [x] The Arc Lightning skill names the MCP tools next to the commands

Verification:

- `cargo test -p arcl-mcp`
- `just check`

Out of scope:

- The end-to-end workflow and error coverage (T10.4)
