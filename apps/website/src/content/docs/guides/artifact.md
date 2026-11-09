---
title: claude.ai artifact
description: Publish the web app as a claude.ai artifact, export records into it with arcl web export, and apply the edits people queue there with arcl web apply.
---

The web app also runs as a claude.ai artifact, so a person can browse and
triage a project from claude.ai without `arcl serve`. The artifact cannot
reach the repository. An agent session copies the project's records into the
artifact's database and applies the edits people queue on the page.

## Build and publish the page

`pnpm --filter @arclightning/web build:artifact` writes the artifact page to
`apps/web/build-artifact/index.html`: the same views as `arcl serve`'s page,
in one file with no external scripts. It routes on the URL's hash, such as
`#/tasks/<id>`, sends no token and never calls `/api`. Publish it with this
capability declaration:

```json
{
	"db": {
		"rules": [
			{ "path": "", "read": "view", "write": "admin" },
			{ "path": "queue", "read": "view", "write": "interact" }
		]
	},
	"user": {}
}
```

Everyone the artifact admits reads the records. Contributors and above queue
edits; only editors and the owner write records. Opened outside claude.ai, or
without the database, the page says to open it in claude.ai.

## Export records

`arcl web export <dir>` syncs the workspace, then writes the database's
documents under `<dir>` with the same reads `arcl serve` runs. If the sync
fails, it exits non-zero and writes nothing, so `meta/sync` never stamps
records the sync did not update:

| Document         | File                  | Same JSON as                                       |
| ---------------- | --------------------- | -------------------------------------------------- |
| `views/work`     | `views/work.json`     | `GET /api/work`                                    |
| `views/tree`     | `views/tree.json`     | `GET /api/tree`                                    |
| `views/tasks`    | `views/tasks.json`    | `GET /api/records?kind=task`                       |
| `views/captures` | `views/captures.json` | `GET /api/records?kind=capture&status=captured`    |
| `records/<id>`   | `records/<id>.json`   | `GET /api/records/<id>`, per task and open capture |
| `sources/<id>`   | `sources/<id>.json`   | `GET /api/records/<id>/source`, per task           |
| `meta/sync`      | `meta/sync.json`      | the commit, time and revision of the export        |

`<dir>/manifest.json` lists every document in `documents`, the ones that are
new or differ from the files of the last export into `<dir>` in `changed`,
and the ones the last export had and this one does not in `removed`. A sync
writes the `changed` documents in order and deletes the `removed` ones, so it
touches only the records that changed. `changed` compares against `<dir>`,
not the database, so after an upload that did not finish, or into a
database `<dir>` never described, run `arcl web export --full <dir>`. It
lists every document in `changed`.

A document over 200 KiB is written as parts, `chunks/<collection>~<id>~<n>`,
each `{"part": "..."}`, listed before a header
`{"chunked": {"parts": n, "digest": "..."}}` at the document's own path. The
page joins the parts and checks them against `digest`, the 64-bit FNV-1a hash of
the joined text; parts that do not match are read as a sync in progress. A
document needs at most 64 parts, about 12.5 MiB. `export` fails on a larger one.

## Queued edits

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

`op` names the change: `create_task`, `update_task`, `start_task`,
`park_task`, `unpark_task`, `hand_off_task`, `complete_task`,
`create_capture`, `promote_capture`, `discard_capture`, `create_spec`,
`create_plan`, `apply_plan` or `update_source`. `target` is the record ID for
those that name one and `null` otherwise. `input` holds the options of the
matching `arcl serve` POST endpoint. The page says the edit is queued, keeps
showing the records as they are, and lists the queued edits above each view.
A board move queues the same way, and its card stays put until the edit
applies. The page refuses an edit whose entry is over 200 KiB as UTF-8 JSON
and says to make it smaller or make it with `arcl`.

**Edit file** reads the task's file from `sources/<id>` and **Save file**
queues `update_source` with `{"source", "revision"}`: the whole file and the
revision from that document. `apply` runs it as
`POST /api/records/<id>/edit`, with the same checks. If the file changed
after the export the page read, the entry fails with the `conflict` error;
open the file again after the next sync and redo the edit. Any change to the
task that applies first rewrites its file, whether a board move, an edit or
another raw edit, so the raw edit then fails the same way. Saving closes the
editor, and while any edit to the task waits for the sync, queued or
applying, the page disables **Edit file** and says to edit the file after
that sync.

## Apply queued edits

`arcl web apply <file>` reads a JSON array of queue entries, each the
document's fields plus its `id`, from a file or `-` for standard input. It
runs each entry whose `status` is `queued` through the same code as
`arcl serve`'s endpoint: the same validation, sync lock and export, waiting up
to 60 seconds for the lock. An entry with any other status, or none, is
skipped; the page does not list it either. A file that is not a JSON array
exits with code 3 and applies nothing. An entry that is not a valid queue
entry, such as one with a field the page does not write, fails with an
`invalid_request` error naming its position and `id`, and the other entries
still run. So does an entry whose `input` is over 1 MiB as JSON, the
endpoints' body limit. Under `--json` it prints
`{"format_version": 1, "data": {"results": [...]}}`, one result per entry in
order: `id`, `op`, `target` (each `null` when a malformed entry has no string
value for it), `status` (`applied`, `failed` or `skipped`), the command's
`data` when it applied, the CLI's `error` when it failed, and `warnings`.

Anyone who can queue an edit can also change or delete another queued entry,
so the queue is untrusted input. `apply` checks every entry as the CLI checks
a command, and an applied edit runs as the person who runs `arcl`.

## Sync the artifact

To sync, the agent session that owns the artifact:

1. Reads the `queue` collection. An entry whose `status` is `applying` was
   claimed by a sync that stopped before it finished. Check whether its
   change is in the records: if it is, delete the entry in step 5 with this
   sync's applied ones; if it is not, set `status` back to `queued`.
2. Sets `status` to `applying` on each `queued` entry in the database, then
   writes those entries, with their IDs and `status` still `queued`, to a
   file. An entry claimed this way never runs twice: `apply` skips it if a
   later sync reads it again.
3. Runs `arcl --json web apply <file>`, then sets `status` to `failed` and
   `error` to the result's error on each failed entry, so the page shows
   why. Applied entries stay `applying` for now.
4. Runs `arcl --json web export <dir>` into the directory of the last sync,
   writes each `changed` document from `<dir>/<path>.json` and deletes each
   `removed` one. If the export fails because the workspace did not sync,
   resolve that, with `arcl workspace status`, and run it again. If a write
   fails, or `<dir>` is new, the next sync runs
   `arcl --json web export --full <dir>`, writes every `changed` document
   and deletes every document in `views`, `records`, `sources` and `chunks`
   that `documents` does not list.
5. Deletes each applied entry. This comes after the writes, so the page never
   offers **Edit file** on a `sources/<id>` document older than the edit. If
   a write in step 4 failed, stop before this step: the applied entries stay
   `applying`, and step 1 of the next sync finds their changes in the
   records.

The page follows `meta/sync` and the queue. When `revision` changes, it offers
to reload the view with the new records. It lists `applying` entries with the
queued ones, marked as applying now. Dismissing a failed edit on the page
deletes its entry. If the database ends the page's subscription for good, the
page says it stopped following syncs and to reload.
