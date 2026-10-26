+++
id = "arcl-t-01M5VDDDD562QZYTSA0VJNWNTS"
phase = "arcl-ph-01M5VDD6PCB2B64MKW9TX01XXG"
title = "Fold finished milestones in arcl tree"
status = "pending"
priority = "normal"
position = 4
blocked-by = ["arcl-t-01M5VDDD42HZQ1FEN49X80GBA5", "arcl-t-01M5VDDD61CZH33T5S34E02BKM", "arcl-t-01M5VDDD8HBGD5CTHESBV2SSQH", "arcl-t-01M5VDDDAS9P34RVKCAGHXRSFH"]
+++

Make `arcl tree`'s default human output a box-drawing tree that folds
finished milestones, so open work fits on one screen.

Each line starts with a status mark: `✓` done, `◐` in progress, `●` ready,
`○` pending or blocked. Phases show progress counts such as `7/11`, and
blocked tasks end with `blocked by <short ID>`. Finished milestones collapse
into one line, such as `✓ Milestones 1–6  done · 29 tasks`, and open ones
expand. A footer summarizes the unplaced records:
`Inbox: 21 open · Notes: 17 · Releases: v0.1.0, v0.2.0`.

```text
Arc Lightning first release and desktop app
└── Roadmap
    ├── ✓ Milestones 1–6  done · 29 tasks
    ├── ◐ Milestone 7: Local web app  7/12
    │   ├── ✓ T23: Polish the web app and speed up its tests
    │   ├── ◐ T24: Format apps/web and packages/ui with Prettier
    │   └── ○ T25: Enable bracketSameLine  blocked by 1A2B
    └── ○ Milestone 8: Website and documentation  0/4
Inbox: 21 open · Notes: 17 · Releases: v0.1.0, v0.2.0
```

Colour only on a TTY, never with `NO_COLOR` set, and as `--color` says.
`--ascii` swaps the box characters for `|--`, `` `-- `` and `|`, as GNU
`tree -A` does. `--all` unfolds every milestone and includes closed captures.
`--depth N` stops after N levels.

Blocked by short record IDs, status per tree line, closed captures out of
the tree and plan first, since this view builds on all four.

Scope:

- the box-drawing renderer, status marks, progress counts and blocked-by
  references
- folding finished milestones into one line, and the unplaced-records footer
- `--ascii`, `--all` and `--depth N`
- the README's `arcl tree` example, from a real run on this repository,
  trimmed if long

Acceptance criteria:

- [ ] Default output is a box-drawing tree with a status mark on each line
- [ ] Phases show done/total counts, and blocked tasks name a blocker by
      short ID
- [ ] Consecutive finished milestones fold into one line; open ones expand
- [ ] A footer counts open captures and notes and names the releases
- [ ] Colour appears only on a TTY, and never with `NO_COLOR` or
      `--color never`
- [ ] `--ascii` uses only ASCII; `--all` unfolds everything and lists
      closed captures; `--depth N` limits depth
- [ ] The README shows the output of a real `arcl tree` run on this
      repository
- [ ] `--json` output is unchanged

Verification:

- snapshot tests of the renderer for folded, unfolded, ASCII and
  depth-limited output
- CLI tests for `NO_COLOR`, a non-TTY and `--color always`
- `arcl tree` and `arcl tree --ascii --all` on this repository
- `just check`
