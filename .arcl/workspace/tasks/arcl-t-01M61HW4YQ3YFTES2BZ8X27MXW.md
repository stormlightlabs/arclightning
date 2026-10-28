+++
id = "arcl-t-01M61HW4YQ3YFTES2BZ8X27MXW"
phase = "arcl-ph-01M5YRFB35TZ51QW4AG1A8MS5B"
title = "Build the REST reads on GraphQL"
status = "pending"
priority = "normal"
position = 11
blocked-by = ["arcl-t-01M61HW4VBTNF4G8PEJ6G996Z4"]
+++

Make GraphQL the one read path in `arcl serve`. Each `GET /api/...` read runs
a named query against the schema and maps the result into the envelope it
returns today, so the web app and scripts see no change and a field added to
the schema reaches REST without a second code path.

Scope:

- a named query for each read: records, record, source, ready, explain,
  context, tree and work
- a mapping from each result to the current envelope, with the same
  `format_version`
- the read code in `arcl-serve` that the queries replace, removed
- `arcl web export`, which uses the same reads, moved onto the queries
- mutations stay as they are

Acceptance criteria:

- [ ] Every `/api` read returns the same bytes as before for a fixture project
- [ ] The web app's browser tests pass unchanged
- [ ] `arcl web export` writes the same documents for a fixture project
- [ ] The `Reads` trait is gone or answers only through the schema

Verification:

- golden tests that record each read's envelope before the change and
  compare after
- `just check`
