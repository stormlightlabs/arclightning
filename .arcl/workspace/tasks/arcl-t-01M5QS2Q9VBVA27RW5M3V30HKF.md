+++
id = "arcl-t-01M5QS2Q9VBVA27RW5M3V30HKF"
parent = "arcl-t-01M5NE5P6XHRR4Q84820YACQ07"
title = "T18.1: Serve a read-only API and the web app shell"
status = "completed"
priority = "normal"
position = 1
evidence = "Delivered on branch claude/t18-1-serve-shell: the arcl-serve crate, `arcl serve` and the apps/web shell. Verified with `cargo test --workspace --all-features serve`, `pnpm --filter @arclightning/web check`, `test`, `test:e2e`, `test:visual` and `just check`."
+++

Serve a read-only web app from the `arcl` binary, with the security checks in
place before any view exists (Decision: Local web app security), and an empty
`apps/web` shell to build the views in.

Scope:

- a `serve` command in `crates/arcl-cli/src/cli.rs` and
  `crates/arcl-cli/src/app/commands.rs`
- an `arcl-serve` crate in the workspace: the HTTP adapter, its read-only JSON
  endpoints over the application operations, and the built `apps/web` assets
  embedded in the binary
- `apps/web`, a SvelteKit app with `adapter-static` like `apps/desktop`,
  named `@arclightning/web`, that imports `@arclightning/ui` and its UnoCSS
  config from `packages/ui/uno.config.mjs`
- the Phosphor (`ph`), Bootstrap (`bi`) and Remix (`ri`) Iconify collections
  in the UnoCSS icons preset, and Google Sans and Inter in `packages/ui`
  (Decision: Web app stack and look)
- the command in `apps/website/src/content/docs/reference/manual.md`

Acceptance criteria:

- [x] `arcl serve` binds `127.0.0.1` on an ephemeral port by default, `--port`
      pins one, and it prints the URL on start
- [x] Every request needs a matching `Host` header and the per-run token in a
      request header; the token is never in a cookie, and the app drops it
      from the URL after the first page load
- [x] Responses carry no CORS headers, and no GET changes state
- [x] Read endpoints for records, lists, ready work, readiness explanation and
      context return the same data and errors as the CLI's JSON envelopes
- [x] `apps/web` builds and renders an empty shell with the Inbox, Work and
      Planning navigation from shared UI components
- [x] `apps/web` has `check`, `test`, `test:e2e` and `test:visual` scripts

Verification:

- `cargo test --workspace --all-features serve`: missing token, wrong token,
  wrong `Host`, token in a query string after the first load, and a read
  endpoint matching the CLI's JSON for the same record
- `pnpm --filter @arclightning/web check`
- `pnpm --filter @arclightning/web test`
- `just check`

Out of scope:

- Mutations and the `Origin` check (T18.4)
- Views (T18.2 onward)
