---
title: Manual
description: Complete command and output reference for the arcl CLI.
---

`arcl` operates on the enclosing non-bare Git worktree. Run `arcl --help` or
`arcl <command> --help` for the generated command help.

## Options

These options can appear before or after a subcommand & are global:

| Option                        | Purpose                                                                          |
| ----------------------------- | -------------------------------------------------------------------------------- |
| `--color auto\|always\|never` | Choose when human output may use ANSI colors. Defaults to `auto`.                |
| `--json`                      | Emit a versioned JSON envelope. Mutually exclusive with `--plain` and `--quiet`. |
| `--plain`                     | Emit one stable, unstyled record per line.                                       |
| `--quiet`                     | Suppress explanatory output and print only affected IDs for mutations.           |
| `--help`                      | Print help for the current command.                                              |
| `--version`                   | Print the installed version.                                                     |

## Project Initialization

### `arcl init`

Initialize or rediscover the project in the enclosing Git worktree.

```sh
arcl init
arcl init --workspace
```

The command creates `.arcl/config.toml`, `.arcl/arcl.db`, and a scoped
`.arcl/.gitignore`. It rejects directories outside Git worktrees and bare
repositories. Re-running initialization preserves existing project state.

### `arcl skill install`

Install the Arc Lightning agent skill into the project.

```sh
arcl skill install
arcl skill install --check
```

The command writes the skill into `.claude/skills/arclightning/` and
`.agents/skills/arclightning/` under the project root, and leaves identical
files unchanged. The files are built into `arcl` from the repository's
`skills/arclightning/` directory, so the installed skill matches the binary
that wrote it.

`--check` writes nothing. It reports each file as `unchanged`, `missing`, or
`changed`, and exits with status code `4` when any file is missing or changed.
Without `--check`, each file is `created`, `updated`, or `unchanged`. `--plain`
prints `<status>\t<path>` per file, and `--json` prints `action` (`installed`
or `checked`), `root`, and a `files` array of `path` and `status`.

The command checks every target before it writes any file, and replaces a file
by writing a temporary file beside it and renaming it. It exits with status
code `3` and writes nothing when a skill file is a symbolic link or not a
regular file, a skill directory is not a directory, or a path resolves outside
the project root. A linked directory, such as `.agents/skills` pointing at
`../.claude/skills`, is followed: the file is written once and its second path
is reported as `unchanged`.

## Workspace

| Command                                          | Purpose                                                         |
| ------------------------------------------------ | --------------------------------------------------------------- |
| `arcl workspace export`                          | Write the local database state to the configured workspace.     |
| `arcl workspace import`                          | Validate the workspace and rebuild the local database.          |
| `arcl workspace sync`                            | Carry changes made on one side to the other.                    |
| `arcl workspace status`                          | Show what changed since the last sync, without writing.         |
| `arcl workspace resolve --keep <side> <path>...` | Keep one side's version of files both sides changed, then sync. |

Workspace commands require the workspace to be enabled with
`arcl init --workspace`. A concurrent file change, or a file the database and
the workspace changed differently, stops the command with status code `4`
instead of overwriting either version.

See [Share the project workspace](/guides/workspace/) for the Git workflow and
workspace layout.

## Idea commands

| Command                                | Purpose                                    |
| -------------------------------------- | ------------------------------------------ |
| `arcl idea create <title>`             | Capture a new idea.                        |
| `arcl idea update <id>`                | Replace an idea title or description.      |
| `arcl idea discard <id>`               | Soft-delete an idea as `discarded`.        |
| `arcl idea list`                       | List all ideas in the inbox.               |
| `arcl idea promote <id> --spec <PATH>` | Create a linked epic from a captured idea. |

Descriptions use either `--description <MARKDOWN>` or
`--description-file <PATH|->`. The two sources are mutually exclusive; `-`
reads UTF-8 Markdown from standard input.

Promotion accepts `--release <ID>`. It creates the epic and relationship and
marks the idea promoted in one transaction. Repeating a successful promotion
returns the existing epic.

## Release commands

| Command                       | Purpose                   |
| ----------------------------- | ------------------------- |
| `arcl release create <title>` | Create an open release.   |
| `arcl release update <id>`    | Replace release metadata. |
| `arcl release complete <id>`  | Complete a release.       |
| `arcl release cancel <id>`    | Cancel a release.         |

