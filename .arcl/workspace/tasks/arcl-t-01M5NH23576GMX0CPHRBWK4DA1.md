+++
id = "arcl-t-01M5NH23576GMX0CPHRBWK4DA1"
phase = "arcl-ph-01M5NH1M0T75BYF0YNKHZXMYY3"
title = "T04: Adapt task execution to flexible ancestry"
status = "completed"
priority = "normal"
position = 3
blocked-by = ["arcl-t-01M5NH22V9CABE81P6CPCEEQ4H", "arcl-t-01M5NH23068EY1VEVGFVXXWP1G"]
evidence = "Delivered by 1f09ff4 (feat: planning execution and readiness APIs), committed to main before pull requests."
+++

Preserve lifecycle, dependencies, readiness, context, handoff, and evidence
when phases and plans are optional.

Blocked by: T02, T03

Acceptance criteria:

- [x] Compute ready leaf tasks for every valid placement without assuming a
      milestone exists
- [x] Reject dependency cycles, parent cycles, cross-project links, and invalid
      container states before writing
- [x] Explain readiness and blocking in terms of the actual ancestry present
- [x] Include relevant spec, plan, phase, blocker, handoff, and evidence content
      in task context without returning unrelated records
- [x] Preserve atomic handoff, parking, completion, and evidence behavior

Verification:

- `cargo test --workspace --all-features ready`
- `cargo test --workspace --all-features context`
- Cover project-, spec-, plan-, phase-, and parent-task placement in tests
