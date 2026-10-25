+++
id = "arcl-t-01M5NE5P6XHRR4Q84820YACQ07"
phase = "arcl-ph-01M5NE5P5RE8Y5Y5Y84T36FMEE"
title = "T18: Serve the local web app"
status = "completed"
priority = "normal"
position = 0
blocked-by = ["arcl-t-01M5NE5P2N1AMT731RJBAZ66YT", "arcl-t-01M5NH1N3R9AX3XZH6XG9ZX1QE"]
evidence = "Delivered by T18.1–T18.7: #35, #36, #39, #40, #41, #42, #43 and #PR."
+++

Serve the Inbox, Work, Planning, and Task workflow in the browser from the
`arcl` binary.

Blocked by: T11, T17

Delivered by subtasks T18.1–T18.7, as stacked PRs (Decision: Web app stack
and look). The PR for T18.7, the last of them, completes T18 as well
(Decision: Parent completion).

Acceptance criteria:

- [x] Add `arcl serve`, bound to `127.0.0.1` on an ephemeral port by default,
      with `--port` to pin one and the URL printed on start (T18.1)
- [x] Check the `Host` header and require the per-run token in a request
      header on every request, never in a cookie or in a query string after
      the first page load (T18.1)
- [x] Require a matching `Origin` on mutations, send no CORS headers, and
      never change state on GET (T18.1, T18.4)
- [x] Add a thin HTTP adapter over the Rust application layer with the same
      validation and errors as the CLI (T18.1, T18.4)
- [x] Build Inbox, Work, Planning, and Task views in `apps/web` from the shared
      UI package, with CodeMirror 6 for Markdown (T18.1, T18.2, T18.3, T18.6)
- [x] Show readiness reasons, blockers, handoff, and evidence on the Task view
      (T18.3)
- [x] Support keyboard-only use, narrow windows, light and dark themes, and
      reduced motion (T18.2, T18.5, T18.7)
- [x] Add Playwright workflow and screenshot coverage for the four views
      (T18.7)

Verification:

- `cargo test --workspace --all-features serve`
- `pnpm --filter @arclightning/web check`
- `pnpm --filter @arclightning/web test`
- `pnpm --filter @arclightning/web test:e2e` (T18.7)
- `pnpm --filter @arclightning/web test:visual` (T18.7)
- Complete capture -> task and capture -> spec -> plan -> task in the browser
  and confirm the results through the CLI (T18.6)
