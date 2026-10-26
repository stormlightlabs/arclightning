+++
id = "arcl-t-01M5VE2M08XRWY6G6PVZXDSXDN"
phase = "arcl-ph-01M5NE5P9FR8YZK0Z8YCV7YMZT"
title = "Publish release binaries with dist and support cargo-binstall"
status = "pending"
priority = "normal"
position = 1
+++

Publish prebuilt `arcl` binaries on each release, so a new user installs Arc
Lightning with one command instead of compiling it.

Add a cargo-dist (`dist`) config and the release workflow it generates. The
workflow runs on the version tag the Bump workflow pushes, builds `arcl` for
each target, and uploads archives with checksums to the GitHub Release for that
tag. `dist` also generates shell and PowerShell installers and a Homebrew
formula in a tap repository. `[package.metadata.binstall]` in `arcl-cli`
points `cargo binstall arcl-cli` at the same archives.

`arcl-serve` embeds `apps/web/build/index.html` through its `build.rs`, and
serves a page saying how to build the web app when that file is missing. Each
release build must run `pnpm install` and the web build before `cargo build`,
so every release binary carries the web app.

Scope:

- `dist` config in the workspace and its generated release workflow, run on
  the Bump workflow's version tags; pre-release tags such as
  `v0.1.0-alpha.1` publish a GitHub pre-release
- targets: Linux x86_64 and aarch64 (static musl), macOS arm64 and x86_64,
  and Windows x86_64
- archives and SHA-256 checksums on the GitHub Release
- shell and PowerShell installers, and a Homebrew tap
- the web build before each `cargo build` in the release workflow
- `[package.metadata.binstall]` in `crates/arcl-cli/Cargo.toml`
- the README's quick start leads with the one-line install and keeps
  `cargo install --git` as the fallback

Acceptance criteria:

- [ ] Pushing a version tag through the Bump workflow publishes a GitHub
      Release with an archive and checksum for each target
- [ ] `arcl serve` from each release binary serves the web app, not the
      build-it page
- [ ] The shell installer, the PowerShell installer and `brew install`
      each install a working `arcl`
- [ ] `cargo binstall arcl-cli` downloads the release binary instead of
      compiling
- [ ] The README's quick start starts with the one-line install and keeps
      `cargo install --git` as the fallback

Verification:

- `dist plan` and `dist build` locally for the host target, then
  `arcl --version` and `arcl serve` from the built archive
- a pre-release tag through the Bump workflow, then each installer and
  `cargo binstall` against that release on Linux, macOS and Windows
- `just check`

Notes:

- Out of scope: publishing to crates.io. `cargo binstall` resolves versions
  best from crates.io; revisit when the crates settle.
- Fabrial sessions could install `arcl` from the release instead of
  compiling it.
