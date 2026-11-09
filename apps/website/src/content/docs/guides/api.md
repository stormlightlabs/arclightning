---
title: GraphQL and REST API
description: Query records with GraphQL through arcl query or POST /graphql, serve the API with arcl serve --api, and keep it local with its token and limits.
---

Arc Lightning answers read-only GraphQL queries and serves a REST API, so
scripts and local tools can read records without parsing CLI output.

- `arcl query` runs a GraphQL query and prints the response. It needs no
  server.
- `arcl serve --api` serves `POST /graphql` and the `/api` endpoints on
  `127.0.0.1`, without the web app page. `arcl serve` serves the same
  endpoints beside the page.

[REST endpoints](/guides/rest/) lists each `/api` endpoint and its options.
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
`tree`, from an optional `root`, with `all`; `work`, with `all` to include
archived tasks; `context(task:)`; and
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
script or another local tool can use the [endpoints](/guides/rest/) against the
project's database. It binds `127.0.0.1` and refuses a request with the wrong
`Host` or without the token, as the page mode does. Any path outside `/api` and
`/graphql` gets 404. No URL carries the token: a request that puts it in the
query string gets 401, and every request sends it in the `X-Arcl-Token` header.

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

This example writes the header to a file only you can read, since a token on
the `curl` command line shows in `ps`. `printf` is a shell builtin, so it does
not. `curl` retries until the server is listening and fails on an error status:

```sh
export ARCL_SERVE_TOKEN=$(openssl rand -hex 32)
(umask 077; printf 'X-Arcl-Token: %s\n' "$ARCL_SERVE_TOKEN" > token.header)
arcl serve --api --port 4321 &
curl --fail --retry 10 --retry-delay 1 --retry-connrefused \
  -H @token.header -H 'Content-Type: application/json' \
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
closes after the reply. It keeps at most 128 connections open; a new one
waits to be accepted until another closes.
