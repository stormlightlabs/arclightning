+++
id = "arcl-t-01M5NE5P1WQ0MBKBRYTRJJR162"
phase = "arcl-ph-01M5NE5P14T2DD4DCF1P6X059E"
title = "T16: Make blocking relationships strict"
status = "pending"
priority = "normal"
position = 0
+++

Make blockers apply through the task hierarchy and to task transitions, so
automations can trust readiness.

Blocked by: T04

Acceptance criteria:

- [ ] Treat a task as blocked when it or any ancestor task has an unfinished
      blocker, and name that ancestor in readiness explanations
- [ ] Refuse `task start` from pending for a task that is not ready unless
      the caller passes `--force`; starting an in-progress task stays a no-op,
      and the MCP start tool does not expose force
- [ ] Refuse blocker edges that form a cycle or join a task to its own
      ancestor or descendant; report existing ones through readiness,
      `arcl check`, and import with the path that forms them
- [ ] Report a cancelled blocker as its own readiness reason that asks for the
      dependency to be removed or replaced
- [ ] Keep dependency validation the same across the CLI, plan application,
      and workspace import

Verification:

- `cargo test --workspace --all-features ready`
- `cargo test --workspace --all-features dependency`
- Cover inherited blockers, forced and refused starts, repeat starts,
  ancestor and descendant edges, and cancelled blockers in tests
- Update `connected_execution_keeps_invalid_writes_atomic` in
  `crates/arcl-store/src/storage/connected/execution.rs` on purpose: its start
  after a forced container close needs `--force` once readiness applies
