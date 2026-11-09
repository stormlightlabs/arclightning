---
title: Recover the workspace
description: Get sync running again after a branch switch removes record files, after a Git merge conflict, or when the database and the workspace changed one file differently.
---

When a sync stops, commands that change records refuse and name the files.
Commands that only read records still run on the database as it is, with a
warning. Start with `arcl workspace status`, which shows the cause without
writing anything.

## After a branch switch removed records

Switching to a branch that lacks records synced on another branch stops the
sync with exit code `3`, because sync never deletes records:

```text
error: invalid project or record: workspace file `captures/arcl-c-01M70MM9GA8S89Y6NNJN5V6YVM.md` was removed from the workspace; the workspace does not support hard deletion; `arcl workspace resolve --keep database captures/arcl-c-01M70MM9GA8S89Y6NNJN5V6YVM.md` writes it back
```

The database still holds those records. Write them back, one file at a time
with the command the error names, or all at once:

```sh
arcl workspace export
```

Git shows the restored files as untracked. Do not commit them: they belong to
the other branch, and committing them copies its records onto this one.

Git refuses to switch back while the files are untracked:

```text
error: The following untracked working tree files would be overwritten by checkout:
	.arcl/workspace/captures/arcl-c-01M70MM9GA8S89Y6NNJN5V6YVM.md
```

Delete them, then switch:

```sh
rm .arcl/workspace/captures/arcl-c-01M70MM9GA8S89Y6NNJN5V6YVM.md
git switch feature
```

`git switch --force` also works, and overwrites the files.

## After a merge conflict in a record file

When two branches change the same record, Git may leave its file unmerged.
Sync and import stop with exit code `4` and name the file, and write nothing.

Resolve the file as you would any other conflict, then finish the merge:

```sh
git add .arcl/workspace/tasks/<task-id>.md
git commit
```

The next `arcl` command syncs. Sync reads Git's index to find unmerged
files, not the text, so a record body that contains a sample conflict marker
still imports. `arcl workspace status` warns about such lines.

## When both sides changed one file

If the database and the workspace changed the same file differently since the
last sync, sync stops with exit code `4`:

```text
error: the database and the workspace changed these files differently since the last sync: `captures/arcl-c-01M70MM9GA8S89Y6NNJN5V6YVM.md`
```

```sh
arcl workspace status
```

```text
Changed since the last sync:
  divergent: database modified, workspace modified captures/arcl-c-01M70MM9GA8S89Y6NNJN5V6YVM.md (Git: modified)
```

Decide which version to keep, copy anything you need from the other, and
name the files:

```sh
arcl workspace resolve --keep workspace captures/arcl-c-01M70MM9GA8S89Y6NNJN5V6YVM.md
arcl workspace resolve --keep database captures/arcl-c-01M70MM9GA8S89Y6NNJN5V6YVM.md
```

`--keep workspace` imports the file over the database's version. `--keep
database` writes the database's version over the file. The other version is
lost. `resolve` then syncs every other change. It stops, writing nothing, if
another file still diverges, a named file has nothing to resolve, another
record file was removed, or Git reports a workspace file as unmerged.

## When a merge adds a blocker cycle

Two changes that are each valid can meet in a merge and close a blocker cycle,
or tie a task to its own ancestor or descendant. Sync stops with exit code
`3` and names the record and field. Edit that task file to remove one of the
edges, and run the command again.

## When a sync was interrupted

Sync writes workspace files before it updates the database. If a sync or an
export stops partway, the next sync finishes the work. If another `arcl`
command writes a record while sync runs, sync stops with exit code `4`; run it
again.
