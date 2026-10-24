+++
id = "arcl-t-01M5NJTQR1PZ1QHC309R63EY1J"
parent = "arcl-t-01M5NE5P1WQ0MBKBRYTRJJR162"
title = "T16.5: Report existing invalid blocker edges with their path"
status = "pending"
priority = "normal"
position = 5
blocked-by = ["arcl-t-01M5NJTQJRC8CNJFBF5T4RVRZS", "arcl-t-01M5NJTQP8VGM3BEGYT1EDX09B"]
+++

Report invalid blocker edges that already exist in the database, through
readiness and `arcl check`, with the path that forms them. A database written
before T16.3, or edited by hand, can hold a cycle or an edge between a task and
its ancestor or descendant. Today `arcl check` reports only "dependency cycle
includes `<id>`" with no path, and readiness reports nothing.

Scope:

- `check` and `has_dependency_cycle` in
  `crates/arcl-store/src/storage/connected.rs`, using the T16.3 validator
- `readiness` in `crates/arcl-store/src/storage/connected/execution.rs`, after
  T16.1's changes to it
- tests that insert invalid edges with raw SQL

Acceptance criteria:

- [ ] `arcl check` reports each cycle once with its task path, and each
      ancestor or descendant edge with the parent chain that joins the tasks,
      and exits non-zero
- [ ] Readiness for a task on an invalid edge is false, with a reason that
      gives the same path
- [ ] `has_dependency_cycle` is replaced by the shared validator

Verification:

- `cargo test -p arcl-store invalid_edge_` (new store tests, named
  `invalid_edge_…`): raw-SQL cycle and ancestor edge reported by `check` and
  by readiness
- `cargo test -p arcl-cli --test cli invalid_edge_` (new tests, named
  `invalid_edge_…`): the same edges through `arcl check` and `task explain`
- `just check`

Out of scope:

- Refusing new invalid edges and import validation (T16.3, T16.4)
