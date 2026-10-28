---
name: arclightning
description: Pick up, work, hand off, and complete tasks tracked by Arc Lightning (`arcl`). Use in a project with an `.arcl/` directory, when asked what to work on next, to start or finish an arcl task, or to leave a resume note for the next session.
---

# Arc Lightning

Arc Lightning tracks a project's tasks in `.arcl/`. Drive it through the `arcl`
CLI with `--json`, and read fields from the envelope rather than from terminal
text. `arcl` decides readiness and which transitions are allowed. Ask it
instead of working that out yourself.

When the `arcl-mcp` server is connected, call its tools instead of the
commands. Each step below names the tool in brackets. A tool returns the same
envelope as its command, as structured content. Its input schema lists the
fields it takes, which do not always match the command's options.

## Read the envelope

Every `--json` command writes one object to standard output:

```json
{ "format_version": <n>, "data": { "...": "..." } }
```

The version depends on the command. Check it before reading `data`. A failure goes to standard error
as `{"format_version": 1, "error": {"code", "exit_code", "message"}}`, and the
command exits with that nonzero `exit_code`. A failed tool call returns the
same object with `isError` set. The
[automation guide](https://github.com/stormlightlabs/arclightning/blob/main/apps/website/src/content/docs/guides/automation.md)
lists the version each command returns, the readiness reason kinds, and their
ID fields.

## Work the loop

In short (`arcl prime` prints this list):

<!-- arcl-prime:begin -->

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

<!-- arcl-prime:end -->

In full:

1. Find work. First pick up parked work:
   `arcl --json list --kind task --status parked` [`list`] lists tasks a
   session handed off. Parking releases the claim, so these have no assignee;
   read the handoff before you unpark one. Otherwise `arcl --json ready`
   [`ready`] returns `data.tasks` in order, and `arcl --json next` returns
   the first as `data.task`. An empty list means nothing can start. The tools return at
   most `limit` records and a `data.next_cursor` to pass as `cursor` while
   more remain.
2. Start it. `arcl --json task start <id> --claim` [`task_start` with
   `claim`] moves the task to in progress and records you as its assignee.
   The command names you by `ARCL_AGENT` or else the Git user name. The tool
   has no default: pass your name in `claim`. From the CLI,
   `arcl --json next --start --claim` picks and starts the first ready task
   in one step, so another agent cannot take it in between; a null
   `data.task` means nothing can start. If it refuses with
   `invalid_request`, run `arcl --json explain <id>` [`explain`] and read
   `data.readiness.reasons`. A `conflict` means another assignee holds the
   task; pick other work. Do not pass `--force` unless the person asks; it
   does not override a claim. The tool has no force. Completing, parking or
   handing off releases the claim, whoever runs it. A claim holds only within
   one checkout's database: an agent in another Git worktree does not see it
   until the change reaches that checkout through Git and a sync.
3. Read the context. `arcl --json context <id>` [`context`] returns the task
   with its ancestors, spec, plan, phase, blockers, dependents, readiness, and
   earlier evidence. `data.task.handoff` holds the last resume note, and
   `data.task.body` holds the acceptance criteria. `data.notes` holds the 10
   most recent notes, newest first, and `data.omitted_notes` counts the rest,
   which `arcl --json show <id>` lists.
4. Do the work the task describes. Leave a note when you make a decision,
   find something the next session needs, hit a blocker, or get a result:
   `arcl --json task note <id> --kind decision "<what and why>"`
   [`task_note`]. The kinds are `decision`, `observation`, `blocker` and
   `result`; leave `--kind` out for none. Notes are plain text of at most
   2000 characters, kept in order, and survive context compaction, parking
   and completion.
5. Finish or hand off:
   - Done: `arcl --json task complete <id> --evidence "<proof>"`
     [`task_complete`], where the evidence says what shows the work is done.
     `data.unblocked` lists the tasks this made ready. Start the next one from
     there or go back to step 1.
   - Not done: `arcl --json task handoff <id> --note "<note>"`
     [`task_handoff`], where the note says where the work stands and what is
     next. This parks the task. The next session finds it in step 1, runs
     `arcl --json task unpark <id>` [`task_unpark`], then starts it again and
     reads the note from the context.

Use `--evidence-file -` and `--note-file -` to pass longer Markdown on
standard input.

## Record new work

When you find work outside the task, capture it instead of doing it:

```sh
arcl --json capture create "<title>" --body "<what and why>"
```

The tool is `capture_create`. The server also has `capture_promote`, `show`,
`spec_create`, `plan_create`, `task_create`, `task_update` and `note_create`,
for `arcl capture promote`, `arcl show`, `arcl spec create`,
`arcl plan create`, `arcl task create`, `arcl task update` and
`arcl note create`.

## Wait on a pull request, run or date

When a task cannot start until a pull request merges, a CI run passes or a
date arrives, gate it instead of parking it or waiting:

```sh
arcl --json gate add <id> --pr https://HOST/OWNER/REPO/pull/NUMBER
arcl --json gate add <id> --run https://HOST/OWNER/REPO/actions/runs/RUN_ID
arcl --json gate add <id> --until YYYY-MM-DD
```

The tool is `gate_add`, with `kind` and `target`. An open gate keeps the task
out of `ready`, and `explain` lists it as an `open_gate` reason. Gates never
resolve on their own: `arcl --json gate check` [`gate_check`] marks met
gates. Date gates compare with today's UTC date. Pull request and run gates
need the GitHub CLI, `gh`; without it they stay open and `data.gh_missing` is
true. A `failed` outcome means the pull request closed unmerged or the run
failed: remove the gate with `arcl gate remove <gate-id>` or replace it.
`arcl --json gate list [<id>]` [`gate_list`] shows gates.

Run `arcl <command> --help` for every flag, and `arcl check` to validate the
project.
