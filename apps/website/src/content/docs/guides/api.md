---
title: GraphQL and REST API
description: Query records with GraphQL through arcl query or POST /graphql, and read or change them over REST with arcl serve --api.
---

Arc Lightning answers read-only GraphQL queries and serves a REST API, so
scripts and local tools can read records without parsing CLI output.

- `arcl query` runs a GraphQL query and prints the response. It needs no
  server.
- `arcl serve --api` serves `POST /graphql` and the `/api` endpoints on
  `127.0.0.1`, without the web app page. `arcl serve` serves the same
  endpoints beside the page.

GraphQL only reads. Changes go through the `POST /api` endpoints or the CLI.

## Query with arcl query

```sh
arcl query '{ ready { id title priority } }'
```

```json
{
  "data": {
    "ready": [
      {
        "id": "arcl-t-01M6Z4WFW2QQWWHMFT4X4X7TR6",
        "title": "Move the selection with j and k",
        "priority": "normal"
      },
      {
        "id": "arcl-t-01M6Z4X6J461QVR4Q1PF9Z543N",
        "title": "Show shortcuts in a help overlay",
        "priority": "normal"
      }
    ]
  }
}
```

| Command                                                              | Purpose                                    |
| -------------------------------------------------------------------- | ------------------------------------------ |
| `arcl query '<document>' [--variables <JSON>] [--operation <NAME>]`  | Run a query given on the command line.     |
| `arcl query --file <PATH> [--variables <JSON>] [--operation <NAME>]` | Run a query from a file, or `-` for stdin. |
| `arcl query --schema`                                                | Print the schema in SDL.                   |

`arcl query` syncs the workspace first, as other commands do. More examples:

```sh
arcl query '{ task(id: "7TR6") { title status readiness { ready reasons { kind message } } blockers { task { id title } completed } gates { kind target status } } }'
arcl query '{ records(kind: "task", status: ["pending"], priority: ["high"]) { id title ... on Task { phase { title } } } }'
arcl query --file query.graphql --variables '{"id": "7TR6"}' --operation Task
```

The root fields are `record`, `capture`, `release`, `spec`, `plan`, `phase`,
`task`, `note` and `gate`, each by full or short ID; `records`, with the
filters `arcl list` takes; `ready`, with the filters `arcl ready` takes;
`tree`, from an optional `root`, with `all`; `work`; `context(task:)`; and
`source(id:)`, a record's workspace file and its `revision`. Like
`arcl show`, `record` also takes a gate ID, so it returns a union: select
fields with `... on Record { title }` or `... on Gate { target }`. A task's
`readiness`, `blockers`, `inheritedBlockers`, `gates` and `notes` come from
the same view as `arcl explain`. `spec`, `plan` and `phase` on a task are the
effective ones; `specId`, `planId`, `phaseId` and `parentId` are the IDs the
task stores. A tree node's `descendants` lists every node below it, depth
first, each with its `depth`, so a tree of any depth reads in one query. IDs
in responses are full IDs, and every record also has `shortId`. Statuses,
kinds and priorities are the strings `--json` output uses, such as
`in_progress`.

