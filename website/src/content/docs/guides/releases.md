---
title: Releases and Specs
description: Group spec-backed work into releases and epics.
---

A release groups epics that should move together. It stores a title,
description, generated ID, and container status.

## Create and update a release

```sh
arcl release create "Spring release" \
  --description "Ship the next planning slice."
arcl release update arcl-r-01J... --title "Updated spring release"
arcl release update arcl-r-01J... --description-file release-notes.md
```

Use the generated release ID when creating an epic:

```sh
arcl epic create "Keyboard navigation" \
  --spec specs/keyboard-navigation.md \
  --release arcl-r-01J...
```

## Finish or cancel a release

Open releases can be completed or cancelled:

```sh
arcl release complete arcl-r-01J...
arcl release cancel arcl-r-01J...
```

Arc Lightning protects the hierarchy by default. A release cannot become
terminal while a descendant is still open. Pass `--allow-open-children` when
you intentionally want to close only the release:

```sh
arcl release complete arcl-r-01J... --allow-open-children
```

Completion and cancellation are terminal and cannot be changed into one
another. Repeating the same terminal command is safe.
