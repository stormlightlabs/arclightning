---
title: MCP tools
description: The 21 tools arcl-mcp serves, their arguments and format versions, and the rules for results, paging, filters, locking and failures.
---

`arcl-mcp` serves Arc Lightning's operations as MCP tools over standard input
and output. It takes no arguments, serves the project that holds its working
directory, and runs until the client closes standard input. [Connect the MCP
server](/how-to/connect-mcp/) installs it.

## Tools

Each tool matches one command and returns that command's `format_version`.

| Tool              | Command           | Arguments                                                                                                                 | Version                       |
| ----------------- | ----------------- | ------------------------------------------------------------------------------------------------------------------------- | ----------------------------- |
| `prime`           | `prime`           | none                                                                                                                      | `1`                           |
| `show`            | `show`            | `id`                                                                                                                      | `2` for a task, `1` otherwise |
| `list`            | `list`            | `kind`, `status`, `priority`, `release`, `spec`, `plan`, `phase`, `parent`, `all`, `limit`, `cursor`                      | `1`                           |
| `ready`           | `ready`           | `priority`, `spec`, `plan`, `phase`, `parent`, `limit`, `cursor`                                                          | `1`                           |
| `explain`         | `explain`         | `task_id`                                                                                                                 | `2`                           |
| `context`         | `context`         | `task_id`                                                                                                                 | `2`                           |
| `capture_create`  | `capture create`  | `title`, `body`, `from`                                                                                                   | `1`                           |
| `capture_promote` | `capture promote` | `capture_id`, `target`, `title`, `body`, `acceptance_criteria`, `spec`, `plan`, `phase`, `parent`, `priority`, `position` | `1`                           |
| `spec_create`     | `spec create`     | `title`, `body`, `acceptance_criteria`                                                                                    | `1`                           |
| `plan_create`     | `plan create`     | `title`, `spec`, `body`                                                                                                   | `1`                           |
| `task_create`     | `task create`     | `title`, `body`, `spec`, `plan`, `phase`, `parent`, `priority`, `position`, `blocked_by`                                  | `1`                           |
| `task_update`     | `task update`     | `task_id`, `title`, `body`, `priority`, `position`                                                                        | `1`                           |
| `note_create`     | `note create`     | `title`, `body`                                                                                                           | `1`                           |
| `task_start`      | `task start`      | `task_id`, `claim`                                                                                                        | `1`                           |
| `task_handoff`    | `task handoff`    | `task_id`, `note`                                                                                                         | `1`                           |
| `task_note`       | `task note`       | `task_id`, `text`, `kind`                                                                                                 | `1`                           |
| `gate_add`        | `gate add`        | `task_id`, `kind` (`pr`, `run` or `date`), `target`                                                                       | `1`                           |
| `gate_list`       | `gate list`       | `task_id`                                                                                                                 | `1`                           |
| `gate_check`      | `gate check`      | `task_id`                                                                                                                 | `1`                           |
| `task_unpark`     | `task unpark`     | `task_id`                                                                                                                 | `1`                           |
| `task_complete`   | `task complete`   | `task_id`, `evidence`, `open_subtasks`                                                                                    | `2`                           |

`task_complete` takes `open_subtasks` as `refuse` (the default), `leave`, like
`--allow-open-children`, or `complete`, like `--with-subtasks`.

`task_start` has no force option. A task that is not ready fails with
`invalid_request`, and `explain` gives the typed reasons, so an agent asks a
person before starting it. `claim` takes a name, as `--claim <NAME>` does,
and a task another name holds fails with `conflict`.

The read tools are `prime`, `show`, `list`, `ready`, `explain`, `context` and
`gate_list`. They carry `readOnlyHint` because they change no records. With
the workspace on, a read still imports workspace changes and exports the
result, as a command does.

The server's instructions tell the client to call `prime` at the start of a
session.

## Results

A tool's structured content is the command's envelope: `format_version`,
`data`, and a `warnings` array of `code` and `message` objects when the call
went on despite a problem, such as a workspace sync that did not run.

Each tool lists an input and an output JSON Schema, and its `_meta` names its
version as `arcl/format_version`. Renaming, removing or retyping a field in
either schema comes with a new version; a new field does not.

A failure, including arguments the input schema rejects, is an error result.
Its structured content is the version 1 failure object, as in [Output formats
and exit codes](/reference/output/#failures).

## prime

`prime` returns what `arcl --json prime` prints: the working loop as Markdown
in `data.loop`, then `data.open` and `data.ready`, each with up to 5 tasks and
a `more` count. Titles are cut to 200 bytes and handoffs to 400, each ending
with `… [truncated]` when cut, so the result stays near 10 KiB. Outside an
initialized project it succeeds with `data.initialized` false, `data.root`
and a `data.message` that says to run `arcl init`.

## Paging

`list` and `ready` return at most `limit` records, from 1 to 200 and 50 by
default. While more remain, `data.next_cursor` holds the `cursor` for the next
page. A malformed `cursor` fails with `invalid_request`.

`list` pages by kind, then ID, so new records do not shift later pages.
`ready` pages by each task's place in the ready order (priority, position,
then ID), so a task that starts between pages does not hide the next one.

## Filters

`list` takes the filters of `arcl list` other than `--assignee`. IDs may be
full or short. Each record filter matches:

| Filter    | Matches                                                  |
| --------- | -------------------------------------------------------- |
| `spec`    | the spec, and the plans and tasks under it               |
| `plan`    | the plan, and the phases and tasks under it              |
| `phase`   | the phase and the tasks under it                         |
| `release` | the release, and the specs, plans, tasks and notes in it |
| `parent`  | direct subtasks of the task                              |

Every filter given must match. Notes have no status, so `status` leaves them
out. A status or priority the `kind` cannot have, or an ID of another kind,
fails with `invalid_request`. An ID that names no record fails with
`not_found`.

## Locking and cancellation

Each call runs the application operations directly and syncs the workspace
around the call, as a command does. A call waits up to 10 seconds for an
`arcl` command in the same project to finish, then fails with
`application_error` (exit code `1`) naming the `.arcl/arcl.db-sync.lock` file.

`prime` waits 2 seconds instead, then returns the database as it is with a
`workspace_sync` warning, as `arcl prime` does.

A call the client cancels while it waits changes nothing. Once its operation
starts, a write runs to the end.
