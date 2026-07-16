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
parent and child must remain in the same milestone.

Use `--parent` to reparent a task, or `--no-parent` to make it a top-level task:

```sh
arcl task update arcl-t-02J... --parent arcl-t-01J...
arcl task update arcl-t-02J... --no-parent
```

Cycles and invalid cross-milestone relationships are rejected before rows
change.

## Move work through its lifecycle

```sh
arcl task start arcl-t-01J...
arcl task park arcl-t-01J...
arcl task unpark arcl-t-01J...
arcl task complete arcl-t-01J...
arcl task cancel arcl-t-01J...
```

The allowed task statuses are:

| Status | Meaning |
| --- | --- |
| `pending` | Ready to be started. |
| `in_progress` | Work has started. |
| `parked` | Open work temporarily excluded from active readiness. |
| `completed` | Terminal success state. |
| `cancelled` | Terminal state for work that will not continue. |

`start` moves pending work to `in_progress`. `park` accepts pending or in-progress
work. `unpark` always returns parked work to `pending`. `complete` accepts
pending or in-progress work, and `cancel` accepts any non-terminal task.

## Close parent tasks

By default, a task cannot become terminal while a descendant is still open.
Pass `--allow-open-children` to close only the selected task:

```sh
arcl task complete arcl-t-01J... --allow-open-children
```

The override never cascades to child tasks.
