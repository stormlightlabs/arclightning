+++
id = "arcl-t-01M5NWNGG79B4R0N81TXPSQ23W"
parent = "arcl-t-01M5NE5P1WQ0MBKBRYTRJJR162"
title = "T16.4: Run the blocker edge validator in plan application, import, and reparenting"
status = "completed"
priority = "normal"
position = 4
blocked-by = ["arcl-t-01M5NJTQP8VGM3BEGYT1EDX09B"]
evidence = "Plan application, workspace import and task update --parent refuse blocker edge cycles and ancestor or descendant edges through the shared validator. A reparent refuses only violations whose parent chain crosses the link it sets."
+++

Run the T16.3 blocker edge validator in plan application, workspace import,
and reparenting, so every write path refuses the same edges. Today plan
application uses `dependency_reaches` and a recursive SQL query, import uses
`validate_dependency_cycles`, and `update_planning_task` checks only the moved
task's ancestry.

Scope:

- `prepare_plan` and `insert_plan_dependency` in
  `crates/arcl-store/src/storage/connected/plan_ops.rs`
- `validate_dependency_cycles` in `crates/arcl-repo/src/workspace/import.rs`
- `update_planning_task` in `crates/arcl-store/src/storage/connected.rs`
  (`task update --parent`), and parent changes during plan application
- tests in `plan_ops.rs`, `import.rs`, and `crates/arcl-cli/tests/cli.rs`

Acceptance criteria:

- [x] Plan application refuses a cycle, an ancestor edge, or a descendant edge
      with the same error as `dependency add`, and writes nothing
- [x] Plan application and reparenting refuse only violations that involve an
      edge the write adds or a task in the moved subtree. A violation already
      elsewhere in the graph does not block them; T16.5 reports it
- [x] Workspace import refuses any violation in the workspace, naming the
      record path and the task path
- [x] A reparent, through `task update --parent` or plan application, runs the
      validator on the whole graph after the move, so it covers every task in
      the moved subtree, not only the moved task
- [x] Moving task A under P is refused when A's child C is blocked by P, and
      the error names the chain P → A → C
- [x] `dependency_reaches`, the recursive SQL query in
      `insert_plan_dependency`, and `validate_dependency_cycles` are gone

Verification:

- `cargo test -p arcl-store structured_plan` (existing
  `structured_plan_diff_and_apply_are_idempotent`)
- `cargo test -p arcl-repo import::tests` (existing import tests, plus the new
  ones in that module)
- `cargo test -p arcl-cli --test cli dependency_edge_` (new tests, named
  `dependency_edge_…`): plan cycle, plan ancestor edge, import descendant
  edge, the A/C/P reparent through `task update --parent` and through plan
  application, an unrelated plan application and reparent that succeed with
  a bad edge already in the database, and one table-driven test that feeds
  the same graph to the CLI, plan application, and import and expects the
  same refusal
- `just check`

Out of scope:

- Reporting invalid edges already in the database through readiness and
  `arcl check` (T16.5)
