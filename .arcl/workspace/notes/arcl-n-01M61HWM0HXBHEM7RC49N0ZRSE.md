+++
id = "arcl-n-01M61HWM0HXBHEM7RC49N0ZRSE"
title = "Decision: GraphQL is the read path of arcl serve"

[[links]]
kind = "task"
id = "arcl-t-01M5YRFBMHGP74F6CJSSG7FXP7"

[[links]]
kind = "task"
id = "arcl-t-01M61HW4VBTNF4G8PEJ6G996Z4"

[[links]]
kind = "task"
id = "arcl-t-01M61HW4YQ3YFTES2BZ8X27MXW"

[[links]]
kind = "task"
id = "arcl-t-01M61HW51Z6X4YHYAB35AYZDQ8"
+++

`arcl serve` reads through GraphQL. One read-only schema, built on the
existing read models, answers `arcl query`, `/graphql` and every `GET /api`
read. Each REST read is a named query mapped into the envelope it returns
today. Writes stay REST and call the commands' code.

- Local only. The server binds `127.0.0.1` and checks `Host` and the token on
  every request.
- `/graphql` takes POST. The schema has no mutations, so POST there is a read
  and needs no `Origin`.
- `arcl serve --api` serves `/graphql` and `/api` without the page.
  Scripts get the token from `ARCL_SERVE_TOKEN` or the `--json` startup line.
- Writes need a matching `Origin` only when one is sent. The token header
  already blocks other pages.
- REST envelopes keep their `format_version` rules. A field added to the
  schema can appear in an envelope; renaming or removing one changes the
  version.
- `async-graphql`, for its axum support, depth and complexity limits and SDL
  output.

Decided 2026-10-28.
