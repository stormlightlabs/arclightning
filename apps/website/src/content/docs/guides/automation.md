---
title: Automation
description: Consume Arc Lightning mutations from scripts and agents.
---

Arc Lightning keeps mutation output short by default. Choose an explicit output
mode when a script or coding agent needs a stable interface.

## Use versioned JSON

Pass `--json` before or after the command:

```sh
arcl --json idea create "Automate this" --description "Details"
```

The response is a versioned JSON object with a `format_version` and a `data`
object. For a mutation, `data` holds the action and the affected record fields.
For an idea, those fields include `id`, `title`, `description`, and `status`.

Use JSON when the consumer needs named fields rather than text intended for a
terminal.

## Check the format version

A command's `format_version` changes when the shape of its `data` changes. Check
it before reading fields.

| Version | Commands                                                                                                    |
| ------- | ----------------------------------------------------------------------------------------------------------- |
| `2`     | `show <task-id>`, `task show`, `explain`, `task explain`, `context`, `task context`, `task complete`        |
| `1`     | Every other command, including `ready`, `next`, `task start`, failures on standard error, and sync warnings |

Version 2 made each readiness reason an object and added `unblocked` to
`task complete`.

## Read readiness reasons

Task views and context packets carry `data.readiness`:

| Field     | Type    | Meaning                                                                                              |
| --------- | ------- | ---------------------------------------------------------------------------------------------------- |
| `ready`   | boolean | `reasons` is empty, and the `ready` command lists the task.                                          |
| `blocked` | boolean | A blocker of the task or of an ancestor is unfinished or missing.                                    |
| `reasons` | array   | Every condition that keeps the task from being ready, in an order that does not change between runs. |

Each reason has a `kind`, the `message` that human and plain output print, and
the IDs of the records it names. A reason leaves out the ID fields its kind
does not use.

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

`task`, `ancestor`, and `blocker` are task IDs. `container` is a spec, plan, or
phase ID. For an `invalid_edge` cycle, `path` lists the cycle with its first
task repeated at the end, and `task` to `blocker` is the edge to remove. For an
edge that joins a task to its ancestor, `task` and `blocker` are that edge and
`path` is the parent chain from the descendant up to the ancestor.

## Chain from completed work

`task complete` returns the completed `task` and two lists of task IDs:

- `completed_subtasks`: the descendants that `--with-subtasks` completed, in
  tree order. Empty otherwise.
- `unblocked`: tasks that were not ready before the command and are ready after
  it, in `ready` order.

A loop can run `ready`, start a task, complete it, and start a task from
`unblocked` next. `task start` refuses a pending task that is not ready for
any reason other than `has_children`, so a parent can start while its children
are open. A refusal exits with status `3` and writes a version 1 failure with
`code: "invalid_request"`. Its `message` lists the reasons as text. Read the
typed reasons from `explain` instead. `--force` starts the task anyway.

## Give an agent the skill

The Arc Lightning skill teaches a coding agent the loop above with `--json`.
Install it into a project with the [skills CLI](https://skills.sh):

```sh
npx skills add stormlightlabs/arclightning --skill arclightning
```

`--skill arclightning` picks it out of the other skills this repository keeps
for its own development.

## Use plain or quiet output

`--plain` emits one stable, unstyled record per line:

```sh
arcl --plain idea list
```

`--quiet` suppresses explanatory output and prints only the affected ID for a
mutation:

```sh
arcl --quiet task complete arcl-t-01J...
```

These modes keep shell pipelines and agent tool calls free of terminal styling
and incidental status messages.

## Keep failures machine-readable

JSON mode writes structured failures to standard error. Use the exit status to
distinguish a successful command from a parsing error, invalid project or
record, storage failure, or missing record. See the [manual](/reference/manual/)
for the complete output and exit-status reference.
