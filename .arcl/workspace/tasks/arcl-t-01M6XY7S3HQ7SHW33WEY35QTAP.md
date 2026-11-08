+++
id = "arcl-t-01M6XY7S3HQ7SHW33WEY35QTAP"
phase = "arcl-ph-01M5YRFB35TZ51QW4AG1A8MS5B"
title = "Backfill task completion times from git history"
status = "pending"
priority = "normal"
position = 15
+++

Tasks completed before arcl recorded completion times have no
`completed-at`, so `arcl compact` skips them unless `--include-undated` is
given. Recover their real completion dates from Git history.

Scope:

- for each completed task without `completed-at`, find the commit that set
  its task file to `status = "completed"` and write that commit's date as
  `completed-at`
- a subcommand, `arcl task backfill-completion`, with `--dry-run`, so the
  backfill runs once on purpose and never during a normal sync

Limits:

- squashed or rebased history dates the task by the squash commit, not the
  original completion
- a workspace imported from another repository, or files added already
  completed, carry no commit that changed the status; those tasks stay undated
- without a Git checkout there is no history, and the command refuses

Acceptance criteria:

- [ ] Undated completed tasks get the date of the commit that completed them
- [ ] Tasks with no such commit stay undated and are reported
- [ ] `--dry-run` writes nothing
