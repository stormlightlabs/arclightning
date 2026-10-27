+++
id = "arcl-t-01M5VDDDAS9P34RVKCAGHXRSFH"
phase = "arcl-ph-01M5VDD6PCB2B64MKW9TX01XXG"
title = "Put the plan first in arcl tree"
status = "completed"
priority = "normal"
position = 3
evidence = "Delivered by #60: specs print first in arcl tree, with captures, notes and releases under Inbox, Notes and Releases headings."
+++

Put the plan first in `arcl tree`. Unplaced captures, notes and releases
print flush-left above the spec and plan hierarchy, so on this repository the
plan starts about 50 lines down.

Print specs and their plans first, then the unplaced records under labeled
sections: Inbox for captures, Notes and Releases. Each section is a heading
line with its records indented below it.

Scope:

- order human and `--plain` output with specs first and unplaced records
  after
- section headings for the unplaced records in human output
- `--json` keeps its node shape; if its order changes, the manual says so,
  and if the shape must change, bump `format_version`

Acceptance criteria:

- [x] The first line of `arcl tree` is a spec
- [x] Captures, notes and releases follow under Inbox, Notes and Releases
      headings
- [x] `--json` output keeps its shape

Verification:

- CLI tests for the order and the headings
- `arcl tree` on this repository
- `just check`
