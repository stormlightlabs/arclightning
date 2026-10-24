+++
id = "arcl-t-01M5QS2QW2EPNBY5NEP84EA0S9"
parent = "arcl-t-01M5NE5P6XHRR4Q84820YACQ07"
title = "T18.7: Fit, themes, and browser tests for the web app"
status = "pending"
priority = "normal"
position = 7
blocked-by = ["arcl-t-01M5QS2QSJ1S7D1XG5Z6HAY1G3"]
+++

Finish the web app's fit and test coverage. This completes T18.

Scope:

- theme tokens in `packages/ui/src/lib/styles/tokens.css`: light grey and blue
  for light, black and blue for dark (Decision: Web app stack and look)
- responsive layout in `apps/web`
- Playwright workflow and screenshot tests in `apps/web/e2e/`, against
  `arcl serve` on a scratch project

Acceptance criteria:

- [ ] Inbox, Work, Planning and Task work in a 390 px wide window without
      horizontal scrolling
- [ ] Light and dark themes follow the system setting, and each meets WCAG AA
      contrast
- [ ] Motion stops under `prefers-reduced-motion`
- [ ] A keyboard-only Playwright run covers capture -> task and
      capture -> spec -> plan -> task, start, and complete
- [ ] Screenshots cover the four views in both themes at desktop and narrow
      widths
- [ ] This PR also completes T18, with the T18 subtask PRs as its evidence

Verification:

- `pnpm --filter @arclightning/web check`
- `pnpm --filter @arclightning/web test`
- `pnpm --filter @arclightning/web test:e2e`
- `pnpm --filter @arclightning/web test:visual`
- `cargo test --workspace --all-features serve`
- `just check`

Out of scope:

- New views or actions (T18.2 to T18.6)
