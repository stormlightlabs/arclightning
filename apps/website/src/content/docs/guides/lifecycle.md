---
title: Lifecycle
description: The statuses each record kind moves through, which moves are allowed, and how closing a container treats the work below it.
---

Every status change in Arc Lightning is an explicit command. Completing and
cancelling are final for every kind of record. Repeating the same final
command does nothing; switching a completed record to cancelled, or the
reverse, is refused.

## Statuses by kind

| Kind                       | Statuses                                                     |
| -------------------------- | ------------------------------------------------------------ |
| Capture                    | `captured`, then `promoted` or `discarded`                   |
| Spec, plan, phase, release | `open`, then `completed` or `cancelled`                      |
| Task                       | `pending`, `in_progress`, `parked`, `completed`, `cancelled` |
| Note                       | None                                                         |

## Move tasks

```sh
arcl task start 7TR6
arcl task park 7TR6
arcl task unpark 7TR6
arcl task complete 7TR6
arcl task cancel 7TR6
```

| Command    | From                       | To            |
| ---------- | -------------------------- | ------------- |
| `start`    | `pending`                  | `in_progress` |
| `park`     | `pending`, `in_progress`   | `parked`      |
| `handoff`  | `in_progress`              | `parked`      |
| `unpark`   | `parked`                   | `pending`     |
| `complete` | `pending`, `in_progress`   | `completed`   |
| `cancel`   | any status but a final one | `cancelled`   |

`start` refuses a task that is not ready, for any reason other than having
subtasks, and lists the reasons; `--force` starts it anyway. A parked task
must be unparked before it completes. No command reopens a completed or
cancelled task. To reopen one, edit its workspace file, as the
[manual](/reference/manual/#task-commands) describes.

The [tasks guide](/guides/tasks/) covers claims, handoffs, evidence and
completing a task with its subtasks.

## Close containers

Specs, plans, phases and releases close with `complete` or `cancel`:

```sh
arcl plan phase complete KSX8
arcl plan cancel Z234
arcl spec complete FVMC
arcl release complete SVY2C
```

A container, or a parent task, refuses to close while any record below it is
open. `--allow-open-children` closes only the record you name. It never
cascades: open work below stays open, so you can review it later.

Work under a closed spec, plan or phase is held back. Its tasks are no longer
ready, `arcl explain` gives the reason `closed_container`, and a closed
container takes no new tasks. Release membership does not hold work back, so
completing a release changes no task's readiness.

To finish a task and its open subtasks together, use
`arcl task complete <id> --with-subtasks`.
