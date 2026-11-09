---
title: MCP server
description: Connect arcl-mcp to a coding agent so it reads and changes records through typed MCP tools instead of parsing CLI output.
---

`arcl-mcp` is an MCP server that serves Arc Lightning's operations as typed
tools over standard input and output. An agent gets each command's JSON
envelope as structured content, with an input and an output schema per tool,
and no terminal text to parse.

## Install and connect

Install `arcl-mcp` from the release, then have the MCP client start it in the
project directory. For Claude Code:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/stormlightlabs/arclightning/releases/latest/download/arcl-mcp-installer.sh | sh
claude mcp add arcl -- arcl-mcp
```

On Windows, install it with `arcl-mcp-installer.ps1` from the same release, as
the [Quick Start](/quick-start/) shows for `arcl`.
`cargo install --git https://github.com/stormlightlabs/arclightning arcl-mcp`
compiles it instead.

`arcl-mcp` takes no arguments. It serves the project that holds its working
directory until the client closes standard input, so any MCP client that can
start a command in the project directory can use it.

The MCP server and `arcl setup` work together. `arcl setup` writes the
[skill](/guides/agents/#the-skill) and the session hook; the skill tells the
agent to call the tools when the server is connected, and names the tool for
each step. The server's instructions also tell the client to call `prime` at
the start of a session.

## How tools run

Each tool calls the application operations directly and syncs the workspace
around the call, as a command does. The read tools carry `readOnlyHint`
because they change no records, but with the workspace on, a read first
imports workspace changes into the database and exports the result.

A call waits up to 10 seconds for an `arcl` command in the same project to
finish, then fails with `application_error` (exit code `1`) naming the
`.arcl/arcl.db-sync.lock` file. `prime` is the exception: after 2 seconds it
returns the database as it is with a `workspace_sync` warning, as `arcl prime`
does. A call the client cancels while it waits changes nothing. Once its
operation starts, a write runs to the end.

## Tools

Each tool matches one command and returns that command's `format_version`:

| Tool              | Command           | `format_version`              |
| ----------------- | ----------------- | ----------------------------- |
| `prime`           | `prime`           | `1`                           |
| `show`            | `show`            | `2` for a task, `1` otherwise |
| `list`            | `list`            | `1`                           |
| `ready`           | `ready`           | `1`                           |
| `explain`         | `explain`         | `2`                           |
| `context`         | `context`         | `2`                           |
| `capture_create`  | `capture create`  | `1`                           |
| `capture_promote` | `capture promote` | `1`                           |
| `spec_create`     | `spec create`     | `1`                           |
| `plan_create`     | `plan create`     | `1`                           |
| `task_create`     | `task create`     | `1`                           |
| `task_update`     | `task update`     | `1`                           |
| `note_create`     | `note create`     | `1`                           |
| `task_start`      | `task start`      | `1`                           |
| `task_handoff`    | `task handoff`    | `1`                           |
| `task_note`       | `task note`       | `1`                           |
| `gate_add`        | `gate add`        | `1`                           |
| `gate_list`       | `gate list`       | `1`                           |
| `gate_check`      | `gate check`      | `1`                           |
| `task_unpark`     | `task unpark`     | `1`                           |
| `task_complete`   | `task complete`   | `2`                           |

A tool's structured content is the command's envelope: `format_version`,
`data`, and a `warnings` array of `code` and `message` objects when the call
went on despite a problem, such as a workspace sync that did not run. Each
tool lists an input and an output JSON Schema, and its `_meta` names its
version as `arcl/format_version`. Renaming, removing or retyping a field in
either schema comes with a new version; a new field does not. A failure,
including arguments the input schema rejects, is an error result whose
structured content is the version 1 failure object that
[`--json` failures](/guides/automation/#keep-failures-machine-readable) use.

## Prime

`prime` takes no arguments and returns what `arcl --json prime` prints: the
working loop as Markdown in `data.loop`, then `data.open` and `data.ready`,
each with up to 5 tasks and a `more` count. Titles are cut to 200 bytes and
handoffs to 400, each ending with `… [truncated]` when cut, so the result
stays near 10 KiB. Outside an initialized project it succeeds with
`data.initialized` false, `data.root` and a `data.message` that says to run
`arcl init`.

## Pages and filters

`list` and `ready` return at most `limit` records, from 1 to 200 and 50 by
default. While more remain, `data.next_cursor` holds the `cursor` for the next
page. `list` pages by kind, then ID, so new records do not shift later pages.
`ready` pages by each task's place in the ready order (priority, position,
then ID), so a task that starts between pages does not hide the next one. A
malformed `cursor` fails with `invalid_request`.

`list` takes the filters of `arcl list` other than `--assignee`: `kind`,
`status`, `priority`, `release`, `spec`, `plan`, `phase`, `parent` and `all`.
IDs may be full or short. Each record filter matches:

- `spec`: the spec, and the plans and tasks under it.
- `plan`: the plan, and the phases and tasks under it.
- `phase`: the phase and the tasks under it.
- `release`: the release, and the specs, plans, tasks and notes in it.
- `parent`: direct subtasks of the task.

Every filter given must match. Notes have no status, so `status` leaves them
out. The checks run in the command's order and fail as it does: a status or
priority the `kind` cannot have, or an ID of another kind, fails with
`invalid_request`, and an ID that names no record fails with `not_found`.

## Starting tasks

`task_start` has no force option, so an agent asks a person before starting
a task that is not ready. A task that is not ready fails with
`invalid_request`, and `explain` gives the typed reasons. `claim` takes a
name, as `arcl task start --claim <name>` does, and a task another name holds
fails with `conflict`.
