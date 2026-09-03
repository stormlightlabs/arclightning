---
title: Milestones
description: Order implementation stages inside an epic.
---

Milestones divide an epic into ordered implementation stages. Each milestone
belongs to one epic and has a non-negative integer `position` used for display
ordering.

## Create a milestone

```sh
arcl milestone create "Foundation" \
  --epic arcl-e-01J... \
  --position 10 \
  --description "Lay down the storage and command boundaries."
```

Position values do not need to be consecutive. Leaving out `--position` uses
`0`, which is useful when the order is not important yet.

## Update a milestone

```sh
arcl milestone update arcl-m-01J... --title "Storage foundation"
arcl milestone update arcl-m-01J... --position 20
arcl milestone update arcl-m-01J... --description-file milestone.md
```

Updating a milestone changes its tracker metadata only.

## Finish or cancel a milestone

```sh
arcl milestone complete arcl-m-01J...
arcl milestone cancel arcl-m-01J...
```

Arc Lightning refuses to complete or cancel a milestone while it has
non-terminal task descendants. Pass `--allow-open-children` to close only the
milestone when that is intentional:

```sh
arcl milestone complete arcl-m-01J... --allow-open-children
```
