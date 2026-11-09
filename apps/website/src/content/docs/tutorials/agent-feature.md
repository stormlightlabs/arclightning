---
title: Plan and finish a feature with an agent
description: Set up a coding agent, write a spec and a plan file, and follow the agent through claiming, handing off, resuming and completing the work.
---

In this tutorial you plan a small feature, "Export notes as Markdown". Then
you work it the way a coding agent does: claim a task, leave notes, hand off
partway, resume in a new session, and finish. You run the agent's commands
yourself, so you see each step. In a real project the agent runs them.

You need Git and an `arcl` built from `main`, as the [quick
start](/tutorials/quick-start/) installs it. The latest release, v0.3.0, does
not have `arcl setup`, `arcl prime`, `arcl task note` or `--claim`. The examples use Claude Code; the steps are the same for
other agents.

Every record gets its own ID, so yours differ from the ones shown. Where a
command names a short ID such as `N62T`, use the last four or more characters
of your own record's ID.

## Set up the project and the agent

Make a repository with one commit, then initialize arcl and set up the agent:

```sh
mkdir notes-app && cd notes-app
git init
echo "# Notes app" > README.md
git add README.md && git commit -m "Start the notes app"
arcl init --workspace
arcl setup claude
```

```text
Set up Claude Code in `/home/you/notes-app`:
  created .claude/skills/arclightning/SKILL.md
  created .agents/skills/arclightning/SKILL.md
  created CLAUDE.md
  created .claude/settings.json
```

`arcl setup` wrote three things. The skill teaches the agent the working
loop. `CLAUDE.md` tells it to use the skill. `.claude/settings.json` holds a
hook that runs `arcl prime` when a session starts.

## Write the spec

A spec says what should be true when the work is done. Put the acceptance
criteria in `criteria.md`:

```markdown
- [ ] `notes export --format md` writes one Markdown file per note
- [ ] Each file keeps the note's title as its first heading
```

Create the spec from it:

```sh
arcl spec create "Export notes as Markdown" \
  --body "Let people take their notes out of the app." \
  --acceptance-criteria-file criteria.md
```

```text
Created spec `arcl-s-01M70P60VJQYJ2PE3K8K33E5TN`: Export notes as Markdown
```

## Write the plan file

Save the plan as `export-plan.toml`. It has two phases. The `--format` flag
waits for the exporter to write files, and writing waits for rendering:

```toml
format-version = 1

[[phases]]
key = "core"
title = "Exporter"
position = 10

[[phases.tasks]]
key = "render"
title = "Render a note as Markdown"
priority = "high"
position = 10

[[phases.tasks]]
key = "write"
title = "Write one file per note"
position = 20
blocked-by = ["render"]

[[phases]]
key = "cli"
title = "Command line"
position = 20

[[phases.tasks]]
key = "flag"
title = "Add the --format flag"
position = 10
blocked-by = ["core/write"]
```

Create an empty plan under the spec, then check the file against it. The
check writes nothing:

```sh
arcl plan create "Markdown export" --spec E5TN
arcl plan check PPQD --file export-plan.toml
```

```text
Created plan `arcl-pl-01M70P66W2Q2CJPFTGDNMKPPQD`: Markdown export
Plan `arcl-pl-01M70P66W2Q2CJPFTGDNMKPPQD`
phase	core	Create
phase	cli	Create
task	core/render	Create
task	core/write	Create
task	cli/flag	Create
dependency	core/write	render	Create
dependency	cli/flag	core/write	Create
```

Apply the file and look at the result. `--style full` adds each record's
short ID on the right; the rest of the tutorial uses them:

```sh
arcl plan apply PPQD --file export-plan.toml
arcl tree --style full
```

```text
Applied plan `arcl-pl-01M70P66W2Q2CJPFTGDNMKPPQD` (2 phase(s), 3 task(s)).
○ Export notes as Markdown                              E5TN
└── ○ Markdown export                                   PPQD
    ├── ○ Exporter  0/2                                 3SVB
    │   ├── ● Render a note as Markdown                 N62T
    │   └── ○ Write one file per note  blocked by N62T  FH6E
    └── ○ Command line  0/1                             GH7N
        └── ○ Add the --format flag  blocked by FH6E    3ZMZ
```

Commit the plan, so the agent's branch starts from it:

```sh
git add -A && git commit -m "Plan Markdown export"
```

## Start the first session

When a session starts, the hook runs `arcl prime`. It prints the working loop,
then the open and ready work:

```sh
arcl prime
```

```text
Arc Lightning tracks this project's tasks. Work the loop:
...
Open work: none.

Ready:
- N62T [high]: Render a note as Markdown

`arcl context <id>` shows a task's criteria, blockers and full handoff.
```

The agent names itself with `ARCL_AGENT` and takes the first ready task:

```sh
export ARCL_AGENT=claude
arcl next --start --claim
```

```text
Started task `arcl-t-01M70P66XF1Z3GBZFH2JTWN62T`: Render a note as Markdown
```

