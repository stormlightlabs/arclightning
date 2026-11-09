---
title: Share records through Git
description: Turn on the workspace, commit records with the code, set up a fresh clone, edit record files by hand, and check the workspace in CI.
---

<div id="enable-the-workspace"></div>

## Turn on the workspace

In a new project:

```sh
arcl init --workspace
```

In a project that already has a database, the same command turns the
workspace on and keeps the records. The next command writes them to
`.arcl/workspace/`.

`init` adds `.arcl/arcl.db` to `.arcl/.gitignore`, so the database stays out
of Git. Commit `.arcl/.gitignore`, `.arcl/config.toml` and `.arcl/workspace/`:

```sh
git add .arcl
git commit -m "Track work with arcl"
```

<div id="sync-both-ways"></div>
<div id="work-with-git"></div>
<div id="understand-the-workspace"></div>

## Commit records with the code

Each command writes its changes to the workspace as it saves them. Commit the
task files with the change they describe, so the pull request that does the
work also marks it done.

Commit, pull, merge and switch branches with Git as usual. The next `arcl`
command syncs whatever Git left in the workspace. arcl never changes Git's
HEAD, branches, index, remotes or configuration.

## Set up a fresh clone

A clone has the committed workspace but no database, and commands fail until
you run `arcl init`:

```text
error: invalid project or record: Arc Lightning has no database in project directory `/home/you/notes-app`; run `arcl init`, and the next command imports the workspace in `/home/you/notes-app/.arcl/workspace`
```

Run `arcl init` once. The next command imports every record.

<div id="edit-workspace-files"></div>

## Edit a record file

Open the file under `.arcl/workspace/`, change the front matter or the body,
and save it. The next `arcl` command imports it after checking it against the
whole graph:

```text
+++
id = "arcl-s-01..."
title = "Import validation"
status = "open"
acceptance-criteria = "- [ ] Invalid records are rejected"
+++

The specification body goes here.
```

Keep the front matter between the `+++` lines. Do not rename or delete a
record file, since records are never deleted. Close a record through its
status with the CLI instead. [Workspace file
format](/reference/workspace-format/) lists the fields.

<div id="check-before-you-sync"></div>

## See what a sync would do

```sh
arcl workspace status
```

```text
Git: on branch feature at b06075c
The database and the workspace match the last sync.
```

`status` writes nothing. It lists each file that changed since the last sync
and which side changed it, files Git reports as unmerged, and stray entries.
`--json` and `--plain` suit scripts.

To sync without running another command:

```sh
arcl workspace sync
```

<div id="export-the-whole-database"></div>
<div id="import-the-whole-workspace"></div>
<div id="avoid-divergent-changes"></div>
<div id="resolve-divergent-changes"></div>

## Check the workspace in CI

```sh
arcl init
arcl check
```

`check` fails with exit code `3` on workspace content arcl cannot read or
accept, such as a broken file or a blocker cycle merged in from two branches.

When something stops a sync, [Recover the
workspace](/how-to/recover-the-workspace/) has the fix for each case.
