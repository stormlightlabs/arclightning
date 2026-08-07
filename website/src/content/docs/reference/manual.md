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
arcl init --snapshot
```

The command creates `.arcl/config.toml`, `.arcl/arcl.db`, and a scoped
`.arcl/.gitignore`. It rejects directories outside Git worktrees and bare
repositories. Re-running initialization preserves existing project state.

## Snapshots

| Command                | Purpose                                                    |
| ---------------------- | ---------------------------------------------------------- |
| `arcl snapshot export` | Write the local database state to the configured snapshot. |
| `arcl snapshot import` | Validate the snapshot and rebuild the local database.      |

Snapshot commands require snapshot support to be enabled with
`arcl init --snapshot`. A concurrent file change stops the command with status
code `4` instead of overwriting the newer content.

See [Version-control project snapshots](/guides/snapshots/) for the Git workflow
and snapshot layout.

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
| `arcl task start <id>`                      | Move pending work to `in_progress`.                     |
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
child rows must remain in the same milestone, and cyclic reparenting is rejected
before storage changes.

`handoff` requires either `--note <MARKDOWN>` or `--note-file <PATH|->`.

Completion accepts either `--evidence <MARKDOWN>` or `--evidence-file <PATH|->`.
Each pair is mutually exclusive, and `-` reads UTF-8 Markdown from standard input.

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

## Lifecycle

Container records—releases, epics, and milestones—use `open`, `completed`, and
`cancelled`. A container cannot become terminal while a descendant remains
non-terminal unless the command includes `--allow-open-children`.

Task records use `pending`, `in_progress`, `parked`, `completed`, and
`cancelled`. Completion and cancellation are terminal.

Repeating the same terminal command is safe, and switching between completed
and cancelled is rejected.

## Outputs

Human output is concise and may use ANSI colors. `--plain` emits stable unstyled
records for shell pipelines. `--quiet` prints only the affected ID for a
mutation. `--json` emits a top-level object with `format_version: 1` and a
command-specific `data` object containing the action and record fields.

For example:

```sh
arcl --json idea create "Automate this" --description "Details"
```

The JSON mode writes structured failures to standard error and is intended
for automation that needs stable field names rather than human prose.

## Status Codes

The CLI uses these status categories/codes:

| Code | Meaning                                                     |
| ---- | ----------------------------------------------------------- |
| `0`  | Success.                                                    |
| `1`  | Runtime, filesystem, Git operation, or SQLite failure.      |
| `2`  | Command-line parsing failure reported by `clap`.            |
| `3`  | Invalid project, record, input, or lifecycle transition.    |
| `4`  | Reserved for snapshot or concurrent-modification conflicts. |
| `5`  | Requested record was not found.                             |
