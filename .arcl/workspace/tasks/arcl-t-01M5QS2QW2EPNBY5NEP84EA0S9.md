+++
id = "arcl-t-01M5QS2QW2EPNBY5NEP84EA0S9"
parent = "arcl-t-01M5NE5P6XHRR4Q84820YACQ07"
title = "T18.7: Fit, themes, and browser tests for the web app"
status = "completed"
priority = "normal"
position = 7
blocked-by = ["arcl-t-01M5QS2QSJ1S7D1XG5Z6HAY1G3"]
evidence = "Delivered by #44: light grey and blue and black and blue tokens in packages/ui, with a contrast test that checks text at 4.5:1 and borders, focus and accent at 3:1 in both themes; durations drop to 0 under reduced motion; below 48rem the sidebar becomes a top bar with a Menu button. apps/web/e2e adds a keyboard-only workflow against arcl serve on a scratch project, checked through the CLI; fit, theme and motion tests; and 16 screenshot baselines (four views, both themes, 1280 and 390 px) from fixed fixtures. Verified with pnpm check, pnpm test, test:browser, test:e2e, test:visual, cargo fmt, clippy, cargo test --workspace and cargo test --workspace --all-features serve."
+++

Finish the web app's fit and test coverage. This completes T18.

Scope:

- theme tokens in `packages/ui/src/lib/styles/tokens.css`: light grey and blue
  for light, black and blue for dark (Decision: Web app stack and look)
- responsive layout in `apps/web`
- Playwright workflow and screenshot tests in `apps/web/e2e/`, against
  `arcl serve` on a scratch project

Acceptance criteria:

- [x] Inbox, Work, Planning and Task work in a 390 px wide window without
      horizontal scrolling
- [x] Light and dark themes follow the system setting, and each meets WCAG AA
      contrast
- [x] Motion stops under `prefers-reduced-motion`
- [x] A keyboard-only Playwright run covers capture -> task and
      capture -> spec -> plan -> task, start, and complete
- [x] Screenshots cover the four views in both themes at desktop and narrow
      widths
- [x] This PR also completes T18, with the T18 subtask PRs as its evidence

Verification:

- `pnpm --filter @arclightning/web check`
- `pnpm --filter @arclightning/web test`
- `pnpm --filter @arclightning/web test:e2e`
- `pnpm --filter @arclightning/web test:visual`
- `cargo test --workspace --all-features serve`
- `just check`

Out of scope:

- New views or actions (T18.2 to T18.6)
