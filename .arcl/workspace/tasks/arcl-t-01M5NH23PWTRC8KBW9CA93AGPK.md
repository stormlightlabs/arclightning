+++
id = "arcl-t-01M5NH23PWTRC8KBW9CA93AGPK"
phase = "arcl-ph-01M5NH1M2H5JPEW4DMNF5RMDQB"
title = "T07: Remove the superseded internal model"
status = "completed"
priority = "normal"
position = 1
blocked-by = ["arcl-t-01M5NH23A3PA9HHNWPFJA6SP5B", "arcl-t-01M5NH23GE6N1SXWVK9BHVGP5V"]
evidence = "Delivered by 853333d (refactor: migrate legacy model to connected records), committed to main before pull requests."
+++

Remove old storage and internal type paths after production callers use the
connected planning model.

Blocked by: T05, T06

Acceptance criteria:

- [x] Remove superseded tables or fields through a forward migration with
      upgrade coverage in this task
- [x] Remove duplicate domain paths after all production callers use the new
      records
- [x] Confirm upgraded projects retain all supported Markdown, relationships,
      handoffs, and evidence

Verification:

- `cargo test --workspace --all-features migrations`
- `cargo test --workspace --all-features`
