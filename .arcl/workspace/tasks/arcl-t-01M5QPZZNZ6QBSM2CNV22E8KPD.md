+++
id = "arcl-t-01M5QPZZNZ6QBSM2CNV22E8KPD"
parent = "arcl-t-01M5NE5P4D72Y56E0Y2JDE535B"
title = "T10.4: Run a capture-to-completion workflow through MCP"
status = "completed"
priority = "normal"
position = 4
blocked-by = ["arcl-t-01M5QPZZKW6X9JPSGZ4VWRAMCY"]
evidence = "Delivered by #PR: adds the `capture_promote` tool, which the acceptance criteria need and T10.3 left out, and `crates/arcl-mcp/tests/workflow.rs`, which drives the `arcl-mcp` binary over stdio. `a_capture_goes_to_completion_through_mcp_tools` runs capture, promote, ready, start, context, handoff, unpark, start and complete with tools alone and checks each result against its output schema. `tools_stay_inside_their_authority` covers read-only hints, the missing force option, the open-subtask refusal, task placement on a non-task promotion and an unlisted tool. `invalid_transitions_carry_the_application_error` checks that each refused transition returns the failure envelope of the store error for the same call."
+++

Prove the MCP server end to end and cover its error boundaries. This completes
T10.

Scope:

- integration tests in `crates/arcl-mcp/tests/` that drive the server over
  stdio

Acceptance criteria:

- [x] A test runs one capture-to-completion workflow through MCP tools only:
      capture, promote, ready, start, context, handoff, unpark, start, and
      complete
- [x] Cover tool authorization boundaries and invalid-transition errors, and
      check that errors carry the application error's code
- [x] This PR also completes T10, with the T10 subtask PRs as its evidence

Verification:

- `cargo test --workspace --all-features -p arcl-mcp`
- `just check`

Out of scope:

- New tools (T10.3)