The schema is checked in at
[`crates/arcl-graphql/schema.graphql`](https://github.com/stormlightlabs/arclightning/blob/main/crates/arcl-graphql/schema.graphql).

stdout holds the GraphQL response: `data`, and `errors` when there are any.
A response with errors exits `3`. Each error from a resolver has an
`extensions.code` of `NOT_FOUND`, `INVALID_REQUEST`, `CONFLICT` or `INTERNAL`.
A well-formed ID that names no record is `NOT_FOUND`, worded as `arcl show`
words it. An ID that does not parse, such as `arcl-t-bad`, is
`INVALID_REQUEST`, in `record` as in the typed fields.

A query document may be at most 64 KiB. A longer one is refused before it is
parsed: `data` is `null` and the error's `extensions.code` is
`QUERY_TOO_LARGE`.

A query may nest fields at most 16 deep, counting `{ tree { id } }` as 2, and
select at most 1000 fields, counting each field once. A query over either limit
runs nothing: `data` is `null` and the error's `extensions.code` is
`QUERY_TOO_DEEP` or `QUERY_TOO_COMPLEX`.

A query may also resolve at most 10,000 objects, counting each record, task,
gate, note, tree node and other object in the response once. Nested lists
multiply: `plan { tasks { plan { tasks { id } } } }` resolves the square of
the plan's task count. A query that passes the limit stops there: `data` is
`null` and the one error's `extensions.code` is `RESULT_TOO_LARGE`.

## Serve the API

`arcl serve --api` serves `/graphql` and `/api` without the web app page, so a
script or another local tool can use the [endpoints](#endpoints) against
the project's database. It binds `127.0.0.1` and refuses a request with the
wrong `Host` or without the token, as the page mode does. Any path outside `/api` and
`/graphql` gets 404. No URL carries the token: a request that puts it in the
query string gets 401, and every request sends it in the `X-Arcl-Token`
header.

The token comes from `ARCL_SERVE_TOKEN` when it is set. It must be at least 32
characters, ASCII letters and digits only; otherwise `arcl serve --api` exits
`1` at startup with an error naming that rule. Without the variable, each run
makes a new token. The page mode always makes a new one and ignores the
variable.

`arcl --json serve` prints one startup line with the URL, the port and the
token:

```json
{ "format_version": 1, "data": { "status": "serving", "url": "http://127.0.0.1:4321", "port": 4321, "token": "..." } }
```

Under `--api` the URL is the server's origin. In the page mode it is the URL
to open, with the token for the first page load. Human output under `--api`
prints the URL and the token, and `--plain` prints them on two lines. A token
from `ARCL_SERVE_TOKEN` is never printed, so it stays out of logs: human output
says the variable's token is in use, `--plain` prints the URL alone, and the
JSON line has no `token` field.

Under `--api`, a change without an `Origin` header runs once the token
matches. A change whose `Origin` does not name the server gets 403. A page in
a browser cannot send the `X-Arcl-Token` header to another origin without a
CORS preflight, and the server refuses every preflight without CORS headers.
In the page mode, every change needs an `Origin` naming the server.

`curl` retries until the server is listening and fails on an error status:

```sh
export ARCL_SERVE_TOKEN=$(openssl rand -hex 32)
arcl serve --api --port 4321 &
curl --fail --retry 10 --retry-delay 1 --retry-connrefused \
  -H "X-Arcl-Token: $ARCL_SERVE_TOKEN" -H 'Content-Type: application/json' \
  -d '{"query": "{ ready { id title } }"}' http://127.0.0.1:4321/graphql
```

```json
{
  "data": {
    "ready": [
      {
        "id": "arcl-t-01M6Z4WFW2QQWWHMFT4X4X7TR6",
        "title": "Move the selection with j and k"
      },
      {
        "id": "arcl-t-01M6Z4X6J461QVR4Q1PF9Z543N",
        "title": "Show shortcuts in a help overlay"
      }
    ]
  }
}
```

A request without the token gets 401, and a path outside `/api` and
`/graphql` gets 404.

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

`park`, `unpark` and `discard` take an empty body or `{}`. An unknown option, a body that
is not JSON, or a body over 1 MiB is refused. A request whose
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

## Security and limits

`arcl serve`, with or without `--api`, binds `127.0.0.1`, on an ephemeral
port unless `--port` pins one. In the page mode, the URL it prints carries a
per-run token for the first page load. The app moves the token out of the URL
and sends it in the `X-Arcl-Token` header on every later request.

The server refuses a request without the token or with a `Host` header that
does not name the server. Reads take GET and changes take POST; the server
refuses any other method, so a GET never changes records. `/graphql` takes
POST only and needs no `Origin`, since a query changes nothing. In the page
mode a change also needs an `Origin` header naming the server, which stops
other pages, including those on other `127.0.0.1` ports, from sending one.
Responses carry no CORS headers. The page's Content-Security-Policy runs only
the app's own script and loads images only from the server or `data:` URLs,
so Markdown cannot pull images from other hosts.

The server runs until Ctrl-C, then gives open requests up to 5 seconds to
finish and exits. A read or change that has not started by Ctrl-C gets status
503 and does not run; one already running finishes. A second Ctrl-C ends the
wait for open requests, and the server exits once the read or change already
running finishes. A read whose client disconnects before it starts does not
run.

The server closes a connection that takes more than 5 seconds to send a request
head or idles that long between requests. It also closes one whose client reads
replies slower than 64 KiB/s. Each connection starts with 5 seconds of waiting
for its client to read, spends it while a reply waits, earns back a second per
64 KiB the client reads, and never holds more than 5 seconds. A request it
refuses for a bad `Host` or token, for a body over 1 MiB, or for a body that
does not arrive within 5 seconds gets `Connection: close` and the connection
closes after the reply. It keeps at most 128 connections open; a new one waits to be accepted until another closes.
