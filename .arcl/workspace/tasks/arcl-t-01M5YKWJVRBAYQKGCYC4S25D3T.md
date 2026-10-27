+++
id = "arcl-t-01M5YKWJVRBAYQKGCYC4S25D3T"
phase = "arcl-ph-01M5NE5P9FR8YZK0Z8YCV7YMZT"
title = "Prepare and tag v0.3.0"
status = "pending"
priority = "normal"
position = 2
+++

Prepare and tag `v0.3.0`, the first release built and published by dist. It
ships the CLI formatting milestone (short IDs, tree status marks, closed
captures out of the tree, plan first, folded milestones, and the full and
outline styles) and release binaries with installers, a Homebrew formula and
cargo-binstall support.

Scope:

- the project version at 0.3.0 in the workspace `Cargo.toml`, the
  `package.json` files of the root, `packages/ui`, `apps/web` and
  `apps/desktop`, and the desktop app's Tauri crate and config
- a `v0.3.0` release record listing the work it ships and this task
- the quick start's tagged install pointing at `v0.3.0`
- after merge, the tag through the Bump workflow on the merge commit, which
  runs the dist release

Acceptance criteria:

- [x] `arcl --version` prints `arcl 0.3.0`
- [x] The release record lists the work it ships
- [x] The quick start installs `--tag v0.3.0`
- [ ] `v0.3.0` is tagged on the merge commit, the GitHub Release has an
      archive and checksum for each target, and the release is completed

Verification:

- `cargo run -p arcl-cli -- --version`
- `dist plan --tag=v0.3.0`
- `arcl release show <id>`
- `just check`
