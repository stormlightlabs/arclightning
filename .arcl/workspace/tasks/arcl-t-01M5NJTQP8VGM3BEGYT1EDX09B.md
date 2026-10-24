+++
id = "arcl-t-01M5NJTQP8VGM3BEGYT1EDX09B"
parent = "arcl-t-01M5NE5P1WQ0MBKBRYTRJJR162"
title = "T16.3: Add a shared blocker edge validator for dependency add and task create"
status = "pending"
priority = "normal"
position = 3
+++

Add one blocker edge validator and use it for `dependency add` and
`task create --blocked-by`. The validator refuses an edge that closes a cycle
or joins a task to its own ancestor or descendant, and names the path that
forms it. Today each of these commands runs its own recursive SQL cycle query
and neither checks ancestry.

Scope:

- a pure validator in `crates/arcl-core/src/domain/dependency.rs` over task
  parents and edges, returning a violation with its path; new error variants
  in `crates/arcl-core/src/domain/error.rs`
- `add_planning_dependency` and `create_planning_task_with_dependencies` in
  `crates/arcl-store/src/storage/connected.rs`, which drop their recursive SQL
  queries
- tests in `dependency.rs`, `connected.rs`, and `crates/arcl-cli/tests/cli.rs`

Acceptance criteria:

- [ ] Adding an edge that closes a cycle fails with an error that lists the
      task IDs along the cycle, and writes nothing
- [ ] Adding an edge between a task and its ancestor or descendant fails, in
      either direction, and the error names the parent chain that joins them
- [ ] `dependency add` and `task create --blocked-by` call the validator and
      refuse the same edges
- [ ] The validator is public in arcl-core, takes the whole graph of parents
      and edges, and reports every violation in it, so T16.4 and T16.5 can
      reuse it unchanged
- [ ] The recursive SQL cycle queries in `add_planning_dependency` and
      `create_planning_task_with_dependencies` are gone

Verification:

- `cargo test -p arcl-core dependency::tests` (new module): cycle, ancestor
  edge, descendant edge, valid graph
- `cargo test -p arcl-store connected_dependencies` (existing
  `connected_dependencies_preserve_links_and_reject_cycles`, updated for the
  new error)
- `cargo test -p arcl-cli --test cli dependency_edge_` (new tests, named
  `dependency_edge_…`): cycle, ancestor edge, and descendant edge through
  `dependency add` and through `task create --blocked-by`
- `just check`

Out of scope:

- Plan application, workspace import, and reparenting (T16.4)
- Reporting invalid edges already in the database (T16.5)
