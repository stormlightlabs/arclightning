+++
id = "arcl-t-01M5QPZJRBMHJYTP3NVNHQ8T4Y"
parent = "arcl-t-01M5NE5P4D72Y56E0Y2JDE535B"
title = "T10.1: Add the Arc Lightning skill"
status = "completed"
priority = "normal"
position = 1
evidence = "Delivered by #30: `skills/arclightning/SKILL.md` teaches the ready, start, context, handoff and complete loop through `arcl --json` and links the automation guide for envelope fields and reason kinds. README and the automation guide give `npx skills add stormlightlabs/arclightning --skill arclightning`. `npx -y skills add <checkout> --list` names `arclightning`, and `--skill arclightning -a claude-code -y --copy` installed it into `.claude/skills/arclightning/`. The loop was run with `--json` against a scratch project."
+++

Add an Arc Lightning skill that teaches a coding agent the task loop through
`arcl` commands and their `--json` envelopes. Agents install it with
`npx skills add stormlightlabs/arclightning`, which discovers skills under a
top-level `skills/<name>/SKILL.md`.

Scope:

- `skills/arclightning/SKILL.md`, with files under
  `skills/arclightning/references/` where they keep `SKILL.md` short
- the install command in `README.md` and the automation guide,
  `apps/website/src/content/docs/guides/automation.md`

Acceptance criteria:

- [x] `SKILL.md` has `name: arclightning` and a `description` that says when to
      use it
- [x] It teaches the ready, start, context, handoff, and complete loop with
      `arcl` commands and `--json`, and points to the automation guide for
      envelope fields and reason kinds
- [x] It refers to product commands without restating domain rules such as
      readiness conditions or allowed transitions
- [x] `npx skills add stormlightlabs/arclightning --skill arclightning` lists
      and installs it

Verification:

- `npx -y skills add <checkout> --list` names `arclightning`
- Run the loop the skill describes against a scratch project with `--json`
- Prettier on the changed Markdown
- `just check`

Out of scope:

- `arcl skill install` (T10.2)
- MCP tools (T10.3)
