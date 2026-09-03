---
title: Share project snapshots
description: Export planning records to Markdown, commit them with your project, and rebuild the local database after pulling changes.
---

Snapshots let you review and share Arc Lightning records through version
control. Arc Lightning continues to use SQLite while you work, then projects the
connected planning model into a Markdown workspace when you export it.

## Enable snapshots

Initialize the project with snapshot support:

```sh
arcl init --snapshot
```

Arc Lightning creates the snapshot workspace at `.arcl/snapshot/` by default.
It also adds `.arcl/arcl.db` to `.arcl/.gitignore`, so the local database stays
out of version control.

Commit `.arcl/config.toml` and `.arcl/snapshot/` with the rest of the project.

## Export your work

After you change records through the CLI, export the database projection:

```sh
arcl snapshot export
```

Review the changes under `.arcl/snapshot/`, then commit them. Export writes the
complete connected model, but leaves files unchanged when their bytes already
match. It stops if a workspace file has changed since the last export instead
of overwriting that file.

Run another export before each commit that changes Arc Lightning records. This
keeps the committed snapshot in step with your local database.

## Import changes after a pull

Import the snapshot before making new local changes:

```sh
arcl snapshot import
```

Import reads every record and validates IDs, references, task ancestry, parent
cycles, dependency cycles, and the complete relationship graph. Arc Lightning
replaces the database only after the whole workspace passes validation.

For a fresh clone, initialize the local database first, then import the
committed snapshot:

```sh
arcl init --snapshot
arcl snapshot import
```

## Edit snapshot files

You can edit a record in `.arcl/snapshot/` and import it back into SQLite. Keep
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

Do not rename or delete an existing record file. Snapshots do not support hard
deletion. Use the record's status to complete, cancel, discard, or otherwise
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

Import after pulling snapshot changes and export before committing local
database changes. If both the database and workspace changed from their last
shared export, import reports a conflict instead of choosing one version. No
partial database writes occur when an import fails.
