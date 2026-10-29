+++
id = "arcl-t-01M61HW4VBTNF4G8PEJ6G996Z4"
phase = "arcl-ph-01M5YRFB35TZ51QW4AG1A8MS5B"
title = "Serve GraphQL at /graphql in arcl serve"
status = "in_progress"
priority = "normal"
position = 10
blocked-by = ["arcl-t-01M5YRFBMHGP74F6CJSSG7FXP7"]
+++

Serve the schema from "Query records with GraphQL" at `/graphql` in
`arcl serve`, so the web app and local tools can query it over HTTP.

Scope:

- `POST /graphql` with a JSON body of `query`, `variables` and
  `operationName`, behind the same `Host` and token checks as `/api`
- the request runs as a read: it never changes state, so it skips the
  mutation thread and the `Origin` check
- introspection on, and the same depth and complexity limit as `arcl query`
- the 1 MiB body limit and body deadline that mutations use
- a 405 for other methods on `/graphql` with `Allow: POST`

Acceptance criteria:

- [ ] `/graphql` answers a query with the same data as `arcl query`
- [ ] A request without the token or with a bad `Host` is refused as `/api` refuses it
- [ ] A query over the limit gets status 200 and the same GraphQL `errors`
  body that `arcl query` prints
- [ ] The manual and the server's request table list `/graphql`

Verification:

- serve tests that run the same queries through `arcl query` and `/graphql`
- `just check`
