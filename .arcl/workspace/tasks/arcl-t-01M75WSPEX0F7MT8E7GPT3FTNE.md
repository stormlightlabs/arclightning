+++
id = "arcl-t-01M75WSPEX0F7MT8E7GPT3FTNE"
phase = "arcl-ph-01M5NE5P9FR8YZK0Z8YCV7YMZT"
title = "Prepare and tag v0.4.0"
status = "pending"
priority = "normal"
position = 3
+++

Prepare and tag `v0.4.0`. It ships the agent workflow milestone, the MCP list
filters and `prime` tool, GraphQL and REST reads with `arcl serve --api`, the
hosted serve flags, and the website.

Scope:

- the project version at 0.4.0 in the workspace `Cargo.toml`, the
  `package.json` files of the root, `packages/ui`, `apps/web` and
  `apps/desktop`, and the desktop app's Tauri crate and config
- a `v0.4.0` release record listing the work it ships and this task
- after merge, the tag through the Bump workflow on the merge commit, which
  runs the dist release

The website's install and upgrade lines that name 0.3.0 change in the
documentation pull request, not here.

Acceptance criteria:

- [x] `arcl --version` prints `arcl 0.4.0`
- [x] The release record lists the work it ships
- [ ] `v0.4.0` is tagged on the merge commit, the GitHub Release has an
      archive and checksum for each target, and the release is completed

Verification:

- `cargo run -p arcl-cli -- --version`
- `dist plan --tag=v0.4.0`
- `arcl release show <id>`
- `just check`
