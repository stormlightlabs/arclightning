+++
id = "arcl-t-01M5NJTQMF15EBBYPCAH5H816H"
parent = "arcl-t-01M5NE5P1WQ0MBKBRYTRJJR162"
title = "T16.2: Refuse starting unready tasks without --force"
status = "pending"
priority = "normal"
position = 2
+++

Make `arcl task start` refuse a pending task that is not ready unless the
caller passes `--force`. Today `transition` in
`crates/arcl-store/src/storage/connected/execution.rs` applies `Start` from
any pending state without consulting readiness.

Scope:

- `transition` and `transition_planning_task` in
  `crates/arcl-store/src/storage/connected/execution.rs`, and the
  `Database::transition_planning_task` wrapper in
  `crates/arcl-store/src/storage/mod.rs` (add a separate force argument or a
  start-specific entry point; keep `allow_open_children` for complete and
  cancel)
- a not-ready error in `crates/arcl-core/src/domain/error.rs` that carries the
  readiness reasons
- `TaskCommand::Start` in `crates/arcl-cli/src/cli.rs` and
  `crates/arcl-cli/src/app/commands.rs`
- tests in `execution.rs` and `crates/arcl-cli/tests/cli.rs`

Acceptance criteria:

- [ ] `task start` on a pending task whose readiness is false fails with an
      error that lists the readiness reasons, and writes nothing
- [ ] `task start --force` starts the same task
- [ ] `task start` on an in-progress task stays a no-op and succeeds, with or
      without `--force`
- [ ] The store API keeps force as an explicit argument, so the MCP start tool
      (T10) can call start without exposing it
- [ ] `connected_execution_keeps_invalid_writes_atomic` passes `force` for its
      start after the forced plan close, changed on purpose with a comment
      saying why

Verification:

- `cargo test -p arcl-store connected_execution`
- `cargo test --workspace --all-features ready`
- New tests: refused start of a blocked task, forced start, repeat start of an
  in-progress task, CLI JSON error for a refused start
- `just check`

Out of scope:

- New readiness reasons; start uses whatever `readiness` reports (T16.1 adds
  inherited and cancelled blockers)
- The MCP server itself (T10)

Note: readiness is false for a task with child tasks, so starting a parent
also needs `--force` under this rule. Keep that behaviour unless the owner
decides otherwise.
