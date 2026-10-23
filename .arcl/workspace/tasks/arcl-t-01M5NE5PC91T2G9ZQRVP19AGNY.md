+++
id = "arcl-t-01M5NE5PC91T2G9ZQRVP19AGNY"
phase = "arcl-ph-01M5NE5PBCPG6PJ4342DAJ9FZ0"
title = "T12: Build the desktop shell and project inbox"
status = "pending"
priority = "normal"
position = 0
blocked-by = ["arcl-t-01M5NE5P6XHRR4Q84820YACQ07", "arcl-t-01M5NE5PADMZW0JNKCPHJHEGMS"]
+++

After the first release, create the Tauri application, connect it to Rust
application operations, and deliver project selection plus capture triage. Reuse
the web app's views where they apply.

Blocked by: T15, T18

Acceptance criteria:

- [ ] Open, create, remember, and switch between Arc Lightning projects
- [ ] Use thin Tauri commands over the Rust application layer
- [ ] List, create, edit, promote, and discard captures in the Inbox view
- [ ] Show actionable validation and storage errors without exposing internal
      traces
- [ ] Match CLI results for the same project mutations
- [ ] Provide a deterministic browser test adapter for desktop Playwright tests
      without duplicating domain behavior
- [ ] Cover the thin Tauri command boundary with Rust integration tests

Verification:

- `cargo test --workspace --all-features -p arcl-desktop`
- `pnpm --filter @arclightning/desktop check`
- `pnpm --filter @arclightning/desktop test`
- `pnpm --filter @arclightning/desktop test:browser`
- `pnpm --filter @arclightning/desktop test:e2e`
- Complete capture -> task and capture -> spec flows through Playwright
