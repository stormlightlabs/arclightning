+++
id = "arcl-t-01M5NH23XH2AZMTH5QDVWSTVX2"
phase = "arcl-ph-01M5NH1M4357EDSS5J99NA8YY0"
title = "T08: Migrate repository-native files to the new model"
status = "completed"
priority = "normal"
position = 0
blocked-by = ["arcl-t-01M5NH23GE6N1SXWVK9BHVGP5V", "arcl-t-01M5NH23PWTRC8KBW9CA93AGPK"]
evidence = """
Delivered by:

- 3e5e902 (feat: deterministic snapshot import and export), committed to main
- f1820ab (refactor: CLI app -> mods), committed to main; made the connected model format version 1
- b6afb67 (refactor: rename snapshot to workspace), https://github.com/stormlightlabs/arclightning/pull/2"""
+++

Adapt the existing codec, import, and export work to the versioned workspace
layout in `SPEC.md`.

Blocked by: T06, T07

Acceptance criteria:

- [x] Define and document a new manifest version and record encoding for
      captures, specs, plans, tasks, and notes
- [x] Use the connected planning model as workspace format version 1
- [x] Render one deterministic file per record and preserve Markdown bodies
      through parse-render-parse round trips
- [x] Validate the complete candidate graph before mutating the database
- [x] Export changed records atomically and leave unchanged files byte-stable

Verification:

- [x] `cargo test --workspace --all-features workspace::codec`
- [x] `cargo test --workspace --all-features workspace::import`
- [x] `cargo test --workspace --all-features workspace::export`
