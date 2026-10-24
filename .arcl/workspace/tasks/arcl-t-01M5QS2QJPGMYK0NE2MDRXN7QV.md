+++
id = "arcl-t-01M5QS2QJPGMYK0NE2MDRXN7QV"
parent = "arcl-t-01M5NE5P6XHRR4Q84820YACQ07"
title = "T18.4: Add task mutations to the web app"
status = "pending"
priority = "normal"
position = 4
blocked-by = ["arcl-t-01M5QS2QFHS2NWM2CTPQHZ8JTW"]
+++

Let the web app change records, through the same application operations and
errors as the CLI.

Scope:

- mutation endpoints in `crates/arcl-serve`
- start, complete, park, create and edit actions in the Work and Task views,
  with the CodeMirror 6 editor writable for the body
- `apps/web` state that refreshes the affected records after a mutation

Acceptance criteria:

- [ ] Endpoints start, complete, park, create and edit tasks, all as POST
- [ ] Mutations need an `Origin` that matches the served origin, on top of the
      token and `Host` checks
- [ ] A failed mutation returns the CLI's error code and message, and the view
      shows them next to the action
- [ ] After completion the view lists the tasks the completion unblocked
- [ ] Edits keep the title and body the CLI validates, and reject what it
      rejects

Verification:

- `cargo test --workspace --all-features serve`: each mutation, a missing and
  a foreign `Origin`, an invalid transition, and the `unblocked` list
- `pnpm --filter @arclightning/web check`
- `pnpm --filter @arclightning/web test`
- `just check`

Out of scope:

- Capture, spec and plan mutations (T18.5, T18.6)
