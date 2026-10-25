+++
id = "arcl-t-01M5STHSRRT2ADFN3Y9YAC2N7A"
phase = "arcl-ph-01M5NE5P5RE8Y5Y5Y84T36FMEE"
title = "Add a board view and raw record editing to the web app"
status = "completed"
priority = "normal"
position = 5
blocked-by = ["arcl-t-01M5S25XRNG2F9WBXYQQY8E628"]
evidence = "Delivered by #PR: the Work page switches between list and board with a toggle or `v`, remembered in local storage and named by `?view=board`. The board has Pending, In progress, Parked, Completed and Cancelled columns of cards with ID, title, readiness and priority, under the existing filters. Drops and the Move dialog (`m` on the selected card) run start, park, unpark or complete; any other move is refused with a message and the card stays. The Task view's Edit file opens the record file in CodeMirror with TOML front matter, saved through `POST /api/records/{id}/edit`, which checks it with the workspace import's decode and graph validation under the sync lock and refuses a stale SHA-256 revision with 409. The artifact build hides Edit file. The README shows two rounded dark screenshots. Verified with cargo fmt, clippy, cargo test --workspace, pnpm check, pnpm test, both test:browser suites, web build, build:artifact, test:e2e and arcl check."
+++

Add a board to the Work page and raw record editing to the Task view, from
the owner's feedback on the web app, and refresh the README screenshots.

Blocked by: Polish the web app and speed up its tests

Scope:

- `apps/web`: the Work board, the raw record editor, the screenshots script
  and the e2e tests
- `packages/ui`: a CodeMirror editor for a record file, with TOML for the
  front matter and Markdown for the body
- `crates/arcl-serve` and `crates/arcl-cli`: endpoints that read and save a
  record's file
- `crates/arcl-repo`: saving one record file through the import's checks
- the README images and the manual

Acceptance criteria:

- [x] The Work page switches between list and board, remembers the choice
      per viewer and reads it from `?view=board`
- [x] The board has one column per task status, in workflow order, and its
      cards show ID, title, readiness and priority and link to the Task view
- [x] The board follows the Status, Readiness, Priority and Milestone filters
- [x] Dropping a card on another column runs start, park, unpark or
      complete when that transition is allowed, and refuses any other move
      with a message while the card stays in its column
- [x] A focused card has a keyboard move menu, so moving needs no drag
- [x] The board scrolls each column with many cards and scrolls sideways
      at narrow widths
- [x] In the artifact build, a move queues like other edits
- [x] The Task view edits the record's whole file, TOML front matter and
      Markdown body, in CodeMirror
- [x] Saving goes through `arcl serve` under the sync lock and the request
      checks, with the import's parse, schema and relationship rules, and a
      stale revision is refused; a refused save leaves the file unchanged
- [x] The artifact build hides the raw editor, and the docs say so
- [x] The README shows two dark desktop screenshots with rounded corners,
      the Work board and the Task view, and the other images are gone

Verification:

- `cargo test --workspace`, with tests for the new endpoints
- `pnpm check` and `pnpm test`
- `pnpm --filter @arclightning/web test:browser`
- `pnpm --filter @arclightning/web test:e2e`, with a board move and raw
  edits against `arcl serve`
- `just check`

Out of scope:

- Queueing raw edits from the artifact build
- Reordering cards inside a column
