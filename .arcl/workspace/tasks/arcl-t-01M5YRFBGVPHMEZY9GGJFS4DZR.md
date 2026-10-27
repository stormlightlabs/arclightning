+++
id = "arcl-t-01M5YRFBGVPHMEZY9GGJFS4DZR"
phase = "arcl-ph-01M5YRFB35TZ51QW4AG1A8MS5B"
title = "Write agent setup with arcl setup"
status = "pending"
priority = "normal"
position = 6
blocked-by = ["arcl-t-01M5YRFB8NYG5KXXNBF3TSH9ZQ"]
+++

Write the files a coding agent needs to use arcl in one command. Taken from
beads' `bd setup`.

`arcl setup claude|codex|pi` installs the skill, adds the arcl lines to
`AGENTS.md` or `CLAUDE.md`, and writes the hook that runs `arcl prime`. It
shows a diff and asks before changing a file that already exists.

Scope:

- `arcl setup <agent>` for Claude Code, Codex and Pi
- reusing `arcl skill install`
- `--dry-run` to print the changes

Acceptance criteria:

- [ ] Each agent gets its skill, instruction lines and hook
- [ ] Existing files are changed only after confirmation, or with `--yes`
- [ ] `--dry-run` writes nothing

Verification:

- CLI tests in a scratch project for each agent
- `just check`
