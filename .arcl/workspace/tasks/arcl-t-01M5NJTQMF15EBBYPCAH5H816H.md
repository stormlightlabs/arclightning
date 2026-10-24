+++
id = "arcl-t-01M5NJTQMF15EBBYPCAH5H816H"
parent = "arcl-t-01M5NE5P1WQ0MBKBRYTRJJR162"
title = "T16.2: Refuse starting unready tasks without --force"
status = "completed"
priority = "normal"
position = 2
evidence = '`arcl task start` now refuses a pending task that readiness reports as not ready and lists the reasons in a new `DomainError::NotReady`; `--force` starts it anyway, a repeat start of an in-progress task stays a no-op, and the "has child tasks" reason is the only one start skips. The store gains `start_planning_task(id, force)` with force as an explicit argument, and `transition_planning_task` never forces a start. `connected_execution_keeps_invalid_writes_atomic` now forces its start after the forced plan close. Tests added: three `start_` store tests in `execution::tests` and five `start_` CLI tests in `crates/arcl-cli/tests/cli.rs`. Website task, lifecycle and manual pages describe the gate. Gates run: cargo fmt --check, clippy -D warnings, cargo test --workspace, pnpm check, pnpm test, all passing. Branch claude/t16-2-start-gating.'
+++

Make `arcl task start` refuse a pending task that is not ready unless the
caller passes `--force`. Today `transition` in
`crates/arcl-store/src/storage/connected/execution.rs` applies `Start` from
any pending state without consulting readiness.

Having child tasks does not count against start: a parent starts without
`--force` when it and its ancestors have no unfinished blockers and no other
readiness reason applies. Every other reason still refuses the start.

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
- [ ] The "has child tasks" reason is the only one start skips: a pending
      parent with no other reason starts without `--force`, and a parent with
      an unfinished blocker is refused
- [ ] `task start` on an in-progress task stays a no-op and succeeds, with or
      without `--force`
- [ ] The store API keeps force as an explicit argument, so the MCP start tool
      (T10) can call start without exposing it
- [ ] `connected_execution_keeps_invalid_writes_atomic` passes `force` for its
      start after the forced plan close, changed on purpose with a comment
      saying why

Verification:

- `cargo test -p arcl-store execution::tests` (includes
  `connected_execution_keeps_invalid_writes_atomic`, plus the new store tests
  in that module)
- `cargo test -p arcl-cli --test cli start_` (new tests, named `start_…`):
  refused start of a blocked task with its JSON error, forced start, repeat
  start of an in-progress task, parent with children started without
  `--force`, blocked parent refused
- `just check`

Out of scope:

- New readiness reasons; start uses whatever `readiness` reports (T16.1 adds
  inherited and cancelled blockers)
- The MCP server itself (T10)
