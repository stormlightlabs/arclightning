+++
id = "arcl-t-01M5VDDDFDTPS7XC8PCK9PNWV9"
phase = "arcl-ph-01M5VDD6PCB2B64MKW9TX01XXG"
title = "Add full and outline styles to arcl tree"
status = "completed"
priority = "normal"
position = 5
blocked-by = ["arcl-t-01M5VDDDD562QZYTSA0VJNWNTS"]
evidence = "Delivered by #62: arcl tree --style full unfolds every phase with short IDs aligned right, and --style outline prints STATUS, ID and TITLE columns with dot indentation and no box characters. Both follow --all, --ascii, --depth and the colour rules; folded, --plain and --json output are byte-identical to main on this repository."
+++

Add `--style full` and `--style outline` to `arcl tree`, beside the default
folded view.

`--style full` is the unfolded GNU-tree view with each record's short ID
right-aligned on its line. `--style outline` drops the box characters for a
STATUS, ID and TITLE column layout, indenting titles with dots, for narrow
terminals and `grep`.

```text
STATUS       ID    TITLE
open         W6XC  Roadmap
open         FMEE  . Milestone 7: Local web app
in_progress  5BW4  . . T24: Format apps/web and packages/ui with Prettier
```

Blocked by folding finished milestones, whose renderer and flags these
styles share.

Scope:

- `--style folded|full|outline`, defaulting to `folded`
- the full and outline renderers
- the same colour, `--ascii`, `--all` and `--depth` rules as the folded view

Acceptance criteria:

- [x] `--style full` shows every record unfolded, with short IDs on the
      right
- [x] `--style outline` prints STATUS, ID and TITLE columns with dot
      indentation and no box characters
- [x] Both styles follow the colour and `--ascii` rules
- [x] The manual documents `--style`
- [x] `--json` output is unchanged

Verification:

- snapshot tests for each style
- `arcl tree --style outline | grep in_progress` on this repository
- `just check`
