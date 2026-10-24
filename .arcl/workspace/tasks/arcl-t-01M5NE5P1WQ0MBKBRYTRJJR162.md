+++
id = "arcl-t-01M5NE5P1WQ0MBKBRYTRJJR162"
phase = "arcl-ph-01M5NE5P14T2DD4DCF1P6X059E"
title = "T16: Make blocking relationships strict"
status = "pending"
priority = "normal"
position = 0
blocked-by = ["arcl-t-01M5NH23576GMX0CPHRBWK4DA1"]
+++

Make blockers apply through the task hierarchy and to task transitions, so
automations can trust readiness.

Blocked by: T04

Delivered by subtasks T16.1–T16.6. The PR for T16.6, the last of them,
completes T16 as well (Decision: Parent completion).

Acceptance criteria:

- [ ] Treat a task as blocked when it or any ancestor task has an unfinished
      blocker, and name that ancestor in readiness explanations
- [ ] Refuse `task start` from pending for a task that is not ready unless
      the caller passes `--force`; having child tasks does not count against
      start, starting an in-progress task stays a no-op, and the MCP start
      tool does not expose force
- [ ] Refuse blocker edges that form a cycle or join a task to its own
      ancestor or descendant; report existing ones through readiness,
      `arcl check`, and import with the path that forms them
- [ ] Report a cancelled blocker as its own readiness reason that asks for the
      dependency to be removed or replaced
- [ ] Keep dependency validation the same across the CLI, plan application,
      and workspace import
- [ ] Complete a task and its open subtasks in one atomic command,
      `task complete --with-subtasks`

Verification:

- `cargo test -p arcl-store execution::tests`
- `cargo test -p arcl-cli --test cli task_dependencies`
- The new test filters each subtask names
- Cover inherited blockers, forced and refused starts, repeat starts,
  ancestor and descendant edges, cancelled blockers, and completion with
  subtasks in tests
- Update `connected_execution_keeps_invalid_writes_atomic` in
  `crates/arcl-store/src/storage/connected/execution.rs` on purpose: its start
  after a forced container close needs `--force` once readiness applies
- `just check`
