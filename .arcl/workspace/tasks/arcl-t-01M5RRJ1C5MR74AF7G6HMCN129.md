+++
id = "arcl-t-01M5RRJ1C5MR74AF7G6HMCN129"
phase = "arcl-ph-01M5NE5P3EMQJC9Z5CNYZXVHBW"
title = "T22: Update fabrial and tag v0.1.0"
status = "completed"
priority = "normal"
position = 2
evidence = "Delivered by #46: fabrial 0.3.3 with no drift, release v0.1.0 (arcl-r-01M5RRJ1FC0B5XAHJXPGCYRVPA) listing T10, T18, T21 and T22, and the quick start's install at --tag v0.1.0. The tag follows the merge."
+++

Update the repository to fabrial 0.3.3 and tag `v0.1.0`, the first release
after the `v0.1.0-alpha.1` pre-release. It ships the MCP server and skill
(T10), the local web app (T18) and its axum server (T21).

Scope:

- `fab update` to fabrial 0.3.3
- a `v0.1.0` release record listing T10, T18 and T21
- the quick start's tagged install pointing at `v0.1.0`
- after merge, the tag through the Bump workflow on the merge commit

Acceptance criteria:

- [x] `fab check` reports fabrial 0.3.3 with no drift
- [x] The release record lists the work it ships
- [x] The quick start installs `--tag v0.1.0`

Verification:

- `fab check`
- `arcl release show <id>`
- `just check`