The task now belongs to `claude`. Another start, by a person or a second
agent on this checkout, is refused until the claim is released.

Next the agent reads the task's context. `arcl --json context N62T` gives it
the spec's acceptance criteria too:

```sh
arcl context N62T
```

```text
Task `arcl-t-01M70P66XF1Z3GBZFH2JTWN62T` [in_progress] Render a note as Markdown
Ready: false
Assignee: claude
Ancestors:
Spec: arcl-s-01M70P60VJQYJ2PE3K8K33E5TN
Let people take their notes out of the app.
...
```

## Leave a trail

While it works, the agent records a decision on the task:

```sh
arcl task note N62T --kind decision "Render with pulldown-cmark's writer, not string templates"
```

It also notices a problem outside this task and files it in the inbox instead
of fixing it now:

```sh
arcl capture create "Notes with attachments lose them on export"
```

```text
Created capture `arcl-c-01M70P6C1V8ZVXJCXVGMSRKP0G`: Notes with attachments lose them on export
From: task `N62T`
```

The capture names `N62T` as its source, because `claude` holds exactly one
task in progress.

## Hand off and resume

The session ends before the task is done. The agent hands it off with a note
for whoever comes next:

```sh
arcl task handoff N62T --note "Headings and lists render. Code blocks still lose their language tag; fix that in render.rs next."
```

```text
Handed off task `arcl-t-01M70P66XF1Z3GBZFH2JTWN62T`: Render a note as Markdown
```

The handoff parked the task and released the claim. Start a new session. This
time `arcl prime` lists the parked task first:

```text
Open work:
- N62T parked: Render a note as Markdown
  Handoff: Headings and lists render. Code blocks still lose their language tag; fix tha…

Ready: none.
```

The new session reads the whole handoff and the notes with
`arcl context N62T`:

```text
Handoff: Headings and lists render. Code blocks still lose their language tag; fix that in render.rs next.
Evidence: none
Note: 2026-11-09T17:37:45.511Z [decision] Render with pulldown-cmark's writer, not string templates
```

Then it takes the task back and finishes it with evidence:

```sh
arcl task unpark N62T
arcl task start N62T --claim
arcl task complete N62T --evidence "render.rs keeps code block languages; cargo test passes (14 tests)."
```

```text
Unparked task `arcl-t-01M70P66XF1Z3GBZFH2JTWN62T`: Render a note as Markdown
Started task `arcl-t-01M70P66XF1Z3GBZFH2JTWN62T`: Render a note as Markdown
Completed task `arcl-t-01M70P66XF1Z3GBZFH2JTWN62T`: Render a note as Markdown
Unblocked task `arcl-t-01M70P66XFJVHG2640R6X2FH6E`: Write one file per note
```

## Finish the plan

The agent repeats the loop for the next two tasks. Each completion unblocks
the next task, so `arcl next` starts it:

```sh
arcl next --start --claim
arcl task complete FH6E --evidence "Writes notes/<slug>.md per note; tested on 200 notes."
arcl next --start --claim
arcl task complete 3ZMZ --evidence "notes export --format md works end to end."
```

```text
Started task `arcl-t-01M70P66XFJVHG2640R6X2FH6E`: Write one file per note
Completed task `arcl-t-01M70P66XFJVHG2640R6X2FH6E`: Write one file per note
Unblocked task `arcl-t-01M70P66XF22PK2ANDE6S33ZMZ`: Add the --format flag
Started task `arcl-t-01M70P66XF22PK2ANDE6S33ZMZ`: Add the --format flag
Completed task `arcl-t-01M70P66XF22PK2ANDE6S33ZMZ`: Add the --format flag
```

Now triage the capture the agent filed. It belongs in the Exporter phase,
`3SVB`, so promote it to a task there:

```sh
arcl capture promote KP0G task --phase 3SVB --priority low
```

```text
Promoted capture `arcl-c-01M70P6C1V8ZVXJCXVGMSRKP0G` to task `arcl-t-01M70P6HJF427BY91CZFVRC6Q4`
```

Work that last task, `C6Q4`, the same way. Then close the phases, the plan
and the spec. Each refuses to close while work below it is open:

```sh
arcl next --start --claim
arcl task complete C6Q4 --evidence "Attachments copy into an assets/ folder beside each note."
arcl plan phase complete 3SVB
arcl plan phase complete GH7N
arcl plan complete PPQD
arcl spec complete E5TN
arcl tree
```

```text
✓ Export notes as Markdown
└── ✓ Markdown export
    └── ✓ 2 phases  done · 4 tasks
```

Commit the records. The task files carry the notes, the handoff and the
evidence, so the history of the feature travels with the code:

```sh
git add -A && git commit -m "Finish Markdown export"
```

## Where to go next

- [Set up an agent](/how-to/set-up-an-agent/) covers Codex, Pi and agents
  without a start-up hook.
- [Connect the MCP server](/how-to/connect-mcp/) gives the agent typed tools
  in place of CLI output.
- [Claims and handoffs](/explanation/claims-and-handoffs/) explains what a
  claim protects and where it stops.
