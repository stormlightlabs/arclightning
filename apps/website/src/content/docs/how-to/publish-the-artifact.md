---
title: Publish the claude.ai artifact
description: Build the web app as a claude.ai artifact, publish it, and sync it from an agent session with arcl web export and arcl web apply.
---

The artifact lets people browse and triage a project from claude.ai without
`arcl serve`. It cannot reach the repository. An agent session copies the
records into the artifact's database, and applies the edits people queue on
the page. [Artifact documents and queue](/reference/artifact/) describes what
it reads and writes.

## Build the page

From a checkout of Arc Lightning:

```sh
pnpm --filter @arclightning/web build:artifact
```

This writes `apps/web/build-artifact/index.html`: the same views as
`arcl serve`'s page, in one file with no external scripts.

## Publish it

Publish the page as a claude.ai artifact with this capability declaration:

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
edits. Only editors and the owner write records.

## Load the records

Export the records into a directory and write every document to the
artifact's database:

```sh
arcl --json web export --full artifact-sync
```

Write each document listed in `artifact-sync/manifest.json` under `changed`,
from `artifact-sync/<path>.json`. Keep the directory for the next sync.

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
   fix that with `arcl workspace status`, and run it again. If a write
   fails, or `<dir>` is new, the next sync runs
   `arcl --json web export --full <dir>`, writes every `changed` document
   and deletes every document in `views`, `records`, `sources` and `chunks`
   that `documents` does not list.
5. Deletes each applied entry. This comes after the writes, so the page never
   offers **Edit file** on a `sources/<id>` document older than the edit. If
   a write in step 4 failed, stop before this step: the applied entries stay
   `applying`, and step 1 of the next sync finds their changes in the
   records.

The page follows `meta/sync` and the queue, and offers to reload when the
records change.

## Treat the queue as untrusted

Anyone who can queue an edit can also change or delete another queued entry.
`arcl web apply` checks every entry as the CLI checks a command, and an
applied edit runs as the person who runs `arcl`. Review the queue before
applying it if the artifact admits people you do not trust with the project.
