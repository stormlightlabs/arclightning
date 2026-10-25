+++
id = "arcl-t-01M5S25XRNG2F9WBXYQQY8E628"
phase = "arcl-ph-01M5NE5P5RE8Y5Y5Y84T36FMEE"
title = "T23: Polish the web app and speed up its tests"
status = "completed"
priority = "normal"
position = 2
blocked-by = ["arcl-t-01M5QXMQFGA7QPRKNWPCBVN21N"]
evidence = "Delivered by #PR: the readiness callout has a 1 px border, the properties header rounds with its panel, controls use a 6 px `--arcl-radius-control`, and Markdown task list items render as disabled checkboxes through micromark-extension-gfm-task-list-item. The sidebar collapses to an icon rail with a button or `[`, stored in local storage. The Inbox list is a packages/ui DataTable (title, and created read from the ULID in the ID) beside the selected capture from 40rem of Inbox width, stacked below that. `test:e2e` builds through apps/web/scripts/e2e-build.mjs, which skips unchanged web builds and runs the two at once; a repeat run went from 42.9 s to about 20 s, a run with both builds from 48.5 s to 34.9 s. Verified with cargo fmt, clippy, cargo test --workspace, pnpm check, pnpm test, both test:browser suites, web build, build:artifact, test:e2e, test:visual and arcl check."
+++

Fix the web app issues the owner found in the dark theme, and shorten the
web test loop, which made the last UI round slow to verify.

Blocked by: T20

Scope:

- `packages/ui`: Readiness, PropertiesPanel, Markdown, the control radius
  token, and a DataTable component
- `apps/web`: a collapsible sidebar and the Inbox layout
- the web test scripts, Playwright config and test docs

Acceptance criteria:

- [x] The readiness callout has a 1 px border on every side and keeps its
      tinted background
- [x] The properties panel header's hover background stays inside the
      panel's rounded corners
- [x] A button and a shortcut collapse the sidebar, the choice survives a
      reload, and narrow windows keep the top bar and Menu
- [x] The Inbox list fits its container and scrolls at the container's
      edge
- [x] The Inbox shows the capture detail beside the list on wide windows
      and below it on narrow ones
- [x] The Inbox list is a table with title and created columns, and j, k
      and the triage keys still work
- [x] Buttons, select menu triggers and other controls use a 6 px radius;
      badges stay round
- [x] Markdown task list items render as read-only checkboxes without a
      bullet, with HTML still escaped
- [x] A repeat `pnpm test:e2e` with unchanged inputs skips the web builds,
      and `apps/web/README.md` documents the fast test loop

Verification:

- `pnpm check` and `pnpm test`
- `pnpm --filter @arclightning/web test:browser` and
  `pnpm --filter @arclightning/ui test:browser`
- `pnpm --filter @arclightning/web test:e2e`
- `just check`

Out of scope:

- New fields in the captures listing, such as a body excerpt
