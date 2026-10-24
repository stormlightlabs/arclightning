+++
id = "arcl-t-01M5NWNVGS20D75FSEZTFGM0CB"
parent = "arcl-t-01M5NE5P1WQ0MBKBRYTRJJR162"
title = "T16.6: Complete a task together with its open subtasks"
status = "completed"
priority = "normal"
position = 6
blocked-by = ["arcl-t-01M5NJTQJRC8CNJFBF5T4RVRZS", "arcl-t-01M5NJTQMF15EBBYPCAH5H816H", "arcl-t-01M5NJTQR1PZ1QHC309R63EY1J", "arcl-t-01M5NWNGG79B4R0N81TXPSQ23W"]
evidence = "Added `task complete --with-subtasks`, which completes a task and its pending and in-progress descendants in one transaction and refuses, writing nothing, on a parked descendant or an unfinished blocker outside the subtree; JSON lists `completed_subtasks` in tree order. Delivered by the PR from claude/t16-6-complete-with-subtasks."
+++

Add `task complete --with-subtasks`, which completes a task and all its open
subtasks in one transaction. Today `task complete` refuses a task with open
descendants, and `--allow-open-children` completes only the task and leaves
its descendants open.

Rules:

- It completes the task and every pending or in-progress descendant. It leaves
  completed and cancelled descendants as they are.
- It refuses, and writes nothing, when any descendant is parked. The error
  lists them; the caller unparks or cancels them first.
- It refuses, and writes nothing, when the task or any open descendant has an
  unfinished blocker outside the subtree, direct or inherited from an ancestor
  of the task (T16.1). There is no `--force` for this.
- Blockers inside the subtree do not count, because the owner's decision
  treats the subtree as one unit. This includes a cancelled descendant that
  blocks another descendant: it stays cancelled, and the command still
  completes the task it blocked.
- Evidence goes on the task only. Descendants keep their own evidence.
- `--with-subtasks` conflicts with `--allow-open-children`, which keeps its
  current behaviour.

Scope:

- `complete_planning_task` and `transition` in
  `crates/arcl-store/src/storage/connected/execution.rs`, and the matching
  `Database` wrapper in `crates/arcl-store/src/storage/mod.rs`
- `TaskCommand::Complete` in `crates/arcl-cli/src/cli.rs` and
  `crates/arcl-cli/src/app/commands.rs`
- the `completed` rendering in `crates/arcl-cli/src/output.rs`
- tests in `execution.rs` and `crates/arcl-cli/tests/cli.rs`

Acceptance criteria:

- [x] `task complete <id> --with-subtasks` completes the task and its pending
      and in-progress descendants at every depth in one transaction
- [x] Completed and cancelled descendants keep their status and evidence
- [x] A parked descendant, or an unfinished blocker outside the subtree on the
      task or an open descendant, refuses the command with an error naming the
      tasks, and nothing is written
- [x] `--evidence` and `--evidence-file` set the task's evidence only
- [x] `--json` returns the task and a `completed_subtasks` list of the IDs the
      command completed, in tree order; human and plain output name them
- [x] `--with-subtasks` with `--allow-open-children` is a usage error, and
      `--allow-open-children` alone behaves as before
- [x] This PR also completes T16, with the T16 subtask PRs as its evidence

Verification:

- `cargo test -p arcl-store execution::tests` (existing tests, plus the new
  store tests in that module)
- `cargo test -p arcl-cli --test cli complete_with_subtasks_` (new tests,
  named `complete_with_subtasks_…`): nested pending and in-progress subtasks
  completed with JSON output checked; cancelled and completed subtasks left
  alone; parked subtask refused; outside blocker refused; inside blocker
  allowed; cancelled inside blocker allowed, with the blocker left cancelled
  and the task it blocked completed; conflict with `--allow-open-children`;
  `--allow-open-children` unchanged
- `cargo test -p arcl-cli --test cli start_blocked_ancestor_refused`: `task
  start` of a task whose ancestor has an unfinished blocker fails without
  `--force` and succeeds with it, which checks T16.1 and T16.2 together
- `just check`

Blocked by T16.1 and T16.2 for code: the outside-blocker rule reads inherited
blockers, T16.2 changes the same `transition` signature, and the
blocked-ancestor start test needs both. Blocked by T16.4
and T16.5 only so this is the last T16 subtask and its PR completes T16.

Out of scope:

- Cancelling a task together with its subtasks
- The `unblocked` list (T17.2)
