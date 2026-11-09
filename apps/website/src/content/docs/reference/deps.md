---
title: Dependencies and readiness rules
description: The conditions a ready task meets, the blocker edges arcl refuses, every readiness reason with its fields, and how inherited blockers print.
---

## Readiness rules

A task is ready when all of these hold:

- It is pending and has no child tasks, of any status.
- No ancestor task is parked, completed or cancelled.
- Its spec, plan and phase, where it has them, are open.
- Every blocker on it and on its ancestor tasks is completed.
- None of its blocker edges forms a cycle or joins it to its own ancestor or
  descendant.
- It has no open gate.

Only a completed blocker satisfies a dependency. Cancelled and parked
blockers keep the task blocked. A blocker on a parent task blocks every task
under it. A gate holds back only its own task, not its subtasks. An ancestor
blocked by the task itself does not block it.

`ready` lists ready tasks by priority, then position, then ID. `next` prints
the first, or an empty result when nothing is ready. Both take `--priority`,
`--spec`, `--plan`, `--phase` and `--parent`.

`task start` refuses a task that is not ready, except for `has_children`, and
lists the reasons. `--force` starts it anyway.

## Dependency commands

```sh
arcl dependency add <task-id> --blocked-by <blocker-id>
arcl dependency add <task-id> --blocked-by <a>,<b>
arcl task create "<title>" --blocked-by <a> --blocked-by <b>
arcl dependency remove <task-id> --blocked-by <blocker-id>
arcl dependency list
```

In `dependency add` and `task create`, repeat `--blocked-by` or separate IDs
with commas to add several blockers in one write. If one is refused, none are
added. `dependency remove` takes one ID.

## Refused edges

`dependency add`, `task create --blocked-by` and `plan apply` refuse a blocker
edge that:

- closes a cycle;
- joins a task to its own ancestor or descendant;
- touches an archived task.

`task update --parent` and `plan apply` also refuse a move after which a task
in the moved subtree is blocked by a new ancestor, or blocks one.

The error lists the task IDs along the cycle, or the parent chain from the
descendant up to the ancestor, exits `3`, and writes nothing. Plan errors name
tasks the plan would create by their plan path. `plan check` and `plan diff`
report the same errors as `plan apply`.

A bad edge already stored elsewhere in the project does not block an
unrelated write. Workspace sync and import load bad edges the database
already holds, with a `blocker_edge` warning. Import loads all of them that
way when the project has never synced or its database holds no records, as in
a fresh clone.

## How check reports bad edges

A database can hold edges that `dependency add` refuses, such as ones edited
in by hand. `arcl check` reports each as an error:

- A cycle, once, starting at the smallest task ID:
  `blocker cycle: <a> -> <b> -> <a>; each task is blocked by the next`. A task
  blocked by itself is `<a> -> <a>`.
- An edge to the task's own ancestor or descendant, with the parent chain
  from the descendant up to the ancestor:
  ``task `<a>` is blocked by `<b>`, its ancestor; parent chain: <a> -> <b>``.

A task whose own edge is part of one is not ready, and `explain` gives the
same text as a reason. Remove the edge with `dependency remove`.

## Readiness reasons

`explain` lists every reason a task is not ready. In JSON each reason has a
`kind`, the `message` that human and plain output print, and the ID fields its
kind uses. Reasons follow a fixed order: the task's own blockers, then each
ancestor's, nearest first.

| `kind`              | Condition                                                                   | ID fields                                |
| ------------------- | --------------------------------------------------------------------------- | ---------------------------------------- |
| `status`            | The task is not pending.                                                    | `task`                                   |
| `has_children`      | The task has child tasks.                                                   | `task`                                   |
| `closed_ancestor`   | An ancestor task is parked, completed, or cancelled.                        | `ancestor`                               |
| `closed_container`  | The spec, plan, or phase the task is under is not open.                     | `container`                              |
| `direct_blocker`    | One of the task's blockers is unfinished.                                   | `blocker`                                |
| `inherited_blocker` | A blocker held by an ancestor is unfinished.                                | `ancestor`, `blocker`                    |
| `cancelled_blocker` | A blocker was cancelled. Remove or replace the dependency.                  | `blocker`, and `ancestor` when inherited |
| `missing_blocker`   | A blocker task does not exist.                                              | `blocker`, and `ancestor` when inherited |
| `invalid_edge`      | A blocker edge forms a cycle or joins a task to its ancestor or descendant. | `task`, `blocker`, `path`                |
| `open_gate`         | The task has a gate that `arcl gate check` has not found met.               | `gate`                                   |

`task`, `ancestor` and `blocker` are task IDs. `container` is a spec, plan or
phase ID. For an `invalid_edge` cycle, `path` lists the cycle with its first
task repeated at the end, and `task` to `blocker` is the edge to remove. For
an edge that joins a task to its ancestor, `task` and `blocker` are that edge
and `path` is the parent chain from the descendant up to the ancestor. `gate`
is the gate itself, an object with `id`, `task_id`, `kind`, `target`, `status`
and `created_at`.

## Inherited blockers in output

`explain` and `context` list blockers held by ancestor tasks apart from direct
blockers, nearest ancestor first, each with the ancestor that holds it.

- JSON: `blockers` holds direct blockers only. `inherited_blockers` holds the
  inherited ones, each with an `ancestor` field.
- Plain: `explain` prints `inherited\t<blocker>\t<ancestor>` and `context`
  prints `inherited_blocker\t<blocker>\t<status>\t<ancestor>`. The `blockers`
  and `blocker` records stay direct only.
- Human: inherited blockers end with `via <ancestor>`.

`completion_evidence` in `context` draws on both lists and names each blocker
once.
