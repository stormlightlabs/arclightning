---
title: Plan file format
description: The TOML plan file that plan check, plan diff, plan apply and plan create --input read, with its fields, keys, paths and matching rules.
---

`arcl plan check`, `plan diff`, `plan apply` and `plan create --input` read a
plan file. The `POST /api/plans/<id>/apply` endpoint and the web app's
**Apply plan** take the same TOML as `document`.

## Tables and fields

```toml
format-version = 1

[[phases]]
key = "keys"
title = "Key handling"
position = 0

[[phases.tasks]]
key = "selection"
title = "Move the selection with j and k"
priority = "high"
position = 1
blocked-by = ["bindings"]

[[phases.tasks.subtasks]]
key = "tests"
title = "Test the selection keys"
position = 0
```

| Table              | Fields                                                           |
| ------------------ | ---------------------------------------------------------------- |
| top level          | `format-version`, which must be `1`, and `phases`                |
| `[[phases]]`       | `key`, `title`, `position`, and its `tasks`                      |
| `[[phases.tasks]]` | `key`, `title`, `position`, optional `priority` and `blocked-by` |
| `[[...subtasks]]`  | The same fields as a task, nested to any depth                   |

`priority` is `critical`, `high`, `normal` or `low`. An unknown field fails
the check.

Every task in a plan file sits in a phase. Tasks directly under the plan come
from `arcl task create --plan`.

## Keys and paths

Every key and title must be non-empty. Phase keys are unique in the file, and
task keys are unique among siblings.

A task's path is its phase key and the keys of its ancestors, joined by `/`,
such as `keys/selection/tests`. The path is stored on the task as `plan-key`,
and `--json` task output carries it as `plan_key`.

## Blockers

Each `blocked-by` entry is one of:

- a task path, such as `keys/selection`;
- a key that names exactly one task in the file or the plan, such as
  `bindings`;
- a full task ID from anywhere in the project.

A key that matches more than one task fails as ambiguous. An entry that
matches no task fails as unknown.

The edge rules in [Dependencies and readiness
rules](/reference/deps/#refused-edges) apply. Errors name tasks the plan would
create by their plan path.

## How apply matches records

Apply matches phases and tasks by key path:

| The file has                    | Apply                                             |
| ------------------------------- | ------------------------------------------------- |
| a path the plan already has     | Updates the title, priority, position and parent. |
| a path the plan does not have   | Creates the phase or task.                        |
| no entry for an existing record | Leaves the record as it is.                       |
| a task with no `priority`       | Sets `normal`, whatever the task has.             |

Apply never changes a status, never deletes, and never cancels. A phase made
with `arcl plan phase create`, or a task made with `arcl task create`, has no
key, so no file matches it. Applying the same file twice changes nothing the
second time.

## Errors

A file that does not parse, names an unknown or ambiguous dependency, or adds
a refused edge fails with exit code `3`. `plan check` and `plan diff` report
the same errors as `plan apply`, and none of the three writes on failure:

```text
error: invalid project or record: structured plan input is invalid: task `keys/x` references unknown dependency `nope`
```

The manual's [plan commands](/reference/manual/#plan-commands) give the output
of `check` and `diff` in each mode.
