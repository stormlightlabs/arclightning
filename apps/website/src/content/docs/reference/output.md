---
title: Output formats and exit codes
description: The four output modes, the JSON envelope and its format versions, warnings, failure objects and every exit code.
---

## Output modes

| Mode      | Prints                                                                              |
| --------- | ----------------------------------------------------------------------------------- |
| human     | Concise text that may use ANSI colors. The default; it can change between releases. |
| `--plain` | One stable, unstyled, tab-separated record per line.                                |
| `--json`  | One versioned JSON envelope.                                                        |
| `--quiet` | Nothing on success.                                                                 |

The flags are global and go before or after the subcommand. `--json` cannot
be combined with `--plain` or `--quiet`. Errors always go to standard error.

Human output prints IDs as short IDs. `--plain` and `--json` print full IDs.

Color appears only when standard output is a terminal and `NO_COLOR` is unset.
`FORCE_COLOR` or `CLICOLOR_FORCE` turns it on, and `--color
auto|always|never` overrides every check.

## JSON envelope

```json
{"format_version":1,"data":{"action":"created","capture":{"id":"arcl-c-…","title":"Automate this","status":"captured"}}}
```

`data` is specific to the command. For a change, it holds the action and the
affected record. Statuses, kinds and priorities are lowercase strings, such
as `in_progress`.

## Format versions

A command's `format_version` changes when a field in its `data` is renamed,
removed or given a new type. New fields can appear without a new version.

| Version | Commands                                                                                             |
| ------- | ---------------------------------------------------------------------------------------------------- |
| `2`     | `show <task-id>`, `task show`, `explain`, `task explain`, `context`, `task context`, `task complete` |
| `1`     | Every other command, failures, and sync warnings                                                     |

Version 2 made each readiness reason an object and added `unblocked` to
`task complete`. Releases before 1.0 can change JSON shapes; the version names
the shape each envelope carries.

## Readiness

Task views and context packets carry `data.readiness`:

| Field     | Type    | Meaning                                                                                              |
| --------- | ------- | ---------------------------------------------------------------------------------------------------- |
| `ready`   | boolean | `reasons` is empty, and the `ready` command lists the task.                                          |
| `blocked` | boolean | A blocker of the task or of an ancestor is unfinished or missing, or the task has an open gate.      |
| `reasons` | array   | Every condition that keeps the task from being ready, in an order that does not change between runs. |

[Dependencies and readiness rules](/reference/deps/#readiness-reasons) lists
each reason's `kind` and fields.

## Completion results

`task complete` returns the completed `task` and two lists of task IDs:

- `completed_subtasks`: the descendants `--with-subtasks` completed, in tree
  order, or empty.
- `unblocked`: tasks that were not ready before the command and are ready
  after it, in `ready` order, or empty.

Plain output prints the task ID, then a `subtask<TAB><id>` line per completed
subtask and an `unblocked<TAB><id>` line per task it made ready.

## Warnings

A command can finish despite a problem, such as a workspace sync that did not
run before a read. Human output prints a `warning:` line on standard error.
Under `--json`, the warnings go to standard error as one line of JSON whose
`warnings` array holds a `code` and a `message` for each. `--quiet` prints
none.

| Code             | Meaning                                                                                                  |
| ---------------- | -------------------------------------------------------------------------------------------------------- |
| `workspace_sync` | A sync or export did not run, and the command went on.                                                   |
| `blocker_edge`   | A sync or import loaded a blocker edge that forms a cycle or joins a task to its ancestor or descendant. |
| `capture_source` | `capture create` found several tasks claimed by `ARCL_AGENT`, so the capture has no source.              |

## Failures

Under `--json`, a failure goes to standard error as a version 1 object:

```json
{"error":{"code":"not_found","exit_code":5,"message":"no record matches short ID `nope`"},"format_version":1}
```

| `code`              | Exit code | Meaning                                                                               |
| ------------------- | --------- | ------------------------------------------------------------------------------------- |
| `application_error` | `1`       | A runtime, filesystem, Git or SQLite failure.                                         |
| `invalid_request`   | `2`, `3`  | A command line that does not parse, or invalid input, record or lifecycle transition. |
| `workspace_invalid` | `3`       | `arcl check` found workspace content it cannot read or accept.                        |
| `conflict`          | `4`       | A claim, a workspace conflict or a concurrent change.                                 |
| `not_found`         | `5`       | No record matches the ID.                                                             |

## Exit codes

| Code | Meaning                                                                                       |
| ---- | --------------------------------------------------------------------------------------------- |
| `0`  | Success.                                                                                      |
| `1`  | Runtime, filesystem, Git operation, or SQLite failure.                                        |
| `2`  | Command-line parsing failure reported by `clap`.                                              |
| `3`  | Invalid project, record, input, or lifecycle transition.                                      |
| `4`  | Workspace or concurrent-modification conflict, skill drift, or an unconfirmed `setup` change. |
| `5`  | Requested record was not found.                                                               |
