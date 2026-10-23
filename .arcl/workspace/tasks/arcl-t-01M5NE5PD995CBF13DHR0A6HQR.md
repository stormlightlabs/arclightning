+++
id = "arcl-t-01M5NE5PD995CBF13DHR0A6HQR"
phase = "arcl-ph-01M5NE5PBCPG6PJ4342DAJ9FZ0"
title = "T13: Add desktop work, planning, and task views"
status = "pending"
priority = "normal"
position = 1
blocked-by = ["arcl-t-01M5NE5PC91T2G9ZQRVP19AGNY", "arcl-t-01M5NH23576GMX0CPHRBWK4DA1"]
+++

Complete the graphical planning and execution workflow over the shared project
model.

Blocked by: T04, T12

Acceptance criteria:

- [ ] Show backlog, ready, in-progress, and parked work with readiness reasons
- [ ] Navigate and edit specs, plans, optional phases, tasks, and notes in the
      Planning view
- [ ] Use CodeMirror 6 for Markdown editing with source-mode keyboard and
      accessibility behavior covered by component tests
- [ ] Show task ancestry, blockers, Markdown description, handoff, and evidence
      on the Task view
- [ ] Support start, park, handoff, unpark, complete, and cancel transitions with
      the same validation as the CLI
- [ ] Preserve selection and unsaved-edit safety during navigation and external
      project changes
- [ ] Support keyboard-only use, narrow windows, light and dark themes, and
      reduced motion
- [ ] Add Playwright screenshot coverage for Inbox, Work, Planning, and Task at
      fixed desktop and narrow viewports in light and dark themes

Verification:

- `pnpm --filter @arclightning/desktop check`
- `pnpm --filter @arclightning/desktop test`
- `pnpm --filter @arclightning/desktop test:browser`
- `pnpm --filter @arclightning/desktop test:e2e`
- `pnpm --filter @arclightning/desktop test:visual`
- Run Playwright flows for planning, blockers, handoff, and completion
