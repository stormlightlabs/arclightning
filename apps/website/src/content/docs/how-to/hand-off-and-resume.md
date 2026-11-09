---
title: Hand off and resume
description: Park unfinished work with a resume note, then pick it up in a later session from arcl prime and arcl context.
---

## Hand off a task

Run `handoff` on an in-progress task with a note that says where the work
stands and what comes next:

```sh
arcl task handoff PVEA --note "Headings and lists render. Code blocks still lose their language tag; fix that in render.rs next."
```

```text
Handed off task `arcl-t-01M70MA00ZRHB519KSK58YPVEA`: Render a note as Markdown
```

`--note-file <PATH>` reads a longer Markdown note from a file, and
`--note-file -` from standard input. One of `--note` and `--note-file` is
required.

The command stores the note, parks the task and releases its claim in one
transaction. A task keeps one handoff; the next handoff replaces it. With the
workspace on, commit the task file so a session in another clone sees it.

## Find work to resume

`arcl prime` lists parked tasks under `Open work`, each with the first line of
its handoff:

```text
Open work:
- PVEA parked: Render a note as Markdown
  Handoff: Headings and lists render. Code blocks still lose their language tag; fix tha…
```

`arcl list --kind task --status parked` lists every parked task.

## Read the full handoff

```sh
arcl context PVEA
```

```text
Handoff: Headings and lists render. Code blocks still lose their language tag; fix that in render.rs next.
Evidence: none
Note: 2026-11-09T17:04:53.704Z [decision] Render with pulldown-cmark's writer, not string templates
```

The context also carries the spec, plan and phase, blockers, the 10 most
recent task notes, and the evidence of completed blockers.

## Resume the task

Unpark the task, then start it with a claim:

```sh
arcl task unpark PVEA
arcl task start PVEA --claim
```

The handoff stays on the task until another handoff replaces it. If the task
stopped being ready while it was parked, such as when a new blocker was
added, `start` refuses it and lists the reasons. Resolve them, or pass
`--force`.

Finish with `arcl task complete <id> --evidence "<proof>"`, as [Work a task
to done](/how-to/work-a-task/#complete-it) shows.
