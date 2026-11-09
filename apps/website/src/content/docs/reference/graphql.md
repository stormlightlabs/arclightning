---
title: GraphQL schema
description: The root fields of the read-only GraphQL schema, how IDs and values appear, the error codes and the query limits.
---

`arcl query` and `POST /graphql` share one read-only schema. `arcl query
--schema` prints it in SDL, and it is checked in at
[`crates/arcl-graphql/schema.graphql`](https://github.com/stormlightlabs/arclightning/blob/main/crates/arcl-graphql/schema.graphql).

## Root fields

| Field                                                                                | Returns                                                   |
| ------------------------------------------------------------------------------------ | --------------------------------------------------------- |
| `record(id)`                                                                         | Any record or gate, as `arcl show` reads it.              |
| `capture`, `release`, `spec`, `plan`, `phase`, `task`, `note`, `gate` (`id`)         | One record of that kind.                                  |
| `records(kind, status, priority, release, spec, plan, phase, parent, assignee, all)` | Records as `arcl list` lists them.                        |
| `ready(priority, spec, plan, phase, parent)`                                         | Ready tasks in `arcl ready` order.                        |
| `tree(root, all)`                                                                    | The record tree, as `arcl tree` builds it.                |
| `work(all)`                                                                          | Every task with its readiness and phase, and every phase. |
| `context(task, all)`                                                                 | A task's context packet, as `arcl context` prints it.     |
| `source(id)`                                                                         | A record's workspace file and its `revision`.             |

Every `id` takes a full or short ID. `all` includes archived tasks, and in
`tree` also promoted and discarded captures.

## Types and values

`record` also takes a gate ID, so it returns the `RecordOrGate` union. Select
fields with `... on Record { title }` or `... on Gate { target }`. `records`
returns the `Record` interface; select kind fields with `... on Task`.

A task's `readiness`, `blockers`, `inheritedBlockers`, `gates` and `notes`
come from the same view as `arcl explain`. `spec`, `plan` and `phase` on a
task are the effective ones, inherited from a parent where the task has
none. `specId`, `planId`, `phaseId` and `parentId` are the IDs the task
stores.

A tree node's `descendants` lists every node below it, depth first, each with
its `depth`, so a tree of any depth reads in one query.

IDs in responses are full IDs, and every record also has `shortId`.
Statuses, kinds and priorities are the strings `--json` output uses, such as
`in_progress`.

## Errors

Standard output holds the GraphQL response: `data`, and `errors` when there
are any. `arcl query` exits `3` on a response with errors.

Each error from a resolver has an `extensions.code`, named for the CLI exit
code the same failure gets:

| Code              | Exit code | When                                                                                    |
| ----------------- | --------- | --------------------------------------------------------------------------------------- |
| `NOT_FOUND`       | `5`       | A well-formed ID names no record. Worded as `arcl show` words it.                       |
| `INVALID_REQUEST` | `3`       | An ID does not parse, such as `arcl-t-bad`, names another kind, or a filter is invalid. |
| `CONFLICT`        | `4`       | A workspace or concurrent-change conflict.                                              |
| `INTERNAL`        | other     | Any other failure.                                                                      |

## Limits

A query that passes a limit runs nothing, or stops, and returns `data: null`
with one error:

| Limit                             | `extensions.code`   | Checked        |
| --------------------------------- | ------------------- | -------------- |
| a document over 64 KiB            | `QUERY_TOO_LARGE`   | before parsing |
| fields nested more than 16 deep   | `QUERY_TOO_DEEP`    | before running |
| more than 1000 fields selected    | `QUERY_TOO_COMPLEX` | before running |
| more than 10,000 objects resolved | `RESULT_TOO_LARGE`  | while running  |

Depth counts `{ tree { id } }` as 2. The field count counts each field once.
The object count counts each record, task, gate, note, tree node and other
object in the response once. Nested lists multiply:
`plan { tasks { plan { tasks { id } } } }` resolves the square of the plan's
task count.

`POST /graphql` applies the same limits. The `GET /api` reads run named
queries against this schema with no object limit.