`complete` and `cancel` accept `--allow-open-children` to bypass the default
descendant check for the selected release only.

## Epics

| Command                                  | Purpose                                             |
| ---------------------------------------- | --------------------------------------------------- |
| `arcl epic create <title> --spec <PATH>` | Create an epic linked to an existing Markdown spec. |
| `arcl epic update <id>`                  | Replace epic metadata or its linked spec.           |
| `arcl epic complete <id>`                | Complete an epic.                                   |
| `arcl epic cancel <id>`                  | Cancel an epic.                                     |

Epic creation accepts `--release <ID>`. Epic updates accept either
`--release <ID>` or `--no-release`. Spec paths are validated as regular `.md`
files inside the current Git worktree.

## Milestones

| Command                                     | Purpose                                 |
| ------------------------------------------- | --------------------------------------- |
| `arcl milestone create <title> --epic <ID>` | Create an ordered milestone.            |
| `arcl milestone update <id>`                | Replace milestone metadata or position. |
| `arcl milestone complete <id>`              | Complete a milestone.                   |
| `arcl milestone cancel <id>`                | Cancel a milestone.                     |

Use `--position <N>` when creating or updating a milestone. The default position
is `0`; negative positions are invalid. Terminal commands accept
`--allow-open-children`.

## Task Management

| Command                                     | Purpose                                                 |
| ------------------------------------------- | ------------------------------------------------------- |
| `arcl task create <title> --milestone <ID>` | Create a pending task or subtask.                       |
| `arcl task update <id>`                     | Replace metadata, move a subtree, or change its parent. |
| `arcl task start <id>`                      | Move ready pending work to `in_progress`.               |
| `arcl task park <id>`                       | Move pending or in-progress work to `parked`.           |
| `arcl task unpark <id>`                     | Return parked work to `pending`.                        |
| `arcl task handoff <id>`                    | Store a resume note and park in-progress work.          |
| `arcl task complete <id>`                   | Mark pending or in-progress work completed.             |
| `arcl task cancel <id>`                     | Cancel non-terminal work.                               |

Task creation accepts `--parent <ID>`, `--priority <critical|high|normal|low>`
(default `normal`), and `--position <N>` (default `0`). Task updates accept
`--milestone <ID>`, `--parent <ID>`, or `--no-parent`. `--parent` and
`--no-parent` are mutually exclusive.

Moving a task between milestones moves its entire descendant subtree. Parent and
child rows must belong to the same milestone, and cyclic reparenting is rejected
before storage changes.

`start` refuses a pending task that is not ready and lists the reasons. Having
child tasks is the one reason it skips. `--force` starts the task anyway.

`handoff` requires either `--note <MARKDOWN>` or `--note-file <PATH|->`.

Completion accepts either `--evidence <MARKDOWN>` or `--evidence-file <PATH|->`.
Each pair is mutually exclusive, and `-` reads UTF-8 Markdown from standard input.

`complete` refuses a task with open descendants. `--allow-open-children`
completes only the task. `--with-subtasks` also completes its pending and
in-progress descendants in one write, and refuses, writing nothing, while a
descendant is parked or a blocker outside the subtree is unfinished. The two
flags conflict. JSON output from `task complete` includes `completed_subtasks`,
the IDs it completed in tree order, and `unblocked`, the IDs of tasks that were
not ready before the command and are ready after it. Plain output prints the
task ID, then a `subtask<TAB><id>` line for each completed subtask and an
`unblocked<TAB><id>` line for each task it made ready.

## Inspection

| Command                  | Purpose                                                            |
| ------------------------ | ------------------------------------------------------------------ |
| `arcl show <id>`         | Show one record selected by its typed ID prefix.                   |
| `arcl list`              | List records using kind, state, priority, and association filters. |
| `arcl tree [<id>]`       | Show the complete hierarchy or one rooted subtree.                 |
| `arcl explain <task-id>` | Report every condition affecting task readiness.                   |
| `arcl context <task-id>` | Return the bounded context needed to work on a task.               |
| `arcl check`             | Validate database and graph invariants.                            |
| `arcl ready`             | List actionable leaf tasks in deterministic order.                 |
| `arcl next`              | Return the first actionable task, if one exists.                   |

`list` accepts `--kind`, `--status`, `--priority`, `--release`, `--epic`,
`--milestone`, and `--parent`. `ready` and `next` accept the same priority and
association filters.

