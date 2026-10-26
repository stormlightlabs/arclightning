+++
id = "arcl-ph-01M5VDD6PCB2B64MKW9TX01XXG"
plan = "arcl-pl-01M5NE5P0CWMJNP2A7B0ZQW6XC"
title = "Milestone 11: CLI formatting"
status = "open"
position = 11
+++

Make the CLI's human output readable at a glance: short record IDs, and an
`arcl tree` that shows status, folds finished work and puts the plan first.
JSON output keeps its shape.

Deliverables:

- short record IDs in human output, and as input to every command that
  takes a record ID
- status and readiness on each `arcl tree` line
- closed captures left out of `arcl tree` by default
- the plan first in `arcl tree`, with unplaced records after it
- a box-drawing tree that folds finished milestones
- full and outline styles for `arcl tree`

Exit criterion: `arcl tree` on this repository shows what is done, in
progress, ready and blocked in one screen, and any record can be named by its
short ID, while `--json` output is unchanged
