---
name: arclightning
description: Pick up, work, hand off, and complete tasks tracked by Arc Lightning (`arcl`). Use in a project with an `.arcl/` directory, when asked what to work on next, to start or finish an arcl task, or to leave a resume note for the next session.
---

# Arc Lightning

Arc Lightning tracks a project's tasks in `.arcl/`. Drive it through the `arcl`
CLI with `--json`, and read fields from the envelope rather than from terminal
text. `arcl` decides readiness and which transitions are allowed. Ask it
instead of working that out yourself.

## Read the envelope

Every `--json` command writes one object to standard output:

```json
{ "format_version": 2, "data": { "...": "..." } }
```

Check `format_version` before reading `data`. A failure goes to standard error
as `{"format_version": 1, "error": {"code", "exit_code", "message"}}`, and the
command exits with that nonzero `exit_code`. The
[automation guide](https://github.com/stormlightlabs/arclightning/blob/main/apps/website/src/content/docs/guides/automation.md)
lists the version each command returns, the readiness reason kinds, and their
ID fields.

## Work the loop

1. Find work. `arcl --json ready` returns `data.tasks` in order, and
   `arcl --json next` returns the first as `data.task`. An empty list means
   nothing can start.
2. Start it. `arcl --json task start <id>` moves the task to in progress. If
   it refuses with `invalid_request`, run `arcl --json explain <id>` and read
   `data.readiness.reasons`. Do not pass `--force` unless the person asks.
3. Read the context. `arcl --json context <id>` returns the task with its
   ancestors, spec, plan, phase, blockers, dependents, readiness, and earlier
   evidence. `data.task.handoff` holds the last resume note, and
   `data.task.body` holds the acceptance criteria.
4. Do the work the task describes.
5. Finish or hand off:
   - Done: `arcl --json task complete <id> --evidence "<proof>"`, where the
     evidence says what shows the work is done. `data.unblocked` lists the
     tasks this made ready. Start the next one from there or go back to
     step 1.
   - Not done: `arcl --json task handoff <id> --note "<note>"`, where the
     note says where the work stands and what is next. This parks the task.
     The next session runs `arcl task unpark <id>`, then starts it again and
     reads the note from the context.

Use `--evidence-file -` and `--note-file -` to pass longer Markdown on
standard input.

## Record new work

When you find work outside the task, capture it instead of doing it:

```sh
arcl --json capture create "<title>" --body "<what and why>"
```

Run `arcl <command> --help` for every flag. `arcl explain <id>` says why a task
is not ready, and `arcl check` validates the project.
