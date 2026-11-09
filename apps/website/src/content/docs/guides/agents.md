---
title: Agent workflows
description: Set up Claude Code, Codex, Pi or another coding agent with arcl setup, prime each session, and run the claim, note, hand off and complete loop.
---

A coding agent works an Arc Lightning project the way a person does: it picks
a ready task, reads its context, does the work, and completes it with evidence
or hands it off. arcl gives the agent three things for this: a skill that
teaches the loop, a primer that puts the open work in its context at session
start, and claims that stop two agents starting the same task.

## Set up an agent

`arcl setup` writes the skill, the arcl lines in the agent's instruction file
and, where the agent supports one, a hook that runs `arcl prime` when a
session starts:

```sh
arcl setup claude
arcl setup codex
arcl setup pi
arcl setup
```

| Agent    | Instruction file | Hook                                                        |
| -------- | ---------------- | ----------------------------------------------------------- |
| `claude` | `CLAUDE.md`      | `SessionStart` command in `.claude/settings.json`           |
| `codex`  | `AGENTS.md`      | `SessionStart` command in `.codex/hooks.json`               |
| `pi`     | `AGENTS.md`      | `session_start` extension in `.pi/extensions/arcl-prime.ts` |
| `other`  | `AGENTS.md`      | None                                                        |

`arcl setup` with no agent runs `arcl setup other`, for any agent that reads
`AGENTS.md` and `.agents/skills/`. Every agent gets the skill in
`.claude/skills/arclightning/` and `.agents/skills/arclightning/`.

`--dry-run` prints each change as a diff and writes nothing:

```sh
arcl --plain setup codex --dry-run
```

```text
created	.claude/skills/arclightning/SKILL.md
created	.agents/skills/arclightning/SKILL.md
created	AGENTS.md
created	.codex/hooks.json
```

Before changing a file that exists, `setup` shows the diff and asks; `--yes`
skips the question. The instruction lines sit between
`<!-- arcl-setup:begin -->` and `<!-- arcl-setup:end -->` markers, and a
later run replaces only that block. Running `setup` twice changes nothing the
second time. Commit the files it writes so every clone gets them.

Codex runs project hooks only in a trusted project, after you trust the hook
in `/hooks`. Pi loads `.pi/extensions/` and `.agents/skills/` only after you
trust the project. The [manual](/reference/manual/#arcl-setup) covers the
JSON rules for settings files and every exit code.

## The skill

The skill teaches an agent the loop below with `--json`, or with the MCP
tools when the [MCP server](/guides/mcp/) is connected. `arcl skill install`
writes the copy built into `arcl`, the same files `setup` writes, and
`--check` reports drift without writing:

```sh
arcl skill install
arcl skill install --check
```

The [skills CLI](https://skills.sh) installs it from GitHub instead:

```sh
npx skills add stormlightlabs/arclightning --skill arclightning
```

`--skill arclightning` picks it out of the other skills this repository keeps
for its own development.

## Prime each session

`arcl prime` prints the working loop, the in-progress and parked tasks with
the first line of each handoff, and the first ready tasks, in at most 40 lines
and 4 KiB:

```text
Arc Lightning tracks this project's tasks. Work the loop:

1. Pick up parked work first. Parking releases the claim, so any session may
   take it: read the handoff in `arcl context <id>`, then
   `arcl task unpark <id>` and `arcl task start <id> --claim`.
2. Otherwise start the first ready task: `arcl next --start --claim`.
3. Read `arcl context <id>`: acceptance criteria, blockers, last handoff.
4. Do the work. Note each decision, finding or blocker with
   `arcl task note <id> --kind <kind> "<text>"`. Record work outside the task
   with `arcl capture create`.
5. Finish with `arcl task complete <id> --evidence "<proof>"`, or park it with
   `arcl task handoff <id> --note "<where it stands, what is next>"`.

Open work: none.

Ready:
- 7TR6 [normal]: Move the selection with j and k

`arcl context <id>` shows a task's criteria, blockers and full handoff.
```

The hooks `setup` writes run it when a session starts, so the agent begins
with the loop and the open work in its context. For `other`, have the agent
run `arcl prime` itself. The MCP `prime` tool returns the same primer.

The hooks run `arcl prime` only when `arcl` is installed, so a teammate
without it starts a session with no hook error. In a directory with no
`.arcl/arcl.db`, such as a fresh clone, `arcl prime` prints one line saying to
run `arcl init` and exits `0`. It still fails on a database it cannot open.

To write the hook by hand, add it to `.claude/settings.json` for Claude
Code. A `SessionStart` hook with no matcher also runs after `/clear` and after
compaction, so the primer comes back once the context is compacted:

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

For Codex, add the same shape to `.codex/hooks.json`:

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

## Claim work

`--claim` records who started a task:

```sh
export ARCL_AGENT=codex
arcl next --start --claim
arcl list --assignee codex
```

Without a name, `--claim` uses `ARCL_AGENT`, then the Git user name. While a
task has an assignee, any start that does not claim it for the same name is
refused with exit code `4`, even with `--force`. Parking, handing off,
completing and cancelling release the claim. `next --start` selects and starts
the task in one transaction, so two agents on one database never start the
same task.

A claim holds within one database. Agents in separate Git worktrees each have
their own `.arcl/arcl.db`, so both can claim the same task, and each sees the
other's claim only after the workspace change reaches it through Git and a
sync. Give parallel agents different tasks, or different phases with
`arcl next --start --claim --phase <id>`.

## Leave a trail

Record the work in arcl, where the next session can read it:

- `arcl task note <id> --kind decision "<text>"` records a decision,
  observation, blocker or result on the task.
- `arcl capture create "<title>"` files work found along the way. With
  `ARCL_AGENT` set and one claimed task in progress, the capture names that
  task as its source.
- `arcl task handoff <id> --note "<where it stands>"` parks unfinished work
  with a resume note.
- `arcl task complete <id> --evidence "<proof>"` closes the task with what
  shows it is done, and reports the tasks it made ready.

The next session reads all of it from `arcl context <id>`. The
[tasks guide](/guides/tasks/) covers each command.
