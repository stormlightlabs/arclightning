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
captures, notes and releases, with their relations, readiness and blockers.
`arcl query '<graphql>'` runs a query against the local database with no
server, and `arcl serve` exposes the same schema at `/graphql` behind its
token. Mutations stay with the existing commands for now.

Scope:

- the schema in a shared crate, built on the existing read models
- `arcl query` with a query string or `--file`, and `--variables`
- `/graphql` in `arcl serve`, with introspection
- the schema printed by `arcl query --schema`, and documented
- a depth or cost limit

Acceptance criteria:

- [ ] `arcl query` returns only the requested fields as JSON
- [ ] `/graphql` answers the same query with the same result
- [ ] Readiness and blockers are queryable on tasks
- [ ] Queries over the limit are refused with a clear error

Verification:

- tests that run queries against a fixture project through both paths
- `just check`
