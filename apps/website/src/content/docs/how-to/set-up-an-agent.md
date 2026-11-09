---
title: Set up an agent
description: Set up Claude Code, Codex, Pi or another coding agent with arcl setup, install or check the skill, and prime each session with the open work.
---

<div id="set-up-an-agent"></div>

## Run arcl setup

Run `arcl setup` with the agent's name, `claude`, `codex` or `pi`, or with no
name for another agent. For Claude
Code:

```sh
arcl setup claude
```

```text
Set up Claude Code in `/home/you/notes-app`:
  created .claude/skills/arclightning/SKILL.md
  created .agents/skills/arclightning/SKILL.md
  created CLAUDE.md
  created .claude/settings.json
```

`setup` writes the skill, the arcl lines in the agent's instruction file and,
where the agent supports one, a hook that runs `arcl prime` when a session
starts:

| Agent    | Instruction file | Hook                                                        |
| -------- | ---------------- | ----------------------------------------------------------- |
| `claude` | `CLAUDE.md`      | `SessionStart` command in `.claude/settings.json`           |
| `codex`  | `AGENTS.md`      | `SessionStart` command in `.codex/hooks.json`               |
| `pi`     | `AGENTS.md`      | `session_start` extension in `.pi/extensions/arcl-prime.ts` |
| `other`  | `AGENTS.md`      | None                                                        |

With no agent named, `arcl setup` runs `arcl setup other`, for any agent that
reads `AGENTS.md` and `.agents/skills/`. Every agent gets the skill in
`.claude/skills/arclightning/` and `.agents/skills/arclightning/`.

Commit the files `setup` writes, so every clone gets them.

## Preview the changes

```sh
arcl --plain setup codex --dry-run
```

```text
unchanged	.claude/skills/arclightning/SKILL.md
unchanged	.agents/skills/arclightning/SKILL.md
created	AGENTS.md
created	.codex/hooks.json
```

Human output shows each change as a diff. Before changing a file that already
exists, `setup` shows the diff and asks; `--yes` skips the question. Without a
terminal and without `--yes`, it writes nothing and exits with code `4`.

The instruction lines sit between `<!-- arcl-setup:begin -->` and
`<!-- arcl-setup:end -->`. A later run replaces only that block, and running
`setup` twice changes nothing the second time.

## Trust the project

Codex runs project hooks only in a trusted project, after you trust the hook
in `/hooks`. Pi loads `.pi/extensions/` and `.agents/skills/` only after you
trust the project. `setup` prints these as notes.

<div id="prime-each-session"></div>

## Prime an agent without a hook

`other` writes no hook. Have the agent run `arcl prime` when a session starts.
It prints the working loop, the in-progress and parked tasks with the first
line of each handoff, and the first ready tasks. The output stays within 40
lines and 4 KiB. The MCP `prime` tool returns the same content as JSON,
which keeps more of each title and handoff; [MCP tools](/reference/mcp/#prime)
lists its limits.

## Write the hook by hand

For Claude Code, add this to `.claude/settings.json`. A `SessionStart` hook
with no matcher also runs after `/clear` and after compaction, so the primer
comes back once the context is compacted:

```json
{
	"hooks": {
		"SessionStart": [
			{
				"hooks": [
					{
						"type": "command",
						"command": "if command -v arcl >/dev/null; then arcl prime; fi"
					}
				]
			}
		]
	}
}
```

For Codex, add this to `.codex/hooks.json`:

```json
{
	"hooks": {
		"SessionStart": [
			{
				"matcher": "startup|resume",
				"hooks": [
					{
						"type": "command",
						"command": "if command -v arcl >/dev/null; then arcl prime; fi"
					}
				]
			}
		]
	}
}
```

The `command -v` test skips the hook for a teammate without `arcl`. In a
clone with no database yet, `arcl prime` prints one line saying to run
`arcl init` and exits `0`.

<div id="the-skill"></div>

## Install or check the skill alone

`arcl skill install` writes the skill built into `arcl`, the same files
`setup` writes. `--check` reports drift without writing, and exits `4` when a
file is missing or changed:

```sh
arcl skill install
arcl skill install --check
```

```text
The Arc Lightning skill in `/home/you/notes-app` matches this arcl.
```

The [skills CLI](https://skills.sh) installs the skill from GitHub instead:

```sh
npx skills add stormlightlabs/arclightning --skill arclightning
```

`--skill arclightning` picks it out of the other skills the repository keeps
for its own development.

<div id="claim-work"></div>
<div id="leave-a-trail"></div>

## Name each agent

Set `ARCL_AGENT` in each agent's environment, such as `ARCL_AGENT=codex`.
`--claim` uses it as the assignee, and `arcl capture create` uses it to find
the task a capture came from. [Claims and
handoffs](/explanation/claims-and-handoffs/) explains how claims work across
agents.
