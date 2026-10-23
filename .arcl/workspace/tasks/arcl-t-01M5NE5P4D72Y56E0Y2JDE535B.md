+++
id = "arcl-t-01M5NE5P4D72Y56E0Y2JDE535B"
phase = "arcl-ph-01M5NE5P3EMQJC9Z5CNYZXVHBW"
title = "T10: Add the MCP server and Arc Lightning skill"
status = "pending"
priority = "normal"
position = 0
blocked-by = ["arcl-t-01M5NE5P2N1AMT731RJBAZ66YT"]
+++

Expose agent operations through typed MCP tools and document the intended
planning, context, handoff, and evidence workflow in a project skill.

Blocked by: T06, T17

Acceptance criteria:

- [ ] Add MCP tools for record reads and mutations, ready work, readiness
      explanation, context, handoff, and completion
- [ ] Reuse application operations and errors without invoking or scraping the
      CLI
- [ ] Publish versioned input and output schemas with bounded list pagination
- [ ] Add an Arc Lightning skill that refers to product commands and MCP tools
      without restating domain rules
- [ ] Cover tool authorization boundaries and invalid-transition errors

Verification:

- `cargo test --workspace --all-features -p arcl-mcp`
- Run one capture-to-completion workflow through MCP
