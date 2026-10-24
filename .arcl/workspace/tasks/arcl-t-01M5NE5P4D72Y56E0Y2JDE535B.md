+++
id = "arcl-t-01M5NE5P4D72Y56E0Y2JDE535B"
phase = "arcl-ph-01M5NE5P3EMQJC9Z5CNYZXVHBW"
title = "T10: Add the MCP server and Arc Lightning skill"
status = "completed"
priority = "normal"
position = 0
blocked-by = ["arcl-t-01M5NE5P2N1AMT731RJBAZ66YT", "arcl-t-01M5NH23GE6N1SXWVK9BHVGP5V"]
evidence = "Delivered by T10.1–T10.4: #30, #33, #34 and #38."
+++

Expose agent operations through typed MCP tools and document the intended
planning, context, handoff, and evidence workflow in a project skill.

Blocked by: T06, T17

Delivered by subtasks T10.1–T10.4. The PR for T10.4, the last of them,
completes T10 as well (Decision: Parent completion).

Acceptance criteria:

- [x] Add MCP tools for record reads and mutations, ready work, readiness
      explanation, context, handoff, and completion (T10.3)
- [x] Reuse application operations and errors without invoking or scraping the
      CLI (T10.3)
- [x] Publish versioned input and output schemas with bounded list pagination
      (T10.3)
- [x] Add an Arc Lightning skill that refers to product commands and MCP tools
      without restating domain rules (T10.1, T10.3)
- [x] Install the skill with `npx skills add` and with `arcl skill install`
      (T10.1, T10.2)
- [x] Cover tool authorization boundaries and invalid-transition errors
      (T10.4)

Verification:

- `cargo test --workspace --all-features -p arcl-mcp`
- Run one capture-to-completion workflow through MCP (T10.4)
- `just check`
