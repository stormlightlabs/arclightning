+++
id = "arcl-t-01M75WSPEX0F7MT8E7GPT3FTNE"
phase = "arcl-ph-01M5NE5P9FR8YZK0Z8YCV7YMZT"
title = "Prepare and tag v0.4.1"
status = "completed"
priority = "normal"
position = 3
evidence = "Tagged v0.4.1 on 773432c (#101). Release run 43244581346 built all five targets, including aarch64-apple-darwin on macos-15, and published arcl and arcl-mcp archives with SHA-256 checksums, shell and PowerShell installers, and arcl.rb at 0.4.1 in stormlightlabs/homebrew-tap. Checked on Linux x86_64: the shell installer installs arcl 0.4.1, the musl archive matches its checksum, and arcl serve returns the 2.8 MB web app. The v0.4.0 tag on 815f012 has no release."
completed-at = "2026-11-12T15:30:31.653Z"
+++

Prepare and tag `v0.4.1`. The `v0.4.0` tag has no release, because its Apple
Silicon build, queued for a macos-14 runner, never got one. It ships the agent
workflow milestone, the MCP list filters and `prime` tool, GraphQL and REST
reads with `arcl serve --api`, the hosted serve flags, and the website.

Do not re-run the v0.4.0 Release run (actions run 43159277636). Its failed
jobs would publish a v0.4.0 GitHub Release, mark it Latest and push
`arcl 0.4.0` over the tap's newer formula.

Scope:

- the project version at 0.4.1 in the workspace `Cargo.toml`, the
  `package.json` files of the root, `packages/ui`, `apps/web` and
  `apps/desktop`, and the desktop app's Tauri crate and config
- dist building `aarch64-apple-darwin` on macos-15
- a `v0.4.1` release record listing the work it ships and this task
- after merge, the tag through the Bump workflow on the merge commit, which
  runs the dist release

The website's install and upgrade lines that name 0.3.0 or 0.4.0 change in the
documentation pull request, not here.

Acceptance criteria:

- [x] `arcl --version` prints `arcl 0.4.1`
- [x] The release record lists the work it ships
- [x] `v0.4.1` is tagged on the merge commit, the GitHub Release has an
      archive and checksum for each target, and the release is completed

Verification:

- `cargo run -p arcl-cli -- --version`
- `dist plan --tag=v0.4.1`
- `arcl release show <id>`
- `just check`
