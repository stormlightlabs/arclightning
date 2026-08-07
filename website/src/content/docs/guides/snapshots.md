---
title: Version-control project snapshots
description: Export tracker state to Markdown and rebuild the local database from it.
---

Snapshots let a project keep Arc Lightning's shared state in Git while each
checkout uses its own local SQLite database. The snapshot lives in
`.arcl/snapshot/` as a versioned manifest and one Markdown file per record.

## Enable snapshots

Enable snapshots when initializing a worktree:

```sh
arcl init --snapshot
```

Commit `.arcl/snapshot/` with the rest of the project. Keep `.arcl/arcl.db`
local; Arc Lightning's generated `.arcl/.gitignore` already excludes it.

## Export local changes

After changing tracked work, write the database state to the snapshot:

```sh
arcl snapshot export
```

Export produces stable Markdown so unchanged records do not create noisy diffs.
If a snapshot file changed after Arc Lightning read it, export stops instead of
overwriting the newer content.

Review and commit the resulting snapshot changes through your usual Git
workflow.

## Import shared changes

After cloning a project or receiving snapshot changes, rebuild or update the
local database:

```sh
arcl snapshot import
```

Import validates the entire snapshot before replacing database state. It checks
record IDs, relationships, hierarchy, dependencies, plan keys, and linked spec
paths. Invalid or incomplete snapshots are rejected instead of being partially
applied.

A successful import rewrites harmless formatting differences into the canonical
snapshot form. Commit only meaningful changes; canonicalization should leave a
clean snapshot unchanged.
