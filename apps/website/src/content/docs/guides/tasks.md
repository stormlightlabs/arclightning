---
title: Tasks
description: Create tasks, place them under a spec, plan, phase or parent, and move them through their lifecycle with claims, notes, gates, handoffs and evidence.
---

A task is a unit of work. It has a title, a Markdown body, a status, a
priority, a position among its siblings, and optional subtasks, blockers and
gates. A subtask is an ordinary task with a parent, its own ID and its own
status.

## Create a task

```sh
arcl task create "Add focus rings" --phase KSX8 --priority high --body "Show focus on every control."
```

```text
Created task `arcl-t-01M6Z543M9MK02CDC2NKYD8GQ6`: Add focus rings
```

A task can sit under a phase, a plan, a spec, a parent task, or nothing:

| Option          | Places the task                                                |
| --------------- | -------------------------------------------------------------- |
| `--phase <ID>`  | In a phase, and so under its plan and spec.                    |
| `--plan <ID>`   | In a plan outside any phase, and so under the plan's spec.     |
| `--spec <ID>`   | Under a spec outside any plan.                                 |
| `--parent <ID>` | Below another task, sharing the parent's spec, plan and phase. |
| None            | At the top level of the project.                               |

A subtask takes its spec, plan and phase from its parent. Naming one that
differs from the parent's fails with exit code `3` and an error that the task
has contradictory `parent/spec`, `parent/plan` or `parent/phase` ancestry.
Where the parent has none, the subtask can name its own. The spec, plan or
phase must be open.

Priorities are `critical`, `high`, `normal` and `low`, with `normal` the
default. `--position <N>` orders siblings, lowest first, and defaults to `0`.
`--body-file <PATH|->` reads the body from a file or standard input.
`--blocked-by <ID>` adds blockers in the same write; the
[dependencies reference](/reference/deps/) covers them.

