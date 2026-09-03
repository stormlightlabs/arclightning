---
title: Lifecycle
description: Move tracked work through explicit states without losing hierarchy.
---

Arc Lightning makes lifecycle changes explicit. Tasks can move between open
states, while completion and cancellation are terminal decisions.

## Move tasks through their lifecycle

```sh
arcl task start arcl-t-01J...
arcl task park arcl-t-01J...
arcl task unpark arcl-t-01J...   # always returns to pending
arcl task complete arcl-t-01J...
arcl task cancel arcl-t-01J...
```

Tasks can complete from `pending` or `in_progress`; parked tasks must be unparked first.

Repeating the same terminal command is safe, but changing a completed
task to cancelled or a cancelled task to completed is rejected.

See the [Tasks guide](/guides/tasks/) for the complete status model and the
rules for moving parent tasks and subtasks.

## Finish releases, epics, and milestones

Releases, epics, and milestones support `complete` and `cancel`:

```sh
arcl milestone complete arcl-m-01J...
arcl epic cancel arcl-e-01J... --allow-open-children
arcl release complete arcl-r-01J...
```

A parent task or container cannot become terminal while any descendant is
non-terminal unless the command includes `--allow-open-children`.

## Close only the selected record

The `--allow-open-children` override changes only the selected record. It never
cascades to descendants, so open child work stays available for later review.

See the guides for [releases](/guides/releases/), [epics](/guides/epics/), [milestones](/guides/milestones/),
and [tasks](/guides/tasks/) for entity-specific commands and hierarchy rules.
