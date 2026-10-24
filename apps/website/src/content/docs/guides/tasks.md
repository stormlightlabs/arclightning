---
title: Tasks
description: Create, organize, and move work through its explicit lifecycle.
---

Tasks are the smallest tracked work items in Arc Lightning. A task belongs to a
milestone and may have a parent task. Subtasks are ordinary task rows with their
own IDs and lifecycle state.

## Create a task

```sh
arcl task create "Add schema" \
  --milestone arcl-m-01J... \
  --priority high \
  --position 10 \
  --description "Create the tables needed by the tracker."
```

Valid priorities are `critical`, `high`, `normal`, and `low`. The default is
`normal`. Position values are non-negative integers used to order sibling work.

Attach a subtask to a parent task in the same milestone:

```sh
arcl task create "Add migration test" \
  --milestone arcl-m-01J... \
  --parent arcl-t-01J... \
  --description-file migration-test.md
```

## Update and reorganize tasks

```sh
arcl task update arcl-t-01J... --title "Add the initial schema"
arcl task update arcl-t-01J... --priority critical --position 20
arcl task update arcl-t-01J... --milestone arcl-m-02J...
```

Moving a task to another milestone moves its complete descendant subtree. A
parent and child must belong to the same milestone.

Use `--parent` to reparent a task, or `--no-parent` to make it a top-level task:

```sh
arcl task update arcl-t-02J... --parent arcl-t-01J...
arcl task update arcl-t-02J... --no-parent
```

Cycles, invalid cross-milestone relationships, and moves after which a task is
blocked by its own ancestor or descendant are rejected before rows change.

## Move work through its lifecycle

```sh
arcl task start arcl-t-01J...
arcl task park arcl-t-01J...
arcl task unpark arcl-t-01J...
arcl task complete arcl-t-01J...
arcl task cancel arcl-t-01J...
```

The allowed task statuses are:

| Status        | Meaning                                               |
| ------------- | ----------------------------------------------------- |
| `pending`     | Ready to be started.                                  |
| `in_progress` | Work has started.                                     |
| `parked`      | Open work temporarily excluded from active readiness. |
| `completed`   | Terminal success state.                               |
| `cancelled`   | Terminal state for work that will not continue.       |

`start` moves pending work to `in_progress` when it is ready, apart from having
child tasks. Any other reason that `arcl task explain` reports, such as an
unfinished blocker, refuses the start and lists the reasons; `--force` starts the
task anyway. Starting a task that is already in progress does nothing. `park`
accepts pending or in-progress work. `unpark` always returns parked work to
`pending`. `complete` accepts pending or in-progress work, and `cancel` accepts
any non-terminal task.

## Close parent tasks

By default, a task cannot become terminal while a descendant is still open.
Pass `--allow-open-children` to close only the selected task:

```sh
arcl task complete arcl-t-01J... --allow-open-children
```

The override never cascades to child tasks.

Pass `--with-subtasks` to complete the task and every pending or in-progress
descendant in one write:

```sh
arcl task complete arcl-t-01J... --with-subtasks --evidence "Shipped in #42."
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
arcl task handoff arcl-t-01J... --note "Resume at the migration test."
arcl task handoff arcl-t-01J... --note-file handoff.md
```

`handoff` stores the note and parks the task in one transaction. Unparking and
starting the task preserve the note until another handoff replaces it. Use
`--note-file -` to read UTF-8 Markdown from standard input. After `unpark`,
`start` refuses a task that became not ready while parked; resolve the blocker
or pass `--force`.

## Record Completion

Attach optional Markdown evidence when completing a task:

```sh
arcl task complete arcl-t-01J... --evidence "All tests pass."
arcl task complete arcl-t-01J... --evidence-file evidence.md
```

Use `--evidence-file -` to read from standard input. Arc Lightning stores the
evidence with the terminal transition; it does not run commands, open links,
or judge whether the evidence proves completion.

`arcl show` and `arcl context` include the current handoff and relevant blocker
evidence. JSON and plain output expose the same information for agents and scripts.
