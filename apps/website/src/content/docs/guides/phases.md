---
title: Phases
description: Group a plan's tasks into ordered stages, track each stage's progress, and close it when its tasks are done.
---

A phase is an ordered stage inside a plan, such as "Storage" before "Web
app". Phases are optional: a plan can hold tasks directly, and a plan file
creates phases for you. Each phase belongs to one plan and has a title, a
Markdown body and a `position`.

## Create a phase

```sh
arcl plan phase create "Polish" --plan Z234 --position 2
```

```text
Created phase `arcl-ph-01M6Z4X6K39GQSHZVS3W1TJYJH`: Polish
```

`--position` orders phases inside the plan, lowest first, and defaults to `0`.
Positions need not be consecutive, so leave gaps such as 10, 20 and 30 to
insert a phase later. Phases with the same position sort by ID.

A phase created by a [plan file](/guides/plans/#write-a-plan-file) has a key,
and applying the file again updates it. A phase created with
`plan phase create` has none, so a plan file never matches it.

## Put tasks in a phase

```sh
arcl task create "Add focus rings" --phase KSX8
arcl task update 543N --phase YE6M7
```

A task in a phase is also under the phase's plan and spec; `arcl show`
names all three. Moving a task moves its subtasks with it. `--no-phase`
takes a task out of its phase.

## Follow progress

`arcl tree` gives each phase its completed and total task count, counting
subtasks and leaving out cancelled tasks:

```text
◐ Keyboard plan
├── ◐ Key handling  1/3
│   ├── ✓ Add the key bindings table
│   ├── ● Add focus rings
│   └── ◐ Move the selection with j and k
├── ○ Documentation  0/3
│   ├── ○ Document the keyboard shortcuts  blocked by 7TR6
│   └── ○ Show shortcuts in a help overlay
│       └── ● Write tests
├── ✗ Polish
└── ● Support custom bindings
```

Runs of completed phases fold into one line, such as
`✓ Milestones 1–4  done · 20 tasks`, so a long plan stays short. `--all` or
`--style full` unfolds them. [Inspecting work](/guides/inspect-work/) covers
the tree, and `arcl roadmap` prints the same progress as Markdown, as the
[releases guide](/guides/releases/#publish-a-roadmap) shows.

`arcl ready --phase <id>` and `arcl list --phase <id>` narrow work to one
phase.

## Update and close a phase

```sh
arcl plan phase list
arcl plan phase show KSX8
arcl plan phase update TJYJH --title "Final polish" --position 5
arcl plan phase complete KSX8
arcl plan phase cancel TJYJH
```

A phase with open tasks refuses to complete or cancel:

```text
error: invalid project or record: stored phase is invalid: cannot complete phase `arcl-ph-01M6Z4WFW2Y2MPZERE5E1EKSX8` while it has non-terminal descendants; pass --allow-open-children to override
```

`--allow-open-children` closes the phase alone. Its open tasks stay open but
are no longer ready. A closed phase takes no new tasks:

```text
error: invalid project or record: stored planning task is invalid: phase `arcl-ph-01M6Z4X6K39GQSHZVS3W1TJYJH` is `cancelled`; it must be open for this operation
```

[`arcl compact`](/guides/compaction/) archives a phase's old completed tasks
into one note per phase, which keeps long-lived plans fast to read.
