---
title: Epics
description: Link spec-backed goals to releases and track their lifecycle.
---

An epic is a goal backed by an existing Markdown specification. Arc Lightning
stores the relationship and metadata; it does not edit the linked spec.

## Create an epic

Create the spec file first, then point the epic at it:

```sh
arcl epic create "Keyboard navigation" \
  --spec specs/keyboard-navigation.md \
  --release arcl-r-01J... \
  --description "Implement the navigation slice."
```

The spec path is resolved from the current directory and stored relative to the
Git worktree root. It must be an existing regular `.md` file inside the
worktree. Absolute paths, `..` traversal, symlink escapes, duplicate specs, and
non-Markdown files are rejected.

The release is optional. Omit `--release` for an ungrouped epic.

## Update an epic

Update tracker metadata without changing the spec file:

```sh
arcl epic update arcl-e-01J... --title "Keyboard and command navigation"
arcl epic update arcl-e-01J... --description-file epic-notes.md
arcl epic update arcl-e-01J... --spec specs/revised-navigation.md
```

Change the release association with `--release`, or remove it explicitly:

```sh
arcl epic update arcl-e-01J... --release arcl-r-01J...
arcl epic update arcl-e-01J... --no-release
```

## Finish or cancel an epic

```sh
arcl epic complete arcl-e-01J...
arcl epic cancel arcl-e-01J...
```

An epic cannot become terminal while a descendant milestone or task is open
unless you pass `--allow-open-children`. The override changes only the epic; it
does not cascade to its descendants.
