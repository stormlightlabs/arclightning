---
title: Plan phases from a plan file
description: Write a spec's phases, tasks and blockers in one TOML file, check and apply it, and keep the plan in step as the file changes.
---

## Create the plan

A plan belongs to one spec. `--spec` is required:

```sh
arcl plan create "Offline plan" --spec 3J0G --body-file approach.md
```

```text
Created plan `arcl-pl-01M6Z562G8V506ZE36H9N07BRH`: Offline plan
```

`--input <PATH|->` applies a plan file in the same command, as the next steps
do separately. A spec can have more than one plan.

## Write the plan file

List the phases and their tasks in TOML. Format version `1` is the only one:

```toml
format-version = 1

[[phases]]
key = "keys"
title = "Key handling"
position = 0

[[phases.tasks]]
key = "bindings"
title = "Add the key bindings table"
priority = "high"
position = 0

[[phases.tasks]]
key = "selection"
title = "Move the selection with j and k"
position = 1
blocked-by = ["bindings"]

[[phases.tasks.subtasks]]
key = "tests"
title = "Test the selection keys"
position = 0
```

Every task in the file sits in a phase. Subtasks nest to any depth. A task's
path is its phase key and the keys of its ancestors, joined by `/`, such as
`keys/selection/tests`.

`blocked-by` takes task paths, keys that name exactly one task in the file or
the plan, or full task IDs from anywhere in the project:

```toml
blocked-by = ["keys/selection", "bindings", "arcl-t-01M6Z54428P55ZKSEFQX551WBR"]
```

[Plan file format](/reference/plan-file/) lists every field
and rule.

## Check, diff and apply

All three commands take the plan ID and `--file <PATH|->`:

```sh
arcl plan check Z234 --file plan.toml
arcl plan diff Z234 --file plan.toml
arcl plan apply Z234 --file plan.toml
```

`check` and `diff` write nothing. Each lists every phase, task and dependency
in the file with the change `apply` would make:

```text
Plan `arcl-pl-01M70M8FPNJTRS3HK1EZ7RDQ8G`
phase	keys	Unchanged
phase	docs	Create
task	keys/bindings	Unchanged
task	keys/selection	Unchanged
task	docs/page	Create
dependency	keys/selection	bindings	Unchanged
dependency	docs/page	keys/selection	Create
```

`apply` makes those changes in one transaction:

```text
Applied plan `arcl-pl-01M70M8FPNJTRS3HK1EZ7RDQ8G` (2 phase(s), 3 task(s)).
```

A file that does not parse, names an unknown or ambiguous dependency, or would
add a blocker cycle fails with exit code `3`, and nothing is written:

```text
error: invalid project or record: structured plan input is invalid: task `keys/x` references unknown dependency `nope`
```

A key that matches more than one task is ambiguous. Use its path instead.

## Change the plan

Keep the file in the repository. Edit it and apply it again:

- A path the plan already has is updated: its title, priority, position and
  parent. A task with no `priority` in the file gets `normal`, even after
  `arcl task update` changed it.
- A new path is created.
- A phase or task the file leaves out stays as it is. Removing a line never
  deletes or cancels work, so close it with `arcl task cancel` or
  `arcl plan phase cancel`.
- Apply never changes a task's status.

Applying the same file twice changes nothing the second time.

## Add phases and tasks by hand

Tasks outside any phase, and phases you add without the file, use the CLI:

```sh
arcl plan phase create "Polish" --plan Z234 --position 2
arcl task create "Support custom bindings" --plan Z234
arcl task create "Add focus rings" --phase KSX8
```

```text
Created phase `arcl-ph-01M6Z4X6K39GQSHZVS3W1TJYJH`: Polish
```

`--position` orders phases in the plan, lowest first, and defaults to `0`.
Leave gaps such as 10, 20 and 30 to insert a phase later. Phases with the same
position sort by ID.

A phase made with `plan phase create` has no key, so no plan file matches it.
Applying a file never updates it.

## Read the plan

```sh
arcl plan list
arcl plan show Z234
arcl tree pl-Z234
arcl plan phase list
arcl plan phase show KSX8
```

`plan show` counts the plan's phases, tasks and dependencies; `--json` lists
them. `arcl tree` gives each phase its completed and total task count.

## Update and close phases and plans

```sh
arcl plan update Z234 --title "Keyboard plan, revised" --body-file approach.md
arcl plan phase update TJYJH --title "Final polish" --position 5
arcl plan phase complete KSX8
arcl plan phase cancel TJYJH
arcl plan complete Z234
```

A phase or plan with open work below it refuses to close:

```text
error: invalid project or record: stored phase is invalid: cannot complete phase `arcl-ph-01M6Z4WFW2Y2MPZERE5E1EKSX8` while it has non-terminal descendants; pass --allow-open-children to override
```

`--allow-open-children` closes only the record you name. Its open tasks stay
open but are no longer ready. A closed phase takes no new tasks:

```text
error: invalid project or record: stored planning task is invalid: phase `arcl-ph-01M6Z4X6K39GQSHZVS3W1TJYJH` is `cancelled`; it must be open for this operation
```
