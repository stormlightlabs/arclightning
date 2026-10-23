+++
id = "arcl-t-01M5NJTQWCZTH6DWQ3F9VDVX4Z"
parent = "arcl-t-01M5NE5P2N1AMT731RJBAZ66YT"
title = "T17.2: Return the tasks a completion made ready"
status = "pending"
priority = "normal"
position = 2
blocked-by = ["arcl-t-01M5NJTQJRC8CNJFBF5T4RVRZS"]
+++

Return the tasks a completion made ready, so an automation can chain from one
finished task to the next. Today `task complete` returns only the completed
task.

Scope:

- `complete_planning_task` and `transition` in
  `crates/arcl-store/src/storage/connected/execution.rs`: compute the ready set
  before and after the write in the same transaction
- the `completed` rendering in `crates/arcl-cli/src/output.rs` and
  `TaskCommand::Complete` in `crates/arcl-cli/src/app/commands.rs`
- tests in `execution.rs` and `crates/arcl-cli/tests/cli.rs`

Acceptance criteria:

- [ ] `task complete --json` includes an `unblocked` list of tasks that were
      not ready before the completion and are ready after it
- [ ] The list includes descendants of a task that the completed task blocked
      (inherited blockers, T16.1), and excludes tasks still waiting on another
      blocker
- [ ] Human and plain output name the unblocked tasks
- [ ] A completion that unblocks nothing returns an empty list

Verification:

- `cargo test --workspace --all-features cli`
- New tests: direct dependent unblocked, child of a dependent unblocked,
  dependent with a second open blocker not listed
- `just check`

Out of scope:

- Typed reasons (T17.1)
- Bumping `format_version` and documenting shapes (T17.3)
