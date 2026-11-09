---
title: Plans
description: Describe how a spec will be built, and create or update its phases, tasks and blockers from one TOML file.
---

A plan is a persistent description of how a spec will be built. It belongs to
one spec, has a title and a Markdown body, and holds tasks directly or in
ordered [phases](/guides/phases/). A spec can have more than one plan.

## Create a plan

```sh
arcl plan create "Offline plan" --spec 3J0G --body-file approach.md
```

```text
Created plan `arcl-pl-01M6Z562G8V506ZE36H9N07BRH`: Offline plan
```

`--spec` is required. `--input <PATH|->` applies a plan file right after
creating the plan, in the same command; `--no-input`, the default, creates
the plan alone. After that, add phases with `arcl plan phase create` and tasks
with `arcl task create --plan` or `--phase`, or describe them all in a file.

## Write a plan file

A plan file lists phases and their tasks in TOML. Every task in the file sits
in a phase; add tasks outside phases with `arcl task create --plan`. Format
version `1` is the only one:

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

| Table              | Fields                                                           |
| ------------------ | ---------------------------------------------------------------- |
| `[[phases]]`       | `key`, `title`, `position`, and its `tasks`                      |
| `[[phases.tasks]]` | `key`, `title`, `position`, optional `priority` and `blocked-by` |
| `[[...subtasks]]`  | The same fields as a task, nested to any depth                   |

Every key and title must be non-empty. Phase keys are unique in the file, and
task keys are unique among siblings. A task's path is its phase key and the
keys of its ancestors, joined by `/`, such as `keys/selection/tests`. The
path is stored on the task as `plan-key`. An unknown field fails the check.

`blocked-by` lists the tasks that must complete first. Each entry is a task
path, a key that names exactly one task in the file or the plan, or a full
task ID from anywhere in the project:

```toml
blocked-by = ["keys/selection", "bindings", "arcl-t-01M6Z54428P55ZKSEFQX551WBR"]
```

A key that matches more than one task fails as ambiguous. Use its path.

## Check, diff and apply

All three commands take the plan ID and `--file <PATH|->`:

```sh
arcl plan check Z234 --file plan.toml
arcl plan diff Z234 --file plan.toml
arcl plan apply Z234 --file plan.toml
```

`check` and `diff` write nothing. Each lists every phase, task and dependency
in the file with the change `apply` would make to it:

```text
Plan `arcl-pl-01M6Z4WFW1XXC6XZ81Y9F6Z234`
phase	keys	Unchanged
phase	docs	Unchanged
task	keys/bindings	Unchanged
task	keys/selection	Unchanged
task	docs/shortcuts-page	Update
task	docs/help-overlay	Create
dependency	keys/selection	bindings	Unchanged
dependency	docs/shortcuts-page	keys/selection	Unchanged
```

`--plain` drops the `Plan` line and writes the change in lowercase, such as
`create`; `--json` uses the lowercase names in each entry's `change`.

`apply` makes those changes in one transaction:

```text
Applied plan `arcl-pl-01M6Z4WFW1XXC6XZ81Y9F6Z234` (2 phase(s), 4 task(s)).
```

Apply matches phases and tasks by key path. A path the plan already has is
updated: its title, priority, position and parent. A task with no `priority`
in the file gets `normal`, even if it was changed with `arcl task update`. A
new path is created. A
phase or task that the file leaves out is left as it is, so removing a line
from the file never deletes or cancels work. Close it with
`arcl task cancel` or `arcl plan phase cancel` instead. Apply never changes a
task's status.

A file that does not parse, names an unknown or ambiguous dependency, or would
add a blocker cycle fails the check with exit code `3`, and `apply` writes
nothing:

```text
error: invalid project or record: structured plan input is invalid: task `keys/x` references unknown dependency `nope`
```

Applying the same file twice changes nothing the second time. Keep the file
in the repository and apply it again after each edit to keep the plan in step.

## Read and update a plan

```sh
arcl plan list
arcl plan show Z234
arcl plan update Z234 --title "Keyboard plan, revised" --body-file approach.md
arcl tree pl-Z234
```

`plan show` counts the plan's phases, tasks and dependencies; `--json` lists
them. `arcl tree <plan-id>` draws the plan with its phases and tasks.

## Close a plan

```sh
arcl plan complete Z234
arcl plan cancel Z234
```

A plan with open phases or tasks refuses to close unless you pass
`--allow-open-children`, which closes the plan alone. The
[lifecycle guide](/guides/lifecycle/) has the rules.
