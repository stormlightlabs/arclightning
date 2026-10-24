+++
id = "arcl-t-01M5QS2QJPGMYK0NE2MDRXN7QV"
parent = "arcl-t-01M5NE5P6XHRR4Q84820YACQ07"
title = "T18.4: Add task mutations to the web app"
status = "completed"
priority = "normal"
position = 4
blocked-by = ["arcl-t-01M5QS2QFHS2NWM2CTPQHZ8JTW"]
evidence = "Delivered by #41: POST endpoints in crates/arcl-serve for task create, update, start, park, unpark, handoff and complete, run through the CLI's task commands with its write sync, plus the Origin check; DataSource mutation methods in apps/web/src/lib/data; New task in the Work view and the start, complete, hand off, park, unpark and edit actions in the Task view. Verified with cargo test --workspace --all-features serve, pnpm --filter @arclightning/web check and test, the just check steps, and a headless Chromium run against arcl serve on a scratch clone, confirmed with arcl show."
+++

Let the web app change records, through the same application operations and
errors as the CLI.

Scope:

- mutation endpoints in `crates/arcl-serve`
- start, complete, park, create and edit actions in the Work and Task views,
  with the CodeMirror 6 editor writable for the body
- `apps/web` state that refreshes the affected records after a mutation

Acceptance criteria:

- [x] Endpoints start, complete, park, create and edit tasks, all as POST
- [x] Mutations need an `Origin` that matches the served origin, on top of the
      token and `Host` checks
- [x] A failed mutation returns the CLI's error code and message, and the view
      shows them next to the action
- [x] After completion the view lists the tasks the completion unblocked
- [x] Edits keep the title and body the CLI validates, and reject what it
      rejects

Verification:

- `cargo test --workspace --all-features serve`: each mutation, a missing and
  a foreign `Origin`, an invalid transition, and the `unblocked` list
- `pnpm --filter @arclightning/web check`
- `pnpm --filter @arclightning/web test`
- `just check`

Out of scope:

- Capture, spec and plan mutations (T18.5, T18.6)
