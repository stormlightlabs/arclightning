---
title: The workspace and database sync
description: Why arcl keeps records in both SQLite and Markdown files, how sync decides which side changed, and what it refuses to guess.
---

arcl keeps two copies of a project's records. The SQLite database,
`.arcl/arcl.db`, answers queries fast and enforces the rules in transactions.
The workspace, `.arcl/workspace/`, holds one Markdown file per record, which
Git can diff, merge, review and carry between clones. The database stays out
of Git; the workspace is committed.

The workspace is optional. Without it, records live in the local database
only.

## When sync runs

With the workspace on, every command that reads or changes records syncs
first, and every change is written to the workspace as soon as it is saved. A
file edited by hand, pulled or changed by a branch switch reaches the database
on the next command, whatever that command is.

Commands in one project run one at a time while the workspace is on. A command
waits on the lock file `.arcl/arcl.db-sync.lock` until the one before it
finishes its syncs. MCP tools wait up to 10 seconds for the lock, and
`arcl serve` changes up to 3.

## How sync decides

Sync stores the version of each file it last synced, and compares both sides
with that version:

- A file changed only in the database is written to the workspace.
- A file changed only in the workspace is checked against the whole graph and
  imported.
- A file both sides changed differently stops the sync, and nothing is
  written.

The last case is the one sync will not guess at. Either version could be the
one you meant, so `arcl workspace resolve --keep <side>` makes you choose.

Sync also checks blocker edges and parent links. Two branches can each add a
valid edge that together close a cycle. Sync refuses such an edge when it is
new to the database, and names the record and field. An edge the database
already holds loads with a warning, so one bad edge elsewhere does not block
unrelated work.

## Why records are never deleted

A missing file is ambiguous. It could be a deletion, or a branch that never
had the record. Most often it is a branch switch: the database holds records
from the branch you left, and the new branch lacks their files.

Sync never treats a missing file as a deletion. It stops and says how to write
the file back, and records close through their status instead. This keeps one
branch from silently erasing another branch's work.

## Refusing changes, allowing reads

When the first sync of a command stops, a command that changes records
refuses before it changes anything, and names the files. A command that only
reads records still runs on the database as it is, with a warning. You can
still look at the project while you fix the workspace. `arcl check` fails in
both cases, so CI catches the problem.

The web app and the REST reads open the database without syncing, so they
stay fast. Their changes sync like commands do.

## Order of writes

Sync writes workspace files before it updates the database. If it stops
partway, the files hold the newer state and the next sync finishes the work.
If the export after a change fails, the change is already saved, and the next
command writes it to the workspace.

## What Git sees

arcl reads Git's state but never changes HEAD, branches, the index, remotes
or configuration. It uses Git's index to spot unmerged files, and stops on
them rather than importing a file Git has not finished merging. Committing
the workspace is your step, like committing code.

[Share records through Git](/how-to/share-the-workspace/) and [Recover the
workspace](/how-to/recover-the-workspace/) cover the tasks.
