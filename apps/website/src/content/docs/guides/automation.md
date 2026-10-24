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
`arcl skill install` writes the copy built into `arcl` into the project's
`.claude/skills/` and `.agents/skills/`, and `--check` reports drift without
writing:

```sh
arcl skill install
```

The [skills CLI](https://skills.sh) installs it from GitHub instead:

```sh
npx skills add stormlightlabs/arclightning --skill arclightning
```

`--skill arclightning` picks it out of the other skills this repository keeps
for its own development.

## Connect the MCP server

`arcl-mcp` serves the same operations as typed MCP tools over standard input
and output, with no CLI text to parse. Install it, then have the MCP client
start it in the project directory. For Claude Code:

```sh
cargo install --git https://github.com/stormlightlabs/arclightning arcl-mcp
claude mcp add arcl -- arcl-mcp
```

Each tool calls the application operations directly and syncs the workspace
around the call, as a command does.

| Tool             | Command          | `format_version`              |
| ---------------- | ---------------- | ----------------------------- |
| `show`           | `show`           | `2` for a task, `1` otherwise |
| `list`           | `list`           | `1`                           |
| `ready`          | `ready`          | `1`                           |
| `explain`        | `explain`        | `2`                           |
| `context`        | `context`        | `2`                           |
| `capture_create` | `capture create` | `1`                           |
| `spec_create`    | `spec create`    | `1`                           |
| `plan_create`    | `plan create`    | `1`                           |
| `task_create`    | `task create`    | `1`                           |
| `task_update`    | `task update`    | `1`                           |
| `note_create`    | `note create`    | `1`                           |
| `task_start`     | `task start`     | `1`                           |
| `task_handoff`   | `task handoff`   | `1`                           |
| `task_unpark`    | `task unpark`    | `1`                           |
| `task_complete`  | `task complete`  | `2`                           |

A tool's structured content is the command's envelope: `format_version`,
`data`, and a `warnings` array of `code` and `message` objects when the call
went on despite a problem, such as a workspace sync that did not run. Each
tool lists an input and an output JSON Schema, and its `_meta` names its
version as `arcl/format_version`. A change to either schema comes with a new
version. A failure, including arguments the input schema rejects, is an error
result whose structured content is the version 1 failure object above.

`list` and `ready` return at most `limit` records, from 1 to 200 and 50 by
default. While more remain, `data.next_cursor` holds the `cursor` for the next
page. `list` pages by kind, then ID, so new records do not shift later pages.
`ready` pages by each task's place in the ready order (priority, position,
then ID), so a task that starts between pages does not hide the next one. A
malformed `cursor` fails with `invalid_request`.

`task_start` has no force option. A task that is not ready fails with
`invalid_request`, and `explain` gives the typed reasons.

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
