+++
id = "arcl-t-01M5NH23A3PA9HHNWPFJA6SP5B"
phase = "arcl-ph-01M5NH1M0T75BYF0YNKHZXMYY3"
title = "T05: Move the CLI to the new vocabulary"
status = "completed"
priority = "normal"
position = 4
blocked-by = ["arcl-t-01M5NH23068EY1VEVGFVXXWP1G", "arcl-t-01M5NH23576GMX0CPHRBWK4DA1"]
evidence = "Delivered by d1e8dbf (refactor: migrate CLI to connected capture/spec/plan model), committed to main before pull requests."
+++

Expose the connected planning model through `arcl` and replace the old command
nouns.

Blocked by: T03, T04

Acceptance criteria:

- [x] Add coherent `capture`, `spec`, `plan`, `task`, and `note` command groups
- [x] Support stable JSON for every new read operation and structured errors for
      expected failures
- [x] Remove the `idea`, `epic`, and `milestone` command groups when their
      replacements ship and do not add compatibility aliases
- [x] Update help examples so a user can complete capture -> spec -> plan ->
      task and capture -> task workflows
- [x] Keep primary results on stdout and diagnostics on stderr without ANSI
      escapes in machine output

Verification:

- `cargo test --workspace --all-features cli`
- Run both primary workflows using human and JSON output
