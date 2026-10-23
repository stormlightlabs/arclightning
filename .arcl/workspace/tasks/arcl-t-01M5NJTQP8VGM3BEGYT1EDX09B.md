+++
id = "arcl-t-01M5NJTQP8VGM3BEGYT1EDX09B"
parent = "arcl-t-01M5NE5P1WQ0MBKBRYTRJJR162"
title = "T16.3: Validate blocker edges the same way everywhere"
status = "pending"
priority = "normal"
position = 3
+++

Validate blocker edges with one shared check, used by `dependency add`,
`task create --blocked-by`, plan application, and workspace import. The check
refuses an edge that closes a cycle or joins a task to its own ancestor or
descendant, and names the path that forms it. Today each path has its own
cycle check (three recursive SQL queries in arcl-store, `dependency_reaches`
in plan application, `validate_dependency_cycles` in import) and none checks
ancestry.

Scope:

- a pure validator in `crates/arcl-core/src/domain/dependency.rs` over task
  parents and edges, returning a violation with its path; new error variants
  in `crates/arcl-core/src/domain/error.rs`
- `add_planning_dependency` and `create_planning_task_with_dependencies` in
  `crates/arcl-store/src/storage/connected.rs`
- `prepare` and `insert_plan_dependency` in
  `crates/arcl-store/src/storage/connected/plan_ops.rs`
- `validate_dependency_cycles` in `crates/arcl-repo/src/workspace/import.rs`
- reparenting through `task update --parent` and plan application, so a move
  cannot put a task under its own blocker or blocked task

Acceptance criteria:

- [ ] Adding an edge that closes a cycle fails with an error that lists the
      task IDs along the cycle
- [ ] Adding an edge between a task and its ancestor or descendant fails, in
      either direction, and the error names the parent chain that joins them
- [ ] Reparenting that would join a task to its blocker or blocked task fails
      the same way
- [ ] The CLI, plan application, and workspace import call the same validator
      and refuse the same edges; import names the record path and the task
      path
- [ ] The three SQL cycle queries and `dependency_reaches` are gone

Verification:

- `cargo test -p arcl-core dependency`
- `cargo test --workspace --all-features dependency`
- New tests for each entry point: cycle, ancestor edge, descendant edge,
  reparent under a blocker; one table-driven test that feeds the same graph to
  CLI, plan application, and import and expects the same refusal
- `just check`

Out of scope:

- Reporting invalid edges already in the database through readiness and
  `arcl check` (T16.4)
