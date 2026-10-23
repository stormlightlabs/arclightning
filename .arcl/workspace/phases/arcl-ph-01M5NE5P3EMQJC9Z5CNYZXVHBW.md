+++
id = "arcl-ph-01M5NE5P3EMQJC9Z5CNYZXVHBW"
plan = "arcl-pl-01M5NE5P0CWMJNP2A7B0ZQW6XC"
title = "Milestone 5: Agent interface"
status = "open"
position = 5
+++

Make the Rust application operations available directly to agents.

Deliverables:

- an MCP server with typed tools for capture, planning, task execution, notes,
  readiness, and context
- stable schemas and error responses
- an Arc Lightning skill that teaches agents the intended planning and handoff
  workflow without duplicating product rules

Exit criterion: an agent can inspect project context, claim ready work, leave a
handoff, and complete a task without parsing human CLI output
