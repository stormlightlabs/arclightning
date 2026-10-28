+++
id = "arcl-t-01M5YRFBMHGP74F6CJSSG7FXP7"
phase = "arcl-ph-01M5YRFB35TZ51QW4AG1A8MS5B"
title = "Query records with GraphQL"
status = "pending"
priority = "normal"
position = 9
+++

Let agents and the web app ask for exactly the records and fields they need
in one request, as GitHub and Linear do. Taken from beans' `beans graphql`.

Add a read-only GraphQL schema over records: tasks, specs, plans, phases,
captures, notes and releases, with their relations, readiness, blockers,
gates and notes. It also covers what the `/api` reads return today: a
record's workspace source and revision, a task's context packet, the tree
and the work view. `arcl query '<graphql>'` runs a query against the local
database with no server. Mutations stay with the existing commands.

This task builds the schema and `arcl query`. Serving it is "Serve GraphQL
at /graphql in arcl serve", and the REST reads move onto it in "Build the
REST reads on GraphQL".

Scope:

- the schema in a shared crate, built on the existing read models, with
  `async-graphql`
- `arcl query` with a query string or `--file`, and `--variables`
- the schema printed by `arcl query --schema`, and documented
- a depth and complexity limit

Acceptance criteria:

- [ ] `arcl query` prints the GraphQL response (`data`, and `errors` when
  there are any) with only the requested fields
- [ ] Each `/api` read's data, including source, context, tree and work, can
  be queried
- [ ] Readiness, blockers and gates are queryable on tasks
- [ ] A query over the limit is refused with a GraphQL error whose
  `extensions.code` names the limit, and exit code 3
- [ ] `arcl query --schema` prints SDL that matches a checked-in copy

Verification:

- tests that run queries against a fixture project
- `just check`
