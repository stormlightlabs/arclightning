+++
id = "arcl-t-01M5NJTQSVWEDN7GN2GB7TXBW5"
parent = "arcl-t-01M5NE5P2N1AMT731RJBAZ66YT"
title = "T17.1: Add typed kinds and record IDs to readiness reasons"
status = "pending"
priority = "normal"
position = 1
blocked-by = ["arcl-t-01M5NJTQJRC8CNJFBF5T4RVRZS"]
+++

Give every readiness reason a stable `kind` and the record IDs it refers to,
so automations can act on reasons without parsing text. Today
`PlanningReadiness.reasons` in
`crates/arcl-store/src/storage/connected/execution.rs` is a list of strings.

Scope:

- a reason type (`kind`, `message`, record ID fields) next to
  `PlanningReadiness` in `execution.rs`, built by `readiness`
- JSON, human, and plain rendering in `crates/arcl-cli/src/output.rs` for
  `task explain`, `explain`, and `context`
- the not-ready error from `task start` (T16.2), if it carries reasons
- tests in `execution.rs` and `crates/arcl-cli/tests/cli.rs`

Acceptance criteria:

- [ ] Each reason in JSON has a `kind` from a fixed snake_case set, a
      `message` with the current human text, and the IDs of the records it
      names (task, ancestor, blocker, container, or the path for an invalid
      edge)
- [ ] The set covers every reason `readiness` produces after T16.1 and T16.5:
      status, children, closed ancestor, closed container, direct blocker,
      inherited blocker, cancelled blocker, missing blocker, invalid edge
- [ ] Human and plain output keep printing the message text

Verification:

- `cargo test -p arcl-store execution::tests`
- `cargo test -p arcl-cli --test cli reason_kind_` (new tests, named
  `reason_kind_…`): one per kind, asserting `kind` and IDs from
  `--json task explain`
- `just check`

Out of scope:

- Unblocked tasks on completion (T17.2)
- Bumping `format_version` and documenting shapes (T17.3)
