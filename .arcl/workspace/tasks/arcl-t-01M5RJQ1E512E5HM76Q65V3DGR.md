+++
id = "arcl-t-01M5RJQ1E512E5HM76Q65V3DGR"
phase = "arcl-ph-01M5NE5P5RE8Y5Y5Y84T36FMEE"
title = "T21: Serve the web app with axum"
status = "completed"
priority = "high"
position = 0
evidence = "Delivered by #45: arcl-serve runs axum on hyper and tokio in place of tiny_http. A declared Content-Length over 1 MiB gets 413 before any check, with or without the token, and the server keeps serving; on main the same request aborted arcl serve. A stalled body gets 408 after 5 s, request heads time out after 5 s, at most 128 connections are open, and Ctrl-C shuts down with a 5 s grace. Reads and mutations stay sync on their own threads. Verified with cargo fmt, clippy, cargo test --workspace, cargo test --workspace --all-features serve, pnpm check, pnpm test, web test:e2e and a live run against arcl serve."
+++

Move `crates/arcl-serve` from tiny_http to axum on hyper and tokio. tiny_http
allocates a body's declared length when it drains it, so one request with
`Content-Length: 1000000000000000`, with or without the token, aborts
`arcl serve`. It also hands new connections to busy threads and has no read
timeouts.

Acceptance criteria:

- [ ] A declared `Content-Length` over 1 MiB gets 413 before any of the body
      is read, with or without the token, and the server keeps serving
- [ ] A body that stalls gets 408 after 5 seconds while reads stay fast
- [ ] Stalled connections do not hold up a new authorized GET; header reads
      time out and connections are capped
- [ ] Ctrl-C shuts the server down gracefully
- [ ] Token, Host, method and Origin checks, CSP, status mapping and the
      one-at-a-time mutation worker behave as before; `Reads` and `Mutations`
      stay sync
- [ ] The manual's Web app section and the module docs describe the limits

Verification:

- `cargo test --workspace --all-features serve`
- `pnpm --filter @arclightning/web test:e2e`
- `just check`
- a request with a huge `Content-Length` against `arcl serve` aborts it on
  `main` and gets 413 on this branch
