---
title: REST endpoints
description: Read and change records over HTTP with arcl serve's /api endpoints and POST /graphql, with each endpoint's options, the raw file edit and the errors.
---

`arcl serve` and `arcl serve --api` serve these endpoints on `127.0.0.1`.
Every request sends the token in the `X-Arcl-Token` header.
[GraphQL and REST API](/guides/api/) covers starting the server, the token
and the server's limits.

## Endpoints

| Endpoint                          | Same JSON as             |
| --------------------------------- | ------------------------ |
| `GET /api/records`                | `arcl list`              |
| `GET /api/records/<id>`           | `arcl show <id>`         |
| `GET /api/records/<id>/source`    | none (see below)         |
| `GET /api/ready`                  | `arcl ready`             |
| `GET /api/tasks/<id>/explain`     | `arcl explain <task-id>` |
| `GET /api/tasks/<id>/context`     | `arcl context <task-id>` |
| `GET /api/tree`                   | `arcl tree`              |
| `GET /api/work`                   | none (see below)         |
| `POST /api/tasks`                 | `arcl task create`       |
| `POST /api/tasks/<id>/update`     | `arcl task update`       |
| `POST /api/tasks/<id>/start`      | `arcl task start`        |
| `POST /api/tasks/<id>/park`       | `arcl task park`         |
| `POST /api/tasks/<id>/unpark`     | `arcl task unpark`       |
| `POST /api/tasks/<id>/handoff`    | `arcl task handoff`      |
| `POST /api/tasks/<id>/complete`   | `arcl task complete`     |
| `POST /api/captures`              | `arcl capture create`    |
| `POST /api/captures/<id>/promote` | `arcl capture promote`   |
| `POST /api/captures/<id>/discard` | `arcl capture discard`   |
| `POST /api/specs`                 | `arcl spec create`       |
| `POST /api/plans`                 | `arcl plan create`       |
| `POST /api/plans/<id>/apply`      | `arcl plan apply`        |
| `POST /api/records/<id>/edit`     | none (see below)         |
| `POST /graphql`                   | `arcl query`             |

## Reads

`/api/records` and `/api/ready` take the command's flags as query parameters,
such as `?kind=task&status=pending&status=parked`. `/api/tree` takes none and
returns the tree of every record. `/api/work` takes none and
returns the Work list in one request: every task that is not archived, with
the readiness `arcl explain` reports and its phase ID, inherited from an
ancestor where the task has none, and every phase. Its envelope is
`{"format_version": 2, "data": {"tasks": [{"task", "readiness", "phase_id"}], "phases": [...]}}`.

`/api/records`, `/api/tree` and `/api/tasks/<id>/context` leave archived tasks
out as the CLI does without `--all`, so the web app's Work list, board and
Planning view never show them.

Each `GET /api` read runs one named query against the schema `arcl query`
uses and maps the result into its envelope. These reads have no limit on the
objects they resolve.

`POST /graphql` takes `{"query", "variables", "operationName"}`, as GraphQL
clients send it, and answers status 200 with the response `arcl query` prints
for the same query, with the same limits. Like the other `arcl serve` reads, it
skips the workspace sync `arcl query` runs first. `errors` and limit refusals
come back in that body too. A body that is not JSON, has no `query` string,
has a `variables` that is not an object, or has an `operationName` that is not
a string gets `invalid_request` with status 400.

## Change options

A POST takes a JSON object of the command's options, named like its flags
with `_` for `-`:

