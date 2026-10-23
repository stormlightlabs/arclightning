---
title: Share the project workspace
description: Export planning records to Markdown, commit them with your project, and rebuild the local database after pulling changes.
---

The workspace lets you review and share Arc Lightning records through version
control. Arc Lightning uses SQLite while you work, then projects the connected
planning model into Markdown files when you export it.

## Enable the workspace

Initialize the project with the workspace enabled:

```sh
arcl init --workspace
```

Arc Lightning creates the workspace at `.arcl/workspace/` by default. It also
adds `.arcl/arcl.db` to `.arcl/.gitignore`, so the local database stays out of
version control.

Commit `.arcl/config.toml` and `.arcl/workspace/` with the rest of the project.

## Sync both ways

Run sync after you change records, after you edit workspace files, and after a
pull or a branch switch:

```sh
arcl workspace sync
```

Sync compares each file with the version stored at the last sync. A change made
only in the database is written to the workspace. A change made only in the
workspace is validated with the complete graph and imported. A file that both
sides changed differently stops the sync with status code `4` and the paths
involved, and nothing is written. A record written by another `arcl` command
while sync runs also stops it with status code `4`; run sync again.

In a Git worktree, a file Git reports as unmerged stops sync and import with
status code `4` and the paths involved, and nothing is written. Finish the
merge, then run the command again. Sync and import read Git's state, not the
file text, so a record body can show a sample conflict.

Switching to a branch that lacks records synced elsewhere stops sync with status
code `3`, since sync does not delete records. Run `arcl workspace export` to
write those records back, then run sync again.

Sync writes workspace files before it updates the database. If a sync or an
export stops partway, the next sync finishes the work.

## Check before you sync

Status shows what a sync would do without writing anything:

```sh
arcl workspace status
```

It lists each file that changed since the last sync and which side changed it,
files both sides changed differently, files Git reports as unmerged, and
entries outside the workspace layout. It warns about files with lines that look
like conflict markers; sync accepts those lines as content. In a Git worktree it
also shows the branch, a detached HEAD, or a branch with no commits yet, and
files Git reports as changed that match the last sync, such as files sync wrote
that you have not staged. Use `--json` or `--plain` for scripts.

## Export your work

After you change records through the CLI, export the database projection:

```sh
arcl workspace export
```

Review the changes under `.arcl/workspace/`, then commit them. Export writes the
complete connected model, but leaves files unchanged when their bytes already
match. It stops if a workspace file has changed since the last export instead
of overwriting that file.

Run another export before each commit that changes Arc Lightning records. This
keeps the committed workspace in step with your local database.

## Import changes after a pull

Import the workspace before making new local changes:

```sh
arcl workspace import
```

Import reads every record and validates IDs, references, task ancestry, parent
cycles, dependency cycles, and the complete relationship graph. Arc Lightning
replaces the database only after the whole workspace passes validation.

For a fresh clone, initialize the local database first, then import the
committed workspace:

```sh
arcl init --workspace
arcl workspace import
```

## Edit workspace files

You can edit a record in `.arcl/workspace/` and import it back into SQLite. Keep
the TOML front matter between the `+++` delimiters and write the record body as
Markdown:

```text
+++
id = "arcl-s-01..."
title = "Import validation"
status = "open"
acceptance-criteria = "- [ ] Invalid records are rejected"
+++

The specification body goes here.
```

Do not rename or delete an existing record file. The workspace does not support
hard deletion. Use the record's status to complete, cancel, discard, or otherwise
close it through the CLI.

After a successful import, Arc Lightning rewrites the workspace in its
canonical form. It sorts files by path and repeated references by kind and ID,
uses LF line endings and one final newline, and omits empty optional metadata.

## Understand the workspace

The workspace contains `manifest.toml` and one Markdown file per record:

- `captures/<capture-id>.md`
- `specs/<spec-id>.md`
- `plans/<plan-id>.md`
- `phases/<phase-id>.md`
- `tasks/<task-id>.md`
- `notes/<note-id>.md`
- `releases/<release-id>.md`

The manifest contains `format-version = 1`. Front matter stores record metadata
and explicit relationships. Descendants are not added to releases or links
implicitly.

## Avoid divergent changes

Sync after pulling workspace changes and before committing local database
changes. If the database and the workspace changed the same file differently
since the last sync, sync and import report a conflict instead of choosing one
version. No partial database writes occur when either fails.
