+++
id = "arcl-t-01M6YVGX916X31EBNF0D5FM15X"
phase = "arcl-ph-01M5YRFB35TZ51QW4AG1A8MS5B"
title = "Pack archived records into JSONL archives"
status = "pending"
priority = "normal"
position = 16
blocked-by = ["arcl-t-01M5YRFBPD4ZGFC56Q2WCSQHHN"]
+++

`arcl compact` archives tasks but keeps their files, so closed work dominates
`.arcl/workspace`. In this repository 73 of 87 tasks are completed. Packing
moves archived records into one JSONL file per phase, while the database
keeps indexing them so links still resolve.

Scope:

- `arcl compact --pack` moves archived records into
  `.arcl/archive/<phase-slug>.jsonl` and deletes their per-record files
- `arcl restore <id>` writes a packed record back to its file
- packing covers archived tasks, closed captures, completed phases, shipped
  releases and superseded notes
- optional `arcl compact --purge` drops archive lines
- a retention policy in `.arcl/config.toml`
- workspace format 7

Acceptance criteria:

- [ ] `arcl compact --pack` writes each archived record as one line of
      `.arcl/archive/<phase-slug>.jsonl`, sorted by ID, with its front matter
      and body intact, and deletes the record's file
- [ ] Records in a project without phases pack into one file per completion
      month
- [ ] The database keeps indexing packed records, so `show`, links and
      `source-task` resolve them
- [ ] `arcl restore <id>` writes the record back to its file and removes its
      archive line
- [ ] Closed captures, completed phases, shipped releases and superseded notes
      pack and restore the same way
- [ ] `arcl compact --purge` drops archive lines and records the purge in the
      phase's compaction note, which says git history still holds them
- [ ] `[retention]` in `.arcl/config.toml` sets `pack-after` and `kinds`, and
      `arcl compact --pack` follows it
- [ ] `arcl check` fails when a record is both a file and an archive line, or
      when an archive line does not parse
- [ ] The workspace format is 7, and older workspaces still load

Verification:

- CLI tests on a fixture project: pack, then `show`, links and `source-task`
  on a packed record; restore; purge; a project without phases
- `arcl check` tests for a duplicated record and a malformed line
- `arcl compact --pack` on a copy of this repository's workspace, then
  `arcl check` and a diff of `arcl show` output before and after
- `just check`
