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

Human output from `show`, `list` and `tree` prints short IDs, the shortest
unique end of each ID, such as `GBA5`. Any command accepts one in place of the
full ID, with an optional kind: `arcl show GBA5` or `arcl task start t-GBA5`.
A short ID can become ambiguous as records are added, so write full IDs in
files and links. The [manual](/reference/manual/#record-ids) gives the rule.

## Read the Hierarchy

Render the complete hierarchy or start at one record:

```sh
arcl tree
arcl tree arcl-e-01J...
arcl tree arcl-t-01J...
```

Records use deterministic position and ID ordering. Each line starts with a
status mark:

| Mark | Status                                                                     |
| ---- | -------------------------------------------------------------------------- |
| `✓`  | completed; capture promoted                                                |
| `✗`  | cancelled; capture discarded                                               |
| `◐`  | task in progress; open spec, plan or phase with a started or finished task |
| `‖`  | task parked                                                                |
| `●`  | pending task that is ready                                                 |
| `○`  | pending task that is not ready; other open record; captured capture; note  |

![arcl tree with a status mark on each line](/images/arcl-tree-status.png)

`--plain` and `--json` give the stored status instead.

## Explain readiness

`ready` lists actionable leaf tasks, while `next` returns the first one. When a
task is absent from that list, ask for every applicable reason:

```sh
arcl ready --milestone arcl-m-01J...
arcl next --priority critical high
arcl explain arcl-t-01J...
```

Reasons include incomplete blockers, blockers inherited from ancestor tasks,
cancelled blockers to remove or replace, invalid blocker edges, open children,
parked or terminal ancestors, closed containers, and the task's own lifecycle
state.

## Gather task context

Use `context` before starting or resuming a task:

```sh
arcl context arcl-t-01J...
arcl context arcl-t-01J... --json
```

The output will contain the task, ancestor chain, milestone, epic, optional
release, linked spec path, direct blockers and dependents, readiness reasons,
current handoff, task evidence, and evidence from completed blockers.

Blockers held by an ancestor task block every task under it. `explain` and
`context` list them apart from direct blockers, nearest ancestor first, each
with the ancestor that holds it:

- JSON: `blockers` holds direct blockers only. `inherited_blockers` holds the
  inherited ones, each with an `ancestor` field.
- Plain: `explain` prints `inherited\t<blocker>\t<ancestor>` and `context`
  prints `inherited_blocker\t<blocker>\t<status>\t<ancestor>`. The `blockers`
  and `blocker` records stay direct only.
- Human: inherited blockers end with `via <ancestor>`.

`completion_evidence` draws on both lists and names each blocker once.
Readiness reasons follow the same order: the task's own blockers, then each
ancestor's, nearest first. An ancestor blocked by the task itself does not
block it.

## Check Graph Integrity

```sh
arcl check
```

`check` validates database and graph invariants and reports duplicate sibling
positions as warnings. It exits successfully when no errors are found.

A database can hold blocker edges that `dependency add` refuses, for example
ones edited in by hand. `check` reports each one as an error:

- A cycle, once, with the tasks along it, starting at the smallest task ID:
  `blocker cycle: <a> -> <b> -> <a>; each task is blocked by the next`. A
  task blocked by itself is a cycle of one task: `<a> -> <a>`.
- An edge to the task's own ancestor or descendant, with the parent chain from
  the descendant up to the ancestor:
  ``task `<a>` is blocked by `<b>`, its ancestor; parent chain: <a> -> <b>``.

A task whose own blocker edge is part of one is not ready, and `explain` gives
the same text as a reason. Remove the edge with `dependency remove` to clear
it.
