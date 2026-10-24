+++
id = "arcl-t-01M5QPZZKW6X9JPSGZ4VWRAMCY"
parent = "arcl-t-01M5NE5P4D72Y56E0Y2JDE535B"
title = "T10.3: Add the MCP server"
status = "pending"
priority = "normal"
position = 3
blocked-by = ["arcl-t-01M5QPZJRBMHJYTP3NVNHQ8T4Y"]
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

- [ ] Add MCP tools for record reads and mutations, ready work, readiness
      explanation, context, handoff, and completion
- [ ] Reuse application operations and errors without invoking or scraping the
      CLI
- [ ] Publish versioned input and output schemas with bounded list pagination
- [ ] The start tool does not expose force (T16)
- [ ] The Arc Lightning skill names the MCP tools next to the commands

Verification:

- `cargo test -p arcl-mcp`
- `just check`

Out of scope:

- The end-to-end workflow and error coverage (T10.4)
