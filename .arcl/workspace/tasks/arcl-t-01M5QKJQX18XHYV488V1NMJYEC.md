+++
id = "arcl-t-01M5QKJQX18XHYV488V1NMJYEC"
phase = "arcl-ph-01M5NE5P3EMQJC9Z5CNYZXVHBW"
title = "T19: Tag a pre-release fabrial can pin"
status = "pending"
priority = "normal"
position = 1
blocked-by = ["arcl-t-01M5NE5P2N1AMT731RJBAZ66YT"]
+++

Tag a pre-release that fabrial can pin, so its cloud sessions install a known
`arcl` instead of building from `main`. Fabrial asked for this once the
milestone 4 JSON shapes settled.

Scope:

- tag `v0.1.0-alpha.1` on the `main` commit that completes milestone 4, and
  record it with `arcl release`
- install steps for cloud sessions in
  `apps/website/src/content/docs/quick-start.md`:
  `cargo install --git https://github.com/stormlightlabs/arclightning --tag v0.1.0-alpha.1 arcl-cli`,
  which installs the `arcl` binary

Acceptance criteria:

- [ ] The tag exists on `main` and the install command above builds `arcl`
      from a clean environment
- [ ] The quick start shows the tagged install next to `cargo install --path .`
- [ ] Fabrial is told the tag name

Verification:

- run the install command in a fresh container and `arcl --version`
- prettier on the changed docs
