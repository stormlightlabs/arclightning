+++
id = "arcl-t-01M5NE5PADMZW0JNKCPHJHEGMS"
phase = "arcl-ph-01M5NE5P9FR8YZK0Z8YCV7YMZT"
title = "T15: Verify the integrated release"
status = "pending"
priority = "normal"
position = 0
blocked-by = ["arcl-t-01M5NE5P2N1AMT731RJBAZ66YT", "arcl-t-01M5NE5P4D72Y56E0Y2JDE535B", "arcl-t-01M5NE5P6XHRR4Q84820YACQ07", "arcl-t-01M5NE5P8KF13P2YZ4B8W2B7MS", "arcl-t-01M5NH23PWTRC8KBW9CA93AGPK", "arcl-t-01M5NH242P4SSTMJT0T6R62Q21"]
+++

Close gaps in the specified workflows and prepare one coherent release across
the CLI, local web app, MCP server, website, and repository-native mode.

Blocked by: T07, T09, T10, T14, T17, T18

Acceptance criteria:

- [ ] Run one project through capture, specification, planning, ready work,
      execution, handoff, completion, repository synchronization, and conflict
      recovery across the supported interfaces
- [ ] Verify equivalent operations return the same semantic result through CLI,
      web app, and MCP adapters
- [ ] Document installation, project migration, web app use, CLI automation,
      MCP, repository-native collaboration, and recovery
- [ ] Map every first-release success criterion in `SPEC.md` to automated
      evidence or a named human review
- [ ] Review and approve intentional Playwright screenshot changes before
      updating baselines
- [ ] Pass all formatting, lint, type, test, build, and documentation checks from
      a clean checkout

Verification:

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --all-features`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- `pnpm -r check`
- `pnpm -r test`
- `pnpm -r build`
