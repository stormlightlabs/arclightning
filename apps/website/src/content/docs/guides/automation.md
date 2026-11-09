---
title: Automation
description: Read arcl from scripts with versioned JSON envelopes, typed readiness reasons, plain output and stable exit codes.
---

Human output is for people and can change between releases. Scripts and
coding agents choose an output mode with a stable shape: `--json`, `--plain`
or `--quiet`. Agents that speak MCP can use the [MCP server](/guides/mcp/)
instead, which returns the same envelopes as typed tool results.

<div id="give-an-agent-the-skill"></div>
<div id="prime-each-session"></div>
<div id="connect-the-mcp-server"></div>

[Agent workflows](/guides/agents/) covers installing the skill and
[priming each session](/guides/agents/#prime-each-session).
[MCP server](/guides/mcp/#install-and-connect) covers connecting the server.

## Use versioned JSON

Pass `--json` before or after the command:

```sh
arcl --json capture create "Automate this" --body "Details"
```

```json
{"format_version":1,"data":{"action":"created","capture":{"body":"Details","created_at":"2026-11-09T03:25:04.536Z","id":"arcl-c-01M6Z5D24RMB67SSTH6HG0NVKG","project_id":"arcl-pj-00000000000000000000000000","status":"captured","title":"Automate this"}}}
```

The response is one JSON object with a `format_version` and a `data` object.
For a change, `data` holds the action and the affected record.

Use JSON when the consumer needs named fields rather than text intended for a
terminal.

## Check the format version

A command's `format_version` changes when a field in its `data` is renamed,
removed or given a new type. New fields can appear without a new version, so
ignore fields you don't know. Check the version before reading fields.

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
| `blocked` | boolean | A blocker of the task or of an ancestor is unfinished or missing, or the task has an open gate.      |
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
| `open_gate`         | The task has a gate that `arcl gate check` has not found met.               | `gate`                                   |

`task`, `ancestor`, and `blocker` are task IDs. `container` is a spec, plan, or
phase ID. For an `invalid_edge` cycle, `path` lists the cycle with its first
task repeated at the end, and `task` to `blocker` is the edge to remove. For an
edge that joins a task to its ancestor, `task` and `blocker` are that edge and
`path` is the parent chain from the descendant up to the ancestor.
`gate` is the gate itself, an object with `id`, `task_id`, `kind`, `target`,
`status` and `created_at`.

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

## Use plain or quiet output

`--plain` emits one stable, unstyled record per line:

```sh
arcl --plain tree
```

```text
0	spec	arcl-s-01M6Z52GCZF3KWGK2JPHSPJ7TV	open	Add keyboard shortcuts
1	plan	arcl-pl-01M6Z52GDTQFRHBSTMT6FK17MR	open	Keyboard plan
2	phase	arcl-ph-01M6Z52GDTM597QMZ1VWYS39QD	open	Key handling
3	task	arcl-t-01M6Z52GDTA3RG3BF0KDD2KAJE	completed	Add the key bindings table
3	task	arcl-t-01M6Z52GDT6HNSB2K69ZD3XG0Y	pending	Move the selection with j and k
```

`--quiet` prints nothing on success, for a step whose exit code is all a
script needs:

```sh
arcl --quiet task complete KAJE
```

These modes keep shell pipelines and agent tool calls free of terminal styling
and incidental status messages.

## Keep failures machine-readable

Under `--json`, a failure goes to standard error as a version 1 object with
a `code`, the `exit_code` and a `message`:

```sh
arcl --json task start nope
```

```json
{"error":{"code":"not_found","exit_code":5,"message":"no record matches short ID `nope`"},"format_version":1}
```

Branch on the exit code. The [manual](/reference/manual/#status-codes) lists
every code.
