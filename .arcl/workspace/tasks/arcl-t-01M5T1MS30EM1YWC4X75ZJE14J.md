+++
id = "arcl-t-01M5T1MS30EM1YWC4X75ZJE14J"
phase = "arcl-ph-01M5NE5P5RE8Y5Y5Y84T36FMEE"
title = "Prepare and tag v0.2.0"
status = "in_progress"
priority = "normal"
position = 7
+++

Prepare and tag `v0.2.0`, which ships the claude.ai artifact build with
`arcl web export` and `arcl web apply` (T20), the web app polish and faster
tests (T23), the board view and raw record editing, and raw edits queued from
the artifact build.

Scope:

- the project version at 0.2.0 in the workspace `Cargo.toml`, the
  `package.json` files of the root, `packages/ui`, `apps/web` and
  `apps/desktop`, and the desktop app's Tauri crate and config
- a `v0.2.0` release record listing the work it ships and this task
- the quick start's tagged install pointing at `v0.2.0`
- the banner at the top of the README
- after merge, the tag through the Bump workflow on the merge commit

Acceptance criteria:

- [x] `arcl --version` prints `arcl 0.2.0`
- [x] The release record lists the work it ships
- [x] The quick start installs `--tag v0.2.0`
- [ ] `v0.2.0` is tagged on the merge commit, and the release is completed

Verification:

- `cargo run -p arcl-cli -- --version`
- `arcl release show <id>`
- `just check`
