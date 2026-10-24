+++
id = "arcl-t-01M5NJTQJRC8CNJFBF5T4RVRZS"
parent = "arcl-t-01M5NE5P1WQ0MBKBRYTRJJR162"
title = "T16.1: Inherit ancestor blockers and report cancelled ones"
status = "completed"
priority = "normal"
position = 1
evidence = """Readiness walks the task's parent chain: a blocker on any ancestor blocks the task, sets readiness.blocked, and gives the reason "ancestor `<id>` is blocked by `<id>` (<status>)". An ancestor blocked by the task itself is skipped. A cancelled blocker, direct or inherited, gets "... blocked by cancelled `<id>`; remove or replace the dependency". Completing the ancestor's blocker makes the descendant ready again. Reasons list the task's own blockers, then each ancestor's, nearest first. explain and context keep `blockers` direct-only and add `inherited_blockers` (JSON, with `ancestor`), plain `inherited` (explain) and `inherited_blocker` (context) records, and "via" in human output; completion_evidence draws on both, once per blocker. Store and CLI tests in execution.rs and cli.rs, including the self-block, two-level order and evidence dedup cases. Updated the deps reference and the inspect-work guide. Gates passing: cargo fmt --check, clippy -D warnings, cargo test --workspace, pnpm check, pnpm test. Branch claude/t16-1-inherited-blockers."""
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

- [x] A task is blocked, and absent from `arcl ready`, when any ancestor task
      has a blocker that is not completed
- [x] The readiness reason for an inherited blocker names the ancestor and
      the blocker, for example ``ancestor `<id>` is blocked by `<id>` (pending)``
- [x] `readiness.blocked` is true for inherited blockers as well as direct ones
- [x] A cancelled blocker, direct or inherited, gets its own reason that asks
      for the dependency to be removed or replaced, distinct from the wait
      reason for a pending or in-progress blocker
- [x] Completing the ancestor's blocker makes the descendant ready again

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
