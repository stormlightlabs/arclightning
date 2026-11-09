---
title: REST endpoints
description: Every /api endpoint and POST /graphql that arcl serve answers, with options, the raw file edit, error statuses, locking and the server's security rules and limits.
---

`arcl serve` and `arcl serve --api` answer these endpoints on `127.0.0.1`.
Every request sends the token in the `X-Arcl-Token` header. [Serve the
API](/how-to/serve-the-api/) starts the server.

## Endpoints

| Endpoint                          | Same JSON as                                           |
| --------------------------------- | ------------------------------------------------------ |
| `GET /api/records`                | `arcl list`                                            |
| `GET /api/records/<id>`           | `arcl show <id>`                                       |
| `GET /api/records/<id>/source`    | none; see [Edit a record's file](#edit-a-records-file) |
| `GET /api/ready`                  | `arcl ready`                                           |
| `GET /api/tasks/<id>/explain`     | `arcl explain <task-id>`                               |
| `GET /api/tasks/<id>/context`     | `arcl context <task-id>`                               |
| `GET /api/tree`                   | `arcl tree`                                            |
| `GET /api/work`                   | none; see [Reads](#reads)                              |
| `POST /api/tasks`                 | `arcl task create`                                     |
| `POST /api/tasks/<id>/update`     | `arcl task update`                                     |
| `POST /api/tasks/<id>/start`      | `arcl task start`                                      |
| `POST /api/tasks/<id>/park`       | `arcl task park`                                       |
| `POST /api/tasks/<id>/unpark`     | `arcl task unpark`                                     |
| `POST /api/tasks/<id>/handoff`    | `arcl task handoff`                                    |
| `POST /api/tasks/<id>/complete`   | `arcl task complete`                                   |
| `POST /api/captures`              | `arcl capture create`                                  |
| `POST /api/captures/<id>/promote` | `arcl capture promote`                                 |
| `POST /api/captures/<id>/discard` | `arcl capture discard`                                 |
| `POST /api/specs`                 | `arcl spec create`                                     |
| `POST /api/plans`                 | `arcl plan create`                                     |
| `POST /api/plans/<id>/apply`      | `arcl plan apply`                                      |
| `POST /api/records/<id>/edit`     | none; see [Edit a record's file](#edit-a-records-file) |
| `POST /graphql`                   | `arcl query`                                           |

## Reads

`/api/records` and `/api/ready` take the command's flags as query parameters,
such as `?kind=task&status=pending&status=parked`. `/api/tree` takes none and
returns the tree of every record.

`/api/work` takes none and returns the Work list in one request. It holds
every phase, and every task that is not archived with the readiness
`arcl explain` reports. Each task's `phase_id` is inherited from an ancestor
where the task has none:

```json
{"format_version": 2, "data": {"tasks": [{"task", "readiness", "phase_id"}], "phases": [...]}}
```

`/api/records`, `/api/tree` and `/api/tasks/<id>/context` leave archived tasks
out, as the CLI does without `--all`.

Each `GET /api` read runs one named query against the schema `arcl query`
uses, and maps the result into its envelope. These reads have no limit on the
objects they resolve.

`POST /graphql` takes `{"query", "variables", "operationName"}`, as GraphQL
clients send it. It answers status 200 with the response `arcl query` prints
for the same query, with the same limits, and `errors` and limit refusals in
that body. Like the other reads, it skips the workspace sync `arcl query`
runs first. A body that is not JSON, has no `query` string, has a `variables`
that is not an object, or has an `operationName` that is not a string gets
`invalid_request` with status 400.

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
is the plan's TOML, as `arcl plan apply --file` reads it. `park`, `unpark` and
`discard` take an empty body or `{}`.

## Edit a record's file

`GET /api/records/<id>/source` returns the record's workspace file, the bytes
`arcl workspace export` writes for it:

```json
{"format_version": 1, "data": {"id", "kind", "path", "source", "revision"}}
```

`path` is the file's place in the workspace, such as `tasks/<id>.md`, and
`revision` is the lowercase hex SHA-256 of `source`.

`POST /api/records/<id>/edit` replaces the record with `source`, a whole file,
and answers with the saved file in the same shape. It checks the file as
`arcl workspace import` does: the TOML must parse, the fields must fit the
record's kind, the ID must match the record, and every relationship and
blocker rule must hold for the whole graph. The edit is compared with the
database, as the task commands are, so it refuses a blocker cycle or ancestry
violation it adds, even before the first sync. One already in the database
loads with a warning.

The edit may not change a field that only a command changes, listed in
[Workspace file format](/reference/workspace-format/#fields-only-commands-change).
Such an edit fails with `invalid_request`, status 400, and the message names
the command. A field left at its value passes, so a closed record's other
fields stay editable.

When the file's revision is no longer `revision`, such as after an edit in Git
or another change, the edit fails with `conflict`, status 409. A refused edit
changes nothing. A saved edit reaches the workspace file through the sync
after the change.

## Errors

A failure returns the CLI's error envelope with a status from the exit code:

| Exit code | Status |
| --------- | ------ |
| `2`, `3`  | 400    |
| `4`       | 409    |
| `5`       | 404    |
| other     | 500    |

An unknown option, a body that is not JSON, or a body over 1 MiB is refused. A
request whose `Content-Length` is over 1 MiB gets 413 before any other check.
The server reads a body only after the request passes its checks, and answers
408 and closes the connection when the body has not arrived after 5 seconds.

## Locking

A read opens the database without syncing the workspace, takes no lock and
never changes records. Opening the database can still run its migrations.

A change runs as the CLI runs it: under the project's sync lock, after a sync
that must succeed, followed by a sync that exports it. Where a sync or export
warns, the reply lists the warnings in `warnings`, as the CLI's JSON output
does. A change waits at most 3 seconds for the lock. If another `arcl` command
still holds it, the change fails with `application_error`, status 503 and a
`Retry-After` header, and records stay as they were.

Other `arcl` commands run while the server serves.

## Security

The server binds `127.0.0.1`, on a free port unless `--port` pins one. It
refuses a request without the token, or with a `Host` header that does not
name the server. No URL carries the token: a request that puts it in the query
string gets 401.

Reads take GET and changes take POST, and the server refuses any other method,
so a GET never changes records. `/graphql` takes POST only and needs no
`Origin`, since a query changes nothing.

| Mode               | Token                                                           | A change needs                             |
| ------------------ | --------------------------------------------------------------- | ------------------------------------------ |
| `arcl serve`       | A new one each run, in the printed URL for the first page load. | An `Origin` header that names the server.  |
| `arcl serve --api` | `ARCL_SERVE_TOKEN` when set, otherwise a new one each run.      | No `Origin`, or one that names the server. |

The page mode ignores `ARCL_SERVE_TOKEN`. The app moves the token out of the
URL and sends it in the header on every later request. The `Origin` check
stops other pages, including those on other `127.0.0.1` ports, from sending a
change. A change whose `Origin` names another server gets 403.

Responses carry no CORS headers, and the server refuses every CORS preflight,
so a page in a browser cannot send `X-Arcl-Token` to the server from another
origin. The page's Content-Security-Policy runs only the app's own script and
loads images only from the server or `data:` URLs, so Markdown cannot pull
images from other hosts.

Outside a project, `arcl serve` exits `3` before it binds. `ARCL_SERVE_TOKEN`
must be at least 32 ASCII letters and digits, or `arcl serve --api` exits `1`
at startup.

## Startup output

| Output    | `arcl serve`                     | `arcl serve --api`                                                        |
| --------- | -------------------------------- | ------------------------------------------------------------------------- |
| human     | the URL to open, with the token  | the origin and the token, or a line saying the variable's token is in use |
| `--plain` | the URL                          | the origin and the token on two lines, or the origin alone                |
| `--json`  | `status`, `url`, `port`, `token` | the same, without `token` when it comes from the variable                 |

## Shutdown and connection limits

The server runs until Ctrl-C, then gives open requests up to 5 seconds to
finish and exits. A read or change that has not started by Ctrl-C gets 503
and does not run; one already running finishes. A second Ctrl-C ends the wait
for open requests, and the server exits once the read or change already
running finishes. A read whose client disconnects before it starts does not
run.

The server closes a connection that takes more than 5 seconds to send a
request head, or idles that long between requests. It also closes one whose
client reads replies slower than 64 KiB/s. Each connection starts with 5
seconds of waiting for its client to read, spends it while a reply waits,
earns back a second per 64 KiB the client reads, and never holds more than 5
seconds.

A request refused for a bad `Host` or token, a body over 1 MiB, or a body that
does not arrive within 5 seconds gets `Connection: close`, and the connection
closes after the reply. The server keeps at most 128 connections open; a new
one waits to be accepted until another closes.
