---
title: Serve the API
description: Run arcl serve --api for scripts and local tools, set a fixed token, and call /graphql and the /api endpoints with curl.
---

`arcl serve --api` serves `POST /graphql` and the `/api` endpoints on
`127.0.0.1`, without the web app page. `arcl serve` serves the same endpoints
beside the page.

## Start the server with a fixed token

Set `ARCL_SERVE_TOKEN` so the script knows the token in advance. It must be at
least 32 ASCII letters and digits:

```sh
export ARCL_SERVE_TOKEN=$(openssl rand -hex 32)
arcl serve --api --port 4321
```

```text
Serving the Arc Lightning API at http://127.0.0.1:4321
Token: the one in ARCL_SERVE_TOKEN
Press Ctrl-C to stop.
```

A token that breaks the rule makes `arcl serve --api` exit `1` at startup.
Without the variable, each run makes a new token and prints it. Leave out
`--port` to take a free port.

For a script, `arcl --json serve --api` prints one startup line:

```json
{"format_version":1,"data":{"status":"serving","url":"http://127.0.0.1:4322","port":4322}}
```

The line includes `token` only when arcl made the token. A token from
`ARCL_SERVE_TOKEN` is never printed, so it stays out of logs.

## Send a request

Every request sends the token in the `X-Arcl-Token` header. A token on the
`curl` command line shows in `ps`, so write the header to a temporary file.
`mktemp` creates it with mode `600`, readable only by you, outside the
working tree. `printf` is a shell builtin and does not show in `ps`:

```sh
header=$(mktemp)
printf 'X-Arcl-Token: %s\n' "$ARCL_SERVE_TOKEN" > "$header"
curl --fail --retry 10 --retry-delay 1 --retry-connrefused \
  -H @"$header" -H 'Content-Type: application/json' \
  -d '{"query": "{ ready { id title } }"}' http://127.0.0.1:4321/graphql
```

```json
{
  "data": {
    "ready": [
      {
        "id": "arcl-t-01M70M8FPNTS24NBFPXW9F35WW",
        "title": "Move the selection with j and k"
      }
    ]
  }
}
```

`--retry-connrefused` waits for the server to start listening, and `--fail`
fails on an error status. A request without the token gets 401, and a path
outside `/api` and `/graphql` gets 404.

## Read and change records

The `/api` reads take a command's flags as query parameters:

```sh
curl -H @"$header" 'http://127.0.0.1:4321/api/records?kind=task&status=pending'
```

```json
{"format_version":1,"data":{"records":[{"id":"arcl-t-01M70M8FPNTS24NBFPXW9F35WW","kind":"task","status":"pending","title":"Move the selection with j and k"},{"id":"arcl-t-01M70MEBNV02V13M6YBHP1AWEF","kind":"task","status":"pending","title":"Document the shortcuts"}]}}
```

Changes are POST requests with a JSON body of the command's options:

```sh
curl -H @"$header" -H 'Content-Type: application/json' \
  -d '{"title": "Add a help overlay", "priority": "high"}' \
  http://127.0.0.1:4321/api/tasks
```

```json
{"format_version":1,"data":{"action":"created","task":{"body":"","evidence":"","handoff":"","id":"arcl-t-01M70N2GQ5DXWMRC63EX9BEQSS","parent_id":null,"phase_id":null,"plan_id":null,"position":0,"priority":"high","project_id":"arcl-pj-00000000000000000000000000","spec_id":null,"status":"pending","title":"Add a help overlay"}}}
```

Under `--api`, a change without an `Origin` header runs once the token
matches.

[REST endpoints](/reference/rest/) lists every endpoint, its options, the
error statuses and the server's limits.

When you are done, delete the header file:

```sh
rm "$header"
```

## Stop the server

Press Ctrl-C. The server gives open requests up to 5 seconds to finish, then
exits.