| Endpoint   | Options                                                                                                |
| ---------- | ------------------------------------------------------------------------------------------------------ |
| `tasks`    | `title` (required), `body`, `priority`, `parent`, `spec`, `plan`, `phase`, `blocked_by`                |
| `update`   | `title`, `body`, `priority`                                                                            |
| `start`    | `force`                                                                                                |
| `handoff`  | `note`                                                                                                 |
| `complete` | `evidence`, `allow_open_children`, `with_subtasks`                                                     |
| `captures` | `title` (required), `body`                                                                             |
| `promote`  | `to` (required), `title`, `body`, `acceptance_criteria`, `spec`, `plan`, `phase`, `parent`, `priority` |
| `specs`    | `title` (required), `body`, `acceptance_criteria`                                                      |
| `plans`    | `title` (required), `spec` (required), `body`                                                          |
| `apply`    | `document` (required)                                                                                  |
| `edit`     | `source` (required), `revision` (required)                                                             |

`blocked_by` is a list of task IDs. `to` is `task`, `spec` or `note`, and
`promote` uses the capture's title and body for any it leaves out. `document`
is the plan's TOML, as `arcl plan apply --file` reads it.

## Edit a record's file

`/api/records/<id>/source` returns the record's workspace file, the bytes
`arcl workspace export` writes for it, as
`{"format_version": 1, "data": {"id", "kind", "path", "source", "revision"}}`.
`path` is the file's place in the workspace, such as `tasks/<id>.md`, and
`revision` is the lowercase hex SHA-256 of `source`. `edit` replaces the
record with `source`, a whole file, and answers with the saved file in the
same shape. It checks the file as `arcl workspace import` checks one: the TOML
must parse, the fields must fit the record's kind, the ID must match the
record, and every relationship and blocker rule must hold for the whole
graph. The edit is compared with the database, as the task commands are, so
a blocker cycle or ancestry rule violation it adds is refused even before the
first sync; one already in the database loads with a warning.

An edit may not change a field that only a command changes, since those
commands check lifecycle rules, readiness and open subtasks that the import
does not. Such an edit fails with `invalid_request`, status 400, and the
message names the command. A field left at its value passes, so a closed
record's other fields stay editable.

| Kind    | Fields only a command changes                                 | Commands                                                                                  |
| ------- | ------------------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| task    | `status`, `handoff`, `evidence`, `assignee`, `notes`, `gates` | `arcl task start`, `park`, `unpark`, `handoff`, `complete`, `cancel`, `note`; `arcl gate` |
| capture | `status`, `promoted-to`, `source-task`                        | `arcl capture promote`, `discard`; `arcl capture create`                                  |
| spec    | `status`, `source-capture`                                    | `arcl spec complete`, `cancel`; `arcl capture promote`                                    |
| plan    | `status`                                                      | `arcl plan complete`, `cancel`                                                            |
| phase   | `status`                                                      | `arcl plan phase complete`, `cancel`                                                      |
| release | `status`                                                      | `arcl release complete`, `cancel`                                                         |

The record ID in the path is the only file the request names. When the
file's revision is no longer `revision`, such as after an edit in Git or
another change, the edit fails with `conflict`, status 409. A refused edit
changes nothing. A saved edit reaches the workspace file through the sync
after the change.

## Bodies, errors and locking

`park`, `unpark` and `discard` take an empty body or `{}`. An unknown option,
a body that is not JSON, or a body over 1 MiB is refused. A request whose
`Content-Length` is over 1 MiB gets status 413 before any other check. The
server reads a body only after the request passes its checks, and answers 408
and closes the connection when the body has not arrived after 5 seconds. A
failure returns the CLI's error envelope with status 400 for
exit codes 2 and 3, 404 for 5, 409 for 4, and 500 otherwise.

A read opens the database without syncing the workspace, takes no lock and
never changes records; opening the database can still run its migrations. A
change runs as the CLI runs it: under the project's sync lock, after a sync
that must succeed, followed by a sync that exports it. Where a sync or export
warns, the reply's envelope lists the warnings in `warnings`, as the CLI's
JSON output does. A change waits at most 3 seconds for the lock; if another
`arcl` command still holds it, the change fails with `application_error`,
status 503 and a `Retry-After` header, and records stay as they were. Other
`arcl` commands run while the server serves. Outside a project, `arcl serve`
exits with code 3 before it binds.
