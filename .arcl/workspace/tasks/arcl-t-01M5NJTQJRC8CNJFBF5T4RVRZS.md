+++
id = "arcl-t-01M5NJTQJRC8CNJFBF5T4RVRZS"
parent = "arcl-t-01M5NE5P1WQ0MBKBRYTRJJR162"
title = "T16.1: Inherit ancestor blockers and report cancelled ones"
status = "completed"
priority = "normal"
position = 1
evidence = """Readiness now walks the task's parent chain: a blocker on any ancestor blocks the task, sets readiness.blocked, and gives the reason "ancestor `<id>` is blocked by `<id>` (<status>)". A cancelled blocker, direct or inherited, gets "... blocked by cancelled `<id>`; remove or replace the dependency" instead of the wait reason. Completing the ancestor's blocker makes the descendant ready again. Store tests readiness_inherits_ancestor_blockers_until_they_complete and readiness_reports_cancelled_blockers_as_decisions in execution.rs; CLI tests readiness_inherits_a_blocker_through_two_levels_of_parents, readiness_asks_to_replace_a_cancelled_direct_blocker, readiness_asks_to_replace_a_cancelled_inherited_blocker and readiness_returns_when_the_ancestor_blocker_completes. Updated the deps reference and the inspect-work guide. Gates run (just check recipe): cargo fmt --check, clippy -D warnings, cargo test --workspace, pnpm check, pnpm test, all passing. Branch claude/t16-1-inherited-blockers."""
+++

Make readiness count blockers inherited from ancestor tasks and report a
cancelled blocker as a decision to make. Today `readiness` in
`crates/arcl-store/src/storage/connected/execution.rs` reads only the task's
own blocker edges and words a cancelled blocker the same as a pending one.

Scope:

- `readiness` and `blocker_views` in
  `crates/arcl-store/src/storage/connected/execution.rs`
- human and plain readiness lines in `crates/arcl-cli/src/output.rs`, if the
  new reasons need them
- tests in `execution.rs` and `crates/arcl-cli/tests/cli.rs`

Acceptance criteria:

- [ ] A task is blocked, and absent from `arcl ready`, when any ancestor task
      has a blocker that is not completed
- [ ] The readiness reason for an inherited blocker names the ancestor and
      the blocker, for example ``ancestor `<id>` is blocked by `<id>` (pending)``
- [ ] `readiness.blocked` is true for inherited blockers as well as direct ones
- [ ] A cancelled blocker, direct or inherited, gets its own reason that asks
      for the dependency to be removed or replaced, distinct from the wait
      reason for a pending or in-progress blocker
- [ ] Completing the ancestor's blocker makes the descendant ready again

Verification:

- `cargo test -p arcl-store execution::tests` (existing tests, plus the new
  store tests in that module)
- `cargo test -p arcl-cli --test cli readiness_` (new tests, named
  `readiness_…`): inherited blocker through two levels of parents; cancelled
  direct blocker; cancelled inherited blocker; blocker completion restores
  readiness
- `just check`

Out of scope:

- Gating `task start` on readiness (T16.2)
- Refusing or reporting invalid edges (T16.3–T16.5)
- Typed reason kinds in JSON (T17.1)
