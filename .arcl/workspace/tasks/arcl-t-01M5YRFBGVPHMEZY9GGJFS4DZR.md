+++
id = "arcl-t-01M5YRFBGVPHMEZY9GGJFS4DZR"
phase = "arcl-ph-01M5YRFB35TZ51QW4AG1A8MS5B"
title = "Write agent setup with arcl setup"
status = "completed"
priority = "normal"
position = 7
blocked-by = ["arcl-t-01M5YRFB8NYG5KXXNBF3TSH9ZQ"]
evidence = "Done in this PR: `arcl setup claude|codex|pi|other` installs the skill through `arcl skill install`, adds a marked arcl block to CLAUDE.md or AGENTS.md, and writes the SessionStart hook (Claude Code, Codex, edited with jsonc-parser's syntax tree) or a Pi extension that runs `arcl prime`. `other`, the default, writes no hook. Existing files change only after a terminal prompt or `--yes`, without a terminal it exits 4, and `--dry-run` prints diffs and writes nothing. crates/arcl-cli/tests/setup.rs covers each agent: fresh project, `--yes`, refusal, dry run and a second run that changes nothing."
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

- [x] Each agent gets its skill, instruction lines and hook
- [x] Existing files are changed only after confirmation, or with `--yes`
- [x] `--dry-run` writes nothing

Verification:

- CLI tests in a scratch project for each agent
- `just check`
