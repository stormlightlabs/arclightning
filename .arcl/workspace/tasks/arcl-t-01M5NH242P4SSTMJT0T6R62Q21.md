+++
id = "arcl-t-01M5NH242P4SSTMJT0T6R62Q21"
phase = "arcl-ph-01M5NH1M4357EDSS5J99NA8YY0"
title = "T09: Finish automatic reconciliation and conflict recovery"
status = "completed"
priority = "normal"
position = 1
blocked-by = ["arcl-t-01M5NH23XH2AZMTH5QDVWSTVX2"]
evidence = """
Delivered by:

- 95368db (feat: classify workspace paths against base, database and workspace), https://github.com/stormlightlabs/arclightning/pull/4
- d16567d (feat: add arcl workspace sync), https://github.com/stormlightlabs/arclightning/pull/7
- 389a853 (feat: add arcl workspace status), https://github.com/stormlightlabs/arclightning/pull/9
- 3acfc7e (feat: add arcl workspace resolve), https://github.com/stormlightlabs/arclightning/pull/10
- 5653898 (feat: sync the workspace around every command), https://github.com/stormlightlabs/arclightning/pull/11
- 8b5fbc7 (feat: track the roadmap in the workspace and test Git scenarios), https://github.com/stormlightlabs/arclightning/pull/12"""
+++

Synchronize one-sided database or workspace changes and provide explicit tools
for true divergence.

Blocked by: T08

Acceptance criteria:

- [x] Implement every state in the base/database/workspace comparison from
      `SPEC.md`
- [x] Reconcile before ordinary reads and writes, including recovery after an
      interrupted post-commit export
- [x] Refuse automatic writes when the database and workspace changed
      differently from their common base
- [x] Report record-level status, unmerged paths, conflict markers, and Git path
      state where Git exists
- [x] Require an explicit side for destructive recovery and never mutate Git
      refs, the index, remotes, or configuration
- [x] Track this repository's own remaining roadmap and tasks in its committed
      workspace

Verification:

- `cargo test --workspace --all-features workspace::sync`
- `cargo test --workspace --all-features workspace::reconcile`
- Exercise external edits, a branch switch, an unresolved merge, dual-side
  divergence, and interrupted export in disposable projects
