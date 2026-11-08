+++
id = "arcl-t-01M5YRFBPD4ZGFC56Q2WCSQHHN"
phase = "arcl-ph-01M5YRFB35TZ51QW4AG1A8MS5B"
title = "Compact old completed work into summaries"
status = "completed"
priority = "normal"
position = 13
evidence = 'Done in this PR: `arcl compact --before <date> [--phase <id>] [--include-undated] [--dry-run]` archives completed tasks into one "Compacted work" note per phase. Tasks record `completed-at` on completion and `archived-at` on compaction (schema 9, workspace format 6). `list`, `tree` and `context` hide archived tasks unless `--all`; progress still counts them. CLI tests in crates/arcl-cli/tests/compact.rs; gates pass.'
completed-at = "2026-11-08T16:02:15.908Z"
+++

Keep `arcl context` and `arcl prime` small on long-lived projects by
replacing old completed tasks with a summary note. Taken from beads'
`bd admin compact`.

Scope:

- `arcl compact --before <date> [--phase <id>]` writing one summary note per
  phase from the evidence of its completed tasks, then archiving those tasks
- archived tasks kept in the workspace files and readable with `--all`
- `--dry-run`

Acceptance criteria:

- [x] Compaction leaves one summary note per phase and archives its tasks
- [x] Archived tasks no longer appear in `tree`, `list` or `context` by default
- [x] `--dry-run` writes nothing

Verification:

- CLI tests on a fixture project
- `just check`