To create many tasks at once, with phases and blockers, write a
[plan file](/guides/plans/#write-a-plan-file).

## Update and move tasks

```sh
arcl task update 8GQ6 --title "Add visible focus rings"
arcl task update 8GQ6 --priority critical --position 20
arcl task update 543N --phase YE6M7
arcl task update AGD4 --parent 543N
arcl task update AGD4 --no-parent
```

`--spec`, `--plan`, `--phase` and `--parent` move a task, and `--no-spec`,
`--no-plan`, `--no-phase` and `--no-parent` take it out. Moving a task moves
its subtasks with it. A move that would create a parent cycle, or leave a task
blocked by its own ancestor or descendant, is refused before anything
changes. A task cannot move under an archived task.

## Move work through its lifecycle

```sh
arcl task start 7TR6
arcl task park 7TR6
arcl task unpark 7TR6
arcl task complete 7TR6
arcl task cancel 7TR6
```

A task has one of five statuses:

| Status        | Meaning                                               |
| ------------- | ----------------------------------------------------- |
| `pending`     | Not started. Ready once nothing holds it back.        |
| `in_progress` | Work has started.                                     |
| `parked`      | Open work temporarily excluded from active readiness. |
| `completed`   | Terminal success state.                               |
| `cancelled`   | Terminal state for work that will not continue.       |

`start` moves pending work to `in_progress` when it is ready, apart from having
child tasks. Any other reason that `arcl task explain` reports, such as an
unfinished blocker, refuses the start and lists the reasons; `--force` starts
the task anyway. Starting a task that is already in progress does nothing.

`--claim [<NAME>]` records who started the task as its assignee. While a task
has an assignee, any other start is refused with exit code `4`, with or without
`--force`; only `--claim` with the same name succeeds. Completing, parking or
handing off releases the claim, whoever runs it, and so does cancelling. A claim
holds within one database, which is one checkout's `.arcl/arcl.db`. Agents in
separate Git worktrees each have their own database, so both can claim the same
task; one sees the other's claim only after the workspace change reaches its
checkout through Git and a sync. `arcl list --assignee <NAME>` lists the
tasks one assignee holds.

`park` accepts pending or in-progress work. `unpark` always returns parked work
to `pending`. `complete` accepts pending or in-progress work, and `cancel`
accepts any non-terminal task.

## Leave notes

Notes record what you tried and decided while working, so the next session
reads it from `show` or `context` instead of guessing:

```sh
arcl task note 7TR6 --kind decision "Store notes in their own table"
arcl task note 7TR6 "The importer reads files in path order"
```

`--kind` is `decision`, `observation`, `blocker`, or `result`, and is optional.
A note is plain text of at most 2000 characters and may span lines. A task in
any status takes notes and keeps them. arcl commands only append notes; to
change or remove one, edit the task file and run `arcl workspace sync`. `show`
and the web app's Task view list them newest first, and `context` lists the 10
most recent. For longer writing that links to several records, use a
[note record](/guides/notes/).

## Wait on a pull request, run or date

A gate holds a task back until something outside arcl happens: a pull request
merges, a CI run passes, or a date arrives.

```sh
arcl gate add 7TR6 --pr https://github.com/acme/app/pull/42
arcl gate add 7TR6 --run https://github.com/acme/app/actions/runs/123456
arcl gate add 7TR6 --until 2026-11-15
```

While a gate is open, the task is not ready and `arcl explain` names the gate.
Gates do not resolve on their own. Run `arcl gate check` to mark the met
ones, for one task or for all:

```sh
arcl gate check
```

A date gate is met from the start of its date in UTC, with no network. Pull
request and run gates ask the GitHub CLI, `gh`, so it must be installed and
signed in. Without it, those gates stay open and the check says
`gh not found; PR/run gates left open`. A pull request that closed without
merging, or a run that failed, is reported as failed and stays open; remove
or replace that gate. `arcl gate list` shows gates and `arcl gate remove`
deletes one.

## Close parent tasks

By default, a task cannot become terminal while a descendant is still open.
Pass `--allow-open-children` to close only the selected task:

```sh
arcl task complete 7TR6 --allow-open-children
```

The override never cascades to child tasks.

Pass `--with-subtasks` to complete the task and every pending or in-progress
descendant in one write:

```sh
arcl task complete 7TR6 --with-subtasks --evidence "Shipped in #42."
```

Completed and cancelled descendants keep their status and evidence, and the
evidence goes on the selected task only. The command writes nothing and lists
the tasks involved when a descendant is parked, or when the task or an open
descendant has an unfinished blocker outside the subtree, held directly or by an
ancestor. Blockers inside the subtree do not count, even cancelled ones. There
is no `--force` for this. `--with-subtasks` and `--allow-open-children` cannot
be combined.

Human output names each subtask the command completed, and plain output adds a
`subtask<TAB><id>` line for each. JSON adds a `completed_subtasks` list of their
IDs in tree order, empty for a plain `complete`.

Every completion also reports the tasks it made ready: tasks that were not
ready before the command and are ready after it, including leaves under a
parent it blocked. Human output adds an `Unblocked task` line for each, plain
output an `unblocked<TAB><id>` line, and JSON an `unblocked` list of IDs, empty
when no task became ready.

## Handoff

An in-progress task can keep one current Markdown resume note:

```sh
arcl task handoff NGEH --note "Table drafted; wire it into the router next."
arcl task handoff NGEH --note-file handoff.md
```

```text
Handed off task `arcl-t-01M6Z4WFW2TSFXSAB8D95FNGEH`: Add the key bindings table
```

`handoff` stores the note and parks the task in one transaction, which also
releases its claim. Unparking and
starting the task preserve the note until another handoff replaces it. Use
`--note-file -` to read UTF-8 Markdown from standard input. After `unpark`,
`start` refuses a task that became not ready while parked; resolve the blocker
or pass `--force`.

## Record completion

Attach optional Markdown evidence when completing a task:

```sh
arcl task complete 7TR6 --evidence "All tests pass."
arcl task complete 7TR6 --evidence-file evidence.md
```

Use `--evidence-file -` to read from standard input. Arc Lightning stores the
evidence with the terminal transition; it does not run commands, open links,
or judge whether the evidence proves completion.

`arcl show` and `arcl context` include the current handoff and relevant blocker
evidence. JSON and plain output expose the same information for agents and
scripts.
