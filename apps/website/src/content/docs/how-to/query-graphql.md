---
title: Query with GraphQL
description: Read records with arcl query, pass variables and pick an operation, read errors, and print the schema.
---

`arcl query` runs a read-only GraphQL query against the project and prints
the response as JSON. It needs no server, and syncs the workspace first as
other commands do. Changes go through the CLI or the `POST /api` endpoints.

<div id="query-with-arcl-query"></div>

## Run a query

```sh
arcl query '{ ready { id title priority } }'
```

```json
{
  "data": {
    "ready": [
      {
        "id": "arcl-t-01M70M8FPNTS24NBFPXW9F35WW",
        "title": "Move the selection with j and k",
        "priority": "normal"
      }
    ]
  }
}
```

Fetch a task with its readiness, blockers and gates in one query:

```sh
arcl query '{ task(id: "AWEF") { title status readiness { ready reasons { kind message } } blockers { task { id title } completed } gates { kind target status } } }'
```

Filter records as `arcl list` does. `records` returns the `Record`
interface, so select task fields with `... on Task`:

```sh
arcl query '{ records(kind: "task", status: ["pending"], priority: ["high"]) { id title ... on Task { phase { title } } } }'
```

## Use a query file and variables

Keep longer queries in a file. `--variables` takes JSON, and `--operation`
picks one operation when the file holds several:

```graphql
query Task($id: ID!) { task(id: $id) { title status } }
query Ready { ready { id } }
```

```sh
arcl query --file q.graphql --variables '{"id": "AWEF"}' --operation Task
```

```json
{
  "data": {
    "task": {
      "title": "Document the shortcuts",
      "status": "pending"
    }
  }
}
```

`--file -` reads the query from standard input.

## Read errors

A response with errors exits `3`. Standard output still holds the response,
with `errors` beside `data`:

```sh
arcl query '{ task(id: "ZZZZ") { title } }'
```

```json
{
  "data": null,
  "errors": [
    {
      "message": "no record matches short ID `ZZZZ`",
      "locations": [{ "line": 1, "column": 3 }],
      "path": ["task"],
      "extensions": { "code": "NOT_FOUND" }
    }
  ]
}
```

Branch on `extensions.code`. [GraphQL schema](/reference/graphql/#errors)
lists the codes and the query limits.

<div id="serve-the-api"></div>
<div id="security-and-limits"></div>

## Print the schema

```sh
arcl query --schema
```

The command prints the schema in SDL, with a description on each field. The
same schema is checked in at
[`crates/arcl-graphql/schema.graphql`](https://github.com/stormlightlabs/arclightning/blob/main/crates/arcl-graphql/schema.graphql).

To query over HTTP instead, [serve the API](/how-to/serve-the-api/).
