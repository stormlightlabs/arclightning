+++
id = "arcl-t-01M5NH23GE6N1SXWVK9BHVGP5V"
phase = "arcl-ph-01M5NH1M2H5JPEW4DMNF5RMDQB"
title = "T06: Extract the shared Rust application"
status = "completed"
priority = "normal"
position = 0
blocked-by = ["arcl-t-01M5NH23576GMX0CPHRBWK4DA1", "arcl-t-01M5NH23A3PA9HHNWPFJA6SP5B"]
evidence = "Delivered by a8a0bf0 (build: split into cargo ws), committed to main before pull requests."
+++

Move the working connected planning operations behind core, store, repository,
and CLI crate boundaries without changing their behavior.

Blocked by: T04, T05

Acceptance criteria:

- [x] Create workspace crates for domain and application operations, SQLite,
      repository files, and the CLI
- [x] Keep domain and application crates independent of Clap, terminal output,
      Tauri, and MCP transports
- [x] Preserve the binary name, commands, exit behavior, and database
      compatibility
- [x] Move tests to the highest stable crate or CLI boundary without duplicating
      coverage

Verification:

- `cargo check --workspace --all-targets --all-features`
- `cargo test --workspace --all-features`
- Run the capture, planning, ready-work, context, and handoff CLI workflows