See [Inspecting Work](/guides/inspect-work/) for the query workflow and context contents.

## Web app

`arcl serve` serves the web app and its read endpoints on `127.0.0.1`, on an
ephemeral port unless `--port` pins one. It prints the URL to open, which
carries a per-run token for the first page load. The app moves the token out
of the URL and sends it in the `X-Arcl-Token` header on every later request.
The server refuses a request without the token, with a `Host` header that does
not name the server, or with a method other than GET. Responses carry no CORS
headers. The server runs until interrupted.

| Endpoint                      | Same JSON as             |
| ----------------------------- | ------------------------ |
| `GET /api/records`            | `arcl list`              |
| `GET /api/records/<id>`       | `arcl show <id>`         |
| `GET /api/ready`              | `arcl ready`             |
| `GET /api/tasks/<id>/explain` | `arcl explain <task-id>` |
| `GET /api/tasks/<id>/context` | `arcl context <task-id>` |

`/api/records` and `/api/ready` take the command's flags as query parameters,
such as `?kind=task&status=pending&status=parked`. A failure returns the CLI's
error envelope with status 400 for exit codes 2 and 3, 404 for 5, 409 for 4,
and 500 otherwise. The server reads the database without syncing the
workspace, and a read never changes records; opening the database can still
run its migrations. Any other `arcl` command syncs the workspace. Outside a
project, `arcl serve` exits with code 3 before it binds.

The Work view lists every task by milestone, then by status, in the order
`arcl tree` gives: milestones by plan and position, and tasks by position,
each followed by its subtasks. A glyph leads each row; for a pending or parked
task, hover over it or focus the row to read why the task is not ready. `j` and `k` move the selection, and `Enter`
opens the task. Filters narrow the list by status, priority, milestone and
readiness: ready, blocked by an incomplete task, or waiting for another
reason, such as open subtasks. The page URL keeps the filters. A task deleted
while the list loads is left out; any other failed read shows an error with a
Retry button.

`arcl` embeds the web app from `apps/web/build` when it compiles. That build is
not committed, so `cargo install --git` produces an `arcl` whose `serve` shows
a page saying the web app was not built. To get the web app, run
`pnpm install` and `pnpm --filter @arclightning/web build` in a checkout, then
`cargo install --path .`.

## Lifecycle

Container records—releases, epics, and milestones—use `open`, `completed`, and
`cancelled`. A container cannot become terminal while a descendant is
non-terminal unless the command includes `--allow-open-children`.

Task records use `pending`, `in_progress`, `parked`, `completed`, and
`cancelled`. Completion and cancellation are terminal.

Repeating the same terminal command is safe, and switching between completed
and cancelled is rejected.

## Outputs

Human output is concise and may use ANSI colors. `--plain` emits stable unstyled
records for shell pipelines. `--quiet` prints only the affected ID for a
mutation. `--json` emits a top-level object with a `format_version` and a
command-specific `data` object containing the action and record fields.

| `format_version` | Commands                                                                                             |
| ---------------- | ---------------------------------------------------------------------------------------------------- |
| `2`              | `show <task-id>`, `task show`, `explain`, `task explain`, `context`, `task context`, `task complete` |
| `1`              | Every other command, failures, and sync warnings                                                     |

Task views and context packets report `readiness` with `ready`, `blocked`, and
`reasons`. Each reason has a `kind`: `status`, `has_children`,
`closed_ancestor`, `closed_container`, `direct_blocker`, `inherited_blocker`,
`cancelled_blocker`, `missing_blocker`, or `invalid_edge`. It also has a
`message` and the `task`, `ancestor`, `blocker`, `container`, or `path` fields
its kind names. [Automation](/guides/automation/) lists the fields per kind.

For example:

```sh
arcl --json idea create "Automate this" --description "Details"
```

The JSON mode writes structured failures to standard error and is intended
for automation that needs stable field names rather than human prose.

## Status Codes

The CLI uses these status categories/codes:

| Code | Meaning                                                        |
| ---- | -------------------------------------------------------------- |
| `0`  | Success.                                                       |
| `1`  | Runtime, filesystem, Git operation, or SQLite failure.         |
| `2`  | Command-line parsing failure reported by `clap`.               |
| `3`  | Invalid project, record, input, or lifecycle transition.       |
| `4`  | Workspace or concurrent-modification conflict, or skill drift. |
| `5`  | Requested record was not found.                                |
