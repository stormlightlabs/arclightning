---
title: Work a task to done
description: Create and place a task, start and claim it, record notes, wait on a pull request or date, and complete it with evidence, alone or with its subtasks.
---

## Create a task

```sh
arcl task create "Add focus rings" --phase KSX8 --priority high --body "Show focus on every control."
```

```text
Created task `arcl-t-01M6Z543M9MK02CDC2NKYD8GQ6`: Add focus rings
```

Choose where the task sits:

| Option          | Places the task                                                |
| --------------- | -------------------------------------------------------------- |
| `--phase <ID>`  | In a phase, and so under its plan and spec.                    |
| `--plan <ID>`   | In a plan outside any phase, and so under the plan's spec.     |
| `--spec <ID>`   | Under a spec outside any plan.                                 |
| `--parent <ID>` | Below another task, sharing the parent's spec, plan and phase. |
| None            | At the top level of the project.                               |

The spec, plan or phase must be open. A subtask that names a different spec,
plan or phase from its parent's fails with exit code `3`. Where the parent has
none, the subtask can name its own.

Priorities are `critical`, `high`, `normal` (the default) and `low`.
`--position <N>` orders siblings, lowest first, and defaults to `0`.
`--blocked-by <ID>` adds blockers in the same write.

To create many tasks at once, with phases and blockers, [write a plan
file](/how-to/plan-from-a-file/).

## Move or change a task

```sh
arcl task update 8GQ6 --title "Add visible focus rings"
arcl task update 8GQ6 --priority critical --position 20
arcl task update 543N --phase YE6M7
arcl task update AGD4 --parent 543N
arcl task update AGD4 --no-parent
```

`--spec`, `--plan`, `--phase` and `--parent` move a task, and `--no-spec`,
`--no-plan`, `--no-phase` and `--no-parent` take it out. Its subtasks move
with it. arcl refuses a move that creates a parent cycle, leaves a task
blocked by its own ancestor or descendant, or puts it under an archived task.

## Make it wait on another task

```sh
arcl dependency add 8GQ6 --blocked-by 7TR6
arcl dependency remove 8GQ6 --blocked-by 7TR6
```

Only a completed blocker releases the task. [Dependencies and readiness
rules](/reference/deps/) lists the edges arcl refuses.

## Start and claim it

```sh
arcl task start 7TR6 --claim
```

`--claim` makes you the task's assignee. Without a name it uses `ARCL_AGENT`,
then the Git user name; `--claim <NAME>` sets one. While the task has an
assignee, any other start is refused with exit code `4`.

To take the first ready task in one step:

```sh
arcl next --start --claim
```

`start` refuses a task that is not ready and lists the reasons. Having
subtasks is the one reason it ignores. `--force` starts the task anyway, but
does not override a claim.

## Record notes as you work

```sh
arcl task note 7TR6 --kind decision "Store notes in their own table"
arcl task note 7TR6 "The importer reads files in path order"
```

`--kind` is `decision`, `observation`, `blocker` or `result`, and is optional.
A note is plain text of at most 2000 characters and may span lines. `show`
lists every note, newest first, and `context` the 10 most recent. For longer
writing that links to several records, [keep a note
record](/how-to/keep-notes/).

## Wait on a pull request, run or date

A gate holds the task back until something outside arcl happens:

```sh
arcl gate add 7TR6 --pr https://github.com/acme/app/pull/42
arcl gate add 7TR6 --run https://github.com/acme/app/actions/runs/123456
arcl gate add 7TR6 --until 2026-11-20
```

```text
Added gate `VS7Q` to task `35WW`: waits until `2026-11-20`
```

Gates do not resolve on their own. Run `arcl gate check` to mark the met
ones, for one task or all:

```sh
arcl gate check
```

```text
Waiting: gate `VS7Q` on task `35WW`: waits until 2026-11-20; today is 2026-11-09 UTC
```

A date gate is met from the start of its date in UTC. Pull request and run
gates ask the GitHub CLI, `gh`, so it must be installed and signed in. Without
it they stay open and the check prints `gh not found; PR/run gates left open`.
A pull request closed without merging, or a failed run, is reported as
`failed` and stays open; remove it with `arcl gate remove <gate-id>` or add
another.

## Complete it

```sh
arcl task complete 7TR6 --evidence "All tests pass."
arcl task complete 7TR6 --evidence-file evidence.md
```

```text
Completed task `arcl-t-01M70M8FPN8SMXD6TMX2G3MJS2`: Add the key bindings table
Unblocked task `arcl-t-01M70M8FPNTS24NBFPXW9F35WW`: Move the selection with j and k
```

The evidence is Markdown; `--evidence-file -` reads standard input. arcl
stores it and does not run commands, open links or judge whether it proves
the work. The output names each task the completion made ready. Completing
also releases the claim.

## Complete a task with subtasks

A task with open subtasks refuses to complete:

```text
error: invalid project or record: stored planning task is invalid: cannot complete task `arcl-t-01M70MFTQ3Y88WHKJVE6NZGV2D` while it has non-terminal descendants; pass --allow-open-children to override
```

Complete the task and its pending and in-progress subtasks in one write:

```sh
arcl task complete GV2D --with-subtasks --evidence "Done."
```

```text
Completed task `arcl-t-01M70MFTQ3Y88WHKJVE6NZGV2D`: Parent
Completed subtask `arcl-t-01M70MFYZG6RZY91G1DEDR81TZ`: Child
```

The evidence goes on the named task only. The command writes nothing while a
subtask is parked, or while the task or an open subtask has an unfinished
blocker outside the subtree.

To close the parent alone and leave its subtasks open, pass
`--allow-open-children`. The two flags cannot be combined.

## Park or cancel it

```sh
arcl task park 7TR6
arcl task unpark 7TR6
arcl task cancel 7TR6
```

`park` sets pending or in-progress work aside and releases the claim.
`unpark` returns it to `pending`. `cancel` closes a task for good. To stop
partway and leave a note for the next session, [hand the task
off](/how-to/hand-off-and-resume/) instead.
