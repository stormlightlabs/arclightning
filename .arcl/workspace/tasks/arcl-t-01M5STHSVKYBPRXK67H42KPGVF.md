+++
id = "arcl-t-01M5STHSVKYBPRXK67H42KPGVF"
phase = "arcl-ph-01M5NE5P5RE8Y5Y5Y84T36FMEE"
title = "Queue raw record edits from the artifact build"
status = "completed"
priority = "normal"
position = 6
blocked-by = ["arcl-t-01M5STHSRRT2ADFN3Y9YAC2N7A"]
evidence = "Delivered by #PR: the artifact build queues raw record edits as `update_source`, which `arcl web apply` runs as `POST /api/records/<id>/edit`; `arcl web export` writes `sources/<id>`."
+++

Let the artifact build queue a raw record edit, so a person on claude.ai can
edit a record's file as they can under `arcl serve`. The artifact build
hides the raw editor because the page cannot write to the repository.

Blocked by: Add a board view and raw record editing to the web app

Scope:

- `apps/web`: the raw editor in the artifact build and the queue op
- `crates/arcl-serve`: an `update_source` operation for `arcl web apply`
- the manual and `apps/web/README.md`

Acceptance criteria:

- [x] The artifact build shows the raw editor and queues a save as an
      `update_source` op carrying the whole file and the revision it was
      based on
- [x] `arcl web apply` runs the op through the same validation and
      revision check as `POST /api/records/{id}/edit`, and refuses a stale
      revision with the server's error
- [x] The page refuses at queue time a file whose queue entry would pass
      the queue's 200 KiB entry limit, and says why
- [x] The manual describes queued raw edits

Verification:

- `cargo test --workspace`, with tests for the apply path
- `pnpm check` and `pnpm test`
- `pnpm --filter @arclightning/web test:e2e`, with a queued raw edit
  applied by `arcl web apply`
- `just check`
