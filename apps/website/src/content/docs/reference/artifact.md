---
title: Artifact documents and queue
description: The documents arcl web export writes, the export manifest and chunking, the queue entries the artifact page writes, and what arcl web apply returns.
---

[Publish the claude.ai artifact](/how-to/publish-the-artifact/) builds the
page and runs a sync. The page routes on the URL's hash, such as
`#/tasks/<id>`, sends no token and never calls `/api`. Opened outside
claude.ai, or without the database, it says to open it in claude.ai.

## arcl web export

```sh
arcl web export [--full] <DIR>
```

`export` syncs the workspace, then writes the database's documents under
`<DIR>` with the same reads `arcl serve` runs. If the sync fails, it exits
non-zero and writes nothing, so `meta/sync` never stamps records the sync did
not update.

| Document         | File                  | Same JSON as                                       |
| ---------------- | --------------------- | -------------------------------------------------- |
| `views/work`     | `views/work.json`     | `GET /api/work`                                    |
| `views/tree`     | `views/tree.json`     | `GET /api/tree`                                    |
| `views/tasks`    | `views/tasks.json`    | `GET /api/records?kind=task`                       |
| `views/captures` | `views/captures.json` | `GET /api/records?kind=capture&status=captured`    |
| `records/<id>`   | `records/<id>.json`   | `GET /api/records/<id>`, per task and open capture |
| `sources/<id>`   | `sources/<id>.json`   | `GET /api/records/<id>/source`, per task           |
| `meta/sync`      | `meta/sync.json`      | the commit, time and revision of the export        |

### Manifest

`<DIR>/manifest.json`, which `--json` also prints as `data`, holds
`format_version`, `commit`, `synced_at`, `revision` and three lists:

| List        | Holds                                                                           |
| ----------- | ------------------------------------------------------------------------------- |
| `documents` | every document                                                                  |
| `changed`   | documents that are new or differ from the files of the last export into `<DIR>` |
| `removed`   | documents the last export had and this one does not                             |

`changed` compares against `<DIR>`, not the artifact's database. After an
upload that did not finish, or into a database `<DIR>` never described, run
`arcl web export --full <DIR>`, which lists every document in `changed`.

### Chunks

A document over 200 KiB is written as parts, `chunks/<collection>~<id>~<n>`,
each `{"part": "..."}`, listed before a header
`{"chunked": {"parts": n, "digest": "..."}}` at the document's own path. The
page joins the parts and checks them against `digest`, the 64-bit FNV-1a hash
of the joined text. Parts that do not match are read as a sync in progress. A
document takes at most 64 parts, about 12.5 MiB; `export` fails on a larger
one.

## Queue entries

A change on the page writes a `queue/<id>` document instead of changing a
record:

```json
{
	"op": "discard_capture",
	"target": "arcl-c-...",
	"input": {},
	"created_at": "2026-10-25T06:30:00.000Z",
	"created_by": "u_...",
	"status": "queued"
}
```

`op` is one of `create_task`, `update_task`, `start_task`, `park_task`,
`unpark_task`, `hand_off_task`, `complete_task`, `create_capture`,
`promote_capture`, `discard_capture`, `create_spec`, `create_plan`,
`apply_plan` or `update_source`. `target` is the record ID for those that name
one and `null` otherwise. `input` holds the options of the matching
`arcl serve` POST endpoint.

`status` is `queued`, `applying` or `failed`, and a failed entry carries
`error`. The page refuses an entry over 200 KiB as UTF-8 JSON and says to make
the edit smaller or make it with `arcl`.

`update_source` carries `{"source", "revision"}`: the whole file from
`sources/<id>` and the revision from that document. It applies as
`POST /api/records/<id>/edit`, with the same checks, and fails with
`conflict` if the file changed after the export the page read.

## arcl web apply

```sh
arcl web apply <FILE>
```

`apply` reads a JSON array of queue entries, each the document's fields plus
its `id`, from a file or `-` for standard input. It runs each entry whose
`status` is `queued` through the same code as the `arcl serve` endpoint: the
same validation, sync lock and export, waiting up to 60 seconds for the lock.

- An entry with any other status, or none, is skipped.
- A file that is not a JSON array exits `3` and applies nothing.
- An entry that is not a valid queue entry, such as one with a field the page
  does not write, fails with `invalid_request`, naming its position and `id`.
  The other entries still run.
- An entry whose `input` is over 1 MiB as JSON, the endpoints' body limit,
  fails the same way.

Under `--json` it prints `{"format_version": 1, "data": {"results": [...]}}`,
one result per entry in order:

| Field                | Value                                                                   |
| -------------------- | ----------------------------------------------------------------------- |
| `id`, `op`, `target` | from the entry, each `null` when a malformed entry has no string for it |
| `status`             | `applied`, `failed` or `skipped`                                        |
| `data`               | the command's `data`, when it applied                                   |
| `error`              | the CLI's error, when it failed                                         |
| `warnings`           | sync or export warnings                                                 |

## Page behavior

The page says an edit is queued, keeps showing the records as they are, and
lists queued and applying entries above each view. A board move queues the
same way, and its card stays put until the edit applies. Dismissing a failed
edit deletes its entry.

**Edit file** reads the task's file from `sources/<id>`, and **Save file**
queues `update_source`. Any change to the task that applies first rewrites its
file, so a queued raw edit then fails with `conflict`; open the file again
after the next sync. While any edit to the task waits for the sync, queued or
applying, the page disables **Edit file**.

When `revision` in `meta/sync` changes, the page offers to reload the view
with the new records. If the database ends the page's subscription for good,
the page says it stopped following syncs and to reload.
