---
title: Version-control project snapshots
description: Export the connected planning model to Markdown and rebuild the local database from it.
---

Snapshots keep a reviewable projection of Arc Lightning's planning records in a
project workspace. SQLite remains the operational store. The workspace is
optional and can be committed with the project.

## Enable snapshots

Enable the workspace when initializing a project:

```sh
arcl init --snapshot
```

The default workspace is `.arcl/snapshot/`. Keep `.arcl/arcl.db` local; the
`.arcl/.gitignore` created by `arcl init` excludes it.

## Workspace format

`manifest.toml` uses `format-version = 2`. Each record has TOML front matter
between `+++` lines and a Markdown body after the closing delimiter:

```text
+++
id = "arcl-s-01..."
title = "Import validation"
status = "open"
acceptance-criteria = "- [ ] Invalid records are rejected"
+++

The specification body is owned by Arc Lightning.
```

The workspace stores one file per record:

- `captures/<capture-id>.md`
- `specs/<spec-id>.md`
- `plans/<plan-id>.md`
- `phases/<phase-id>.md`
- `tasks/<task-id>.md`
- `notes/<note-id>.md`
- `releases/<release-id>.md`

Captures, specs, plans, tasks, and notes keep their Markdown bodies in the
file. Task metadata includes status, priority, position, optional ancestry,
blocking task IDs, handoff, and completion evidence. Plans and phases preserve
their parent IDs and ordering. Release membership and record links are stored
as explicit front matter references; descendants are not added implicitly.

Arc Lightning sorts record files by path and sorts repeated references by kind
and ID. It writes LF line endings, one final newline, and omits empty optional
metadata. These rules make a parse-render-parse round trip deterministic while
preserving Markdown content.

## Export local changes

After changing records, write the database projection to the workspace:

```sh
arcl snapshot export
```

Export stages the complete projection before replacing any destination files.
It does not rewrite a file whose bytes already match the projection. If a
working-tree file diverges from the last exported base, the command stops
rather than replacing the newer bytes.

## Import shared changes

After cloning a project or receiving workspace changes, update the local
SQLite database:

```sh
arcl snapshot import
```

Import reads every record, validates IDs, references, flexible task ancestry,
parent cycles, dependency cycles, and the complete relationship graph before
replacing database state. Invalid or incomplete workspaces are rejected without
partial database writes. If both the database and workspace diverged from the
last exported base, import reports a conflict instead of choosing a side.

Arc Lightning accepts the previous `format-version = 1` workspace as an
explicit migration input. The migration maps ideas to captures, epics to
owned specs and plans, milestones to phases, and preserves task state,
relationships, handoffs, evidence, and Markdown bodies. A successful import
writes the version 2 layout and removes the migrated version 1 record files.
