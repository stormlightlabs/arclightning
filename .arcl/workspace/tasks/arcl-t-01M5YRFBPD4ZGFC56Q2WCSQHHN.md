+++
id = "arcl-t-01M5YRFBPD4ZGFC56Q2WCSQHHN"
phase = "arcl-ph-01M5YRFB35TZ51QW4AG1A8MS5B"
title = "Compact old completed work into summaries"
status = "pending"
priority = "normal"
position = 10
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

- [ ] Compaction leaves one summary note per phase and archives its tasks
- [ ] Archived tasks no longer appear in `tree`, `list` or `context` by default
- [ ] `--dry-run` writes nothing

Verification:

- CLI tests on a fixture project
- `just check`
