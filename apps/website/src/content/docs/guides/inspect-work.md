---
title: Inspecting Work
description: Query the work graph, explain readiness, and gather task context.
---

Arc Lightning can inspect the whole tracker without changing it because typed IDs
use their prefix to select ideas, releases, epics, milestones, tasks, and subtasks.

## Show and list records

Show one record or filter a broader list:

```sh
arcl show arcl-t-01J...
arcl list --kind task --status pending --priority high
arcl list --epic arcl-e-01J...
arcl list --milestone arcl-m-01J... --status pending in_progress
```

`list` accepts `--kind`, `--status`, `--priority`, `--release`, `--epic`,
`--milestone`, and `--parent`. Association filters include the matching
container and work below it where applicable.

## Read the Hierarchy

Render the complete hierarchy or start at one record:

```sh
arcl tree
arcl tree arcl-e-01J...
arcl tree arcl-t-01J...
```

Records use deterministic position and ID ordering. Container and task output
includes descendant progress, so partially completed work is visible without
walking each record separately.

## Explain readiness

`ready` lists actionable leaf tasks, while `next` returns the first one. When a
task is absent from that list, ask for every applicable reason:

```sh
arcl ready --milestone arcl-m-01J...
arcl next --priority critical high
arcl explain arcl-t-01J...
```

Reasons include incomplete blockers, open children, parked or terminal
ancestors, closed containers, and the task's own lifecycle state.

## Gather task context

Use `context` before starting or resuming a task:

```sh
arcl context arcl-t-01J...
arcl context arcl-t-01J... --json
```

The output will contain the task, ancestor chain, milestone, epic, optional
release, linked spec path, direct blockers and dependents, readiness reasons,
current handoff, task evidence, and evidence from completed blockers.

## Check Graph Integrity

```sh
arcl check
```

`check` validates database and graph invariants and reports duplicate sibling
positions as warnings. It exits successfully when no errors are found.
